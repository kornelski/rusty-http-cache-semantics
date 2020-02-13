use chrono::{Duration, Utc};
use http::{header, Method, Request, Response};
use http_cache_semantics::CachePolicy;
use http_cache_semantics::CacheOptions;
use std::time::SystemTime;

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

#[test]
fn test_simple_miss() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(Response::builder()),
        Default::default(),
    );

    assert!(policy.is_stale(now));
}

#[test]
fn test_simple_hit() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "public, max-age=999999"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 999999);
}

#[test]
fn test_quoted_syntax() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "  max-age = \"678\"      "),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 678);
}

#[test]
fn test_iis() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "private, public, max-age=259200"),
        ),
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );

    assert!(!policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 259200);
}

#[test]
fn test_pre_check_tolerated() {
    let now = SystemTime::now();
    let cache_control = "pre-check=0, post-check=0, no-store, no-cache, max-age=100";

    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, cache_control)),
        Default::default(),
    );

    assert!(policy.is_stale(now));
    assert!(!policy.is_storable());
    assert_eq!(policy.max_age().as_secs(), 0);
    assert_eq!(
        get_cached_response(
            &policy,
            &request_parts(Request::builder().header("cache-control", "max-stale")),
            now
        )
        .headers[header::CACHE_CONTROL.as_str()],
        cache_control
    );
}

#[test]
fn test_pre_check_poison() {
    let now = SystemTime::now();
    let original_cache_control =
        "pre-check=0, post-check=0, no-cache, no-store, max-age=100, custom, foo=bar";

    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, original_cache_control)
                .header(header::PRAGMA, "no-cache"),
        ),
        CacheOptions {
            ignore_cargo_cult: true,
            ..Default::default()
        },
    );

    assert!(!policy.is_stale(now));
    assert!(policy.is_storable());
    assert_eq!(policy.max_age().as_secs(), 100);

    let res = get_cached_response(&policy, &request_parts(Request::builder()), now);
    let cache_control_header = &res.headers[header::CACHE_CONTROL.as_str()];
    assert!(!cache_control_header.to_str().unwrap().contains("pre-check"));
    assert!(!cache_control_header
        .to_str()
        .unwrap()
        .contains("post-check"));
    assert!(!cache_control_header.to_str().unwrap().contains("no-store"));

    assert!(cache_control_header
        .to_str()
        .unwrap()
        .contains("max-age=100"));
    assert!(cache_control_header.to_str().unwrap().contains("custom"));
    assert!(cache_control_header.to_str().unwrap().contains("foo=bar"));

    assert!(!res.headers.contains_key(header::PRAGMA.as_str()));
}

#[test]
fn test_age_can_make_stale() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "max-age=100")
                .header(header::AGE, "101"),
        ),
        Default::default(),
    );

    assert!(policy.is_stale(now));
    assert!(policy.is_storable());
}

#[test]
fn test_age_not_always_stale() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "max-age=20")
                .header(header::AGE, "15"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert!(policy.is_storable());
}

#[test]
fn test_bogus_age_ignored() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "max-age=20")
                .header(header::AGE, "golden"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert!(policy.is_storable());
}

#[test]
fn test_cache_old_files() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, Utc::now().to_rfc2822())
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert!(policy.max_age().as_secs() > 100);
}

#[test]
fn test_immutable_simple_hit() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "immutable, max-age=999999"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 999999);
}

#[test]
fn test_immutable_can_expire() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, "immutable, max-age=0")),
        Default::default(),
    );

    assert!(policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_cache_immutable_files() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, Utc::now().to_rfc2822())
                .header(header::CACHE_CONTROL, "immutable")
                .header(header::LAST_MODIFIED, Utc::now().to_rfc2822()),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert!(policy.max_age().as_secs() > 100);
}

#[test]
fn test_immutable_can_be_off() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, Utc::now().to_rfc2822())
                .header(header::CACHE_CONTROL, "immutable")
                .header(header::LAST_MODIFIED, Utc::now().to_rfc2822()),
        ),
        CacheOptions {
            immutable_min_time_to_live: std::time::Duration::from_secs(0),
            ..Default::default()
        },
    );

    assert!(policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_pragma_no_cache() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::PRAGMA, "no-cache")
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
        Default::default(),
    );

    assert!(policy.is_stale(now));
}

#[test]
fn test_blank_cache_control_and_pragma_no_cache() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "")
                .header(header::PRAGMA, "no-cache")
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
}

#[test]
fn test_no_store() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "no-store, public, max-age=1"),
        ),
        Default::default(),
    );

    assert!(policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_observe_private_cache() {
    let now = SystemTime::now();
    let private_header = "private, max-age=1234";

    let request = request_parts(Request::builder().method(Method::GET));
    let response =
        response_parts(Response::builder().header(header::CACHE_CONTROL, private_header));

    let shared_policy = CachePolicy::new(&request, &response, Default::default());

    let unshared_policy = CachePolicy::new(
        &request,
        &response,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );

    assert!(shared_policy.is_stale(now));
    assert_eq!(shared_policy.max_age().as_secs(), 0);
    assert!(!unshared_policy.is_stale(now));
    assert_eq!(unshared_policy.max_age().as_secs(), 1234);
}

#[test]
fn test_do_not_share_cookies() {
    let now = SystemTime::now();
    let request = request_parts(Request::builder().method(Method::GET));
    let response = response_parts(
        Response::builder()
            .header(header::SET_COOKIE, "foo=bar")
            .header(header::CACHE_CONTROL, "max-age=99"),
    );

    let shared_policy = CachePolicy::new(&request, &response, Default::default());

    let unshared_policy = CachePolicy::new(
        &request,
        &response,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );

    assert!(shared_policy.is_stale(now));
    assert_eq!(shared_policy.max_age().as_secs(), 0);
    assert!(!unshared_policy.is_stale(now));
    assert_eq!(unshared_policy.max_age().as_secs(), 99);
}

#[test]
fn test_do_share_cookies_if_immutable() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::SET_COOKIE, "foo=bar")
                .header(header::CACHE_CONTROL, "immutable, max-age=99"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 99);
}

#[test]
fn test_cache_explicitly_public_cookie() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::SET_COOKIE, "foo=bar")
                .header(header::CACHE_CONTROL, "max-age=5, public"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 5);
}

#[test]
fn test_miss_max_age_equals_zero() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, "public, max-age=0")),
        Default::default(),
    );

    assert!(policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_uncacheable_503() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(503)
                .header(header::CACHE_CONTROL, "public, max-age=0"),
        ),
        Default::default(),
    );

    assert!(policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_cacheable_301() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(301)
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
}

#[test]
fn test_uncacheable_303() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(303)
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
        Default::default(),
    );

    assert!(policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_cacheable_303() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(303)
                .header(header::CACHE_CONTROL, "max-age=1000"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
}

#[test]
fn test_uncacheable_412() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(412)
                .header(header::CACHE_CONTROL, "public, max-age=1000"),
        ),
        Default::default(),
    );

    assert!(policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_expired_expires_cache_with_max_age() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public, max-age=9999")
                .header(header::EXPIRES, "Sat, 07 May 2016 15:35:18 GMT"),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 9999);
}

#[test]
fn test_expired_expires_cached_with_s_maxage() {
    let now = SystemTime::now();
    let request = request_parts(Request::builder().method(Method::GET));
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public, s-maxage=9999")
            .header(header::EXPIRES, "Sat, 07 May 2016 15:35:18 GMT"),
    );

    let shared_policy = CachePolicy::new(&request, &response, Default::default());

    let unshared_policy = CachePolicy::new(
        &request,
        &response,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );

    assert!(!shared_policy.is_stale(now));
    assert_eq!(shared_policy.max_age().as_secs(), 9999);
    assert!(unshared_policy.is_stale(now));
    assert_eq!(unshared_policy.max_age().as_secs(), 0);
}

#[test]
fn test_max_age_wins_over_future_expires() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public, max-age=333")
                .header(
                    header::EXPIRES,
                    Utc::now()
                        .checked_add_signed(Duration::hours(1))
                        .unwrap()
                        .to_rfc2822(),
                ),
        ),
        Default::default(),
    );

    assert!(!policy.is_stale(now));
    assert_eq!(policy.max_age().as_secs(), 333);
}

fn get_cached_response(
    policy: &CachePolicy,
    req: &impl http_cache_semantics::RequestLike,
    now: SystemTime,
) -> http::response::Parts {
    match policy.before_request(req, now) {
        http_cache_semantics::BeforeRequest::Fresh(res) => res,
        _ => panic!("stale"),
    }
}
