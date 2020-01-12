use chrono::{Duration, Utc};
use http::{header, Method, Request, Response};
use http_cache_semantics::CacheOptions;

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

#[test]
fn test_simple_miss() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(Response::builder()),
    );

    assert!(policy.is_stale());
}

#[test]
fn test_simple_hit() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "public, max-age=999999"),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 999999);
}

/*
#[test]
fn test_weird_syntax() {
    let policy = CachePolicy::new(
        json!({
            "method": "GET",
            "headers": {},
        }),
        json!({
            "cache-control": ",,,,max-age =  456      ,"
        }),
    );

    assert_eq!(policy.is_stale(), false);
    assert_eq!(policy.max_age(), 456);

    let policy_two = CachePolicy::from_object(HashMap::new());
    // TODO: assert(cache2 instanceof CachePolicy);

    assert_eq!(policy_two.is_stale(), false);
    assert_eq!(policy_two.max_age(), 456);
}
*/

#[test]
fn test_quoted_syntax() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "  max-age = \"678\"      "),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 678);
}

#[test]
fn test_iis() {
    let policy = CacheOptions::new_unshared().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "private, public, max-age=259200"),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 259200);
}

#[test]
fn test_pre_check_tolerated() {
    let cache_control = "pre-check=0, post-check=0, no-store, no-cache, max-age=100";

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, cache_control)),
    );

    assert!(policy.is_stale());
    assert!(!policy.is_storable());
    assert_eq!(policy.max_age(), 0);
    assert_eq!(
        policy.response_headers()[header::CACHE_CONTROL.as_str()],
        cache_control
    );
}

#[test]
fn test_pre_check_poison() {
    let original_cache_control =
        "pre-check=0, post-check=0, no-cache, no-store, max-age=100, custom, foo=bar";

    let policy = CacheOptions::new_with_ignore_cargo_cult_option(true).policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, original_cache_control)
                .header(header::PRAGMA, "no-cache"),
        ),
    );

    assert!(!policy.is_stale());
    assert!(policy.is_storable());
    assert_eq!(policy.max_age(), 100);

    let cache_control_header = &policy.response_headers()[header::CACHE_CONTROL.as_str()];
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

    assert!(!policy
        .response_headers()
        .contains_key(header::PRAGMA.as_str()));
}

#[test]
fn test_pre_check_poison_undefined_header() {
    let original_cache_control = "pre-check=0, post-check=0, no-cache, no-store";

    let policy = CacheOptions::new_with_ignore_cargo_cult_option(true).policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, original_cache_control)
                .header(header::EXPIRES, "yesterday!"),
        ),
    );

    assert!(policy.is_stale());
    assert!(policy.is_storable());
    assert_eq!(policy.max_age(), 0);

    assert!(!policy
        .response_headers()
        .contains_key(header::CACHE_CONTROL.as_str()));
    assert!(!policy
        .response_headers()
        .contains_key(header::EXPIRES.as_str()));
}

#[test]
fn test_cache_with_expires() {
    let now = Utc::now();
    let two_seconds_later = Utc::now().checked_add_signed(Duration::seconds(2)).unwrap();

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, now.to_rfc3339())
                .header(header::EXPIRES, two_seconds_later.to_rfc3339()),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 2);
}

#[test]
fn test_cache_with_expires_relative_to_date() {
    let now = Utc::now();
    let three_seconds_ago = Utc::now().checked_sub_signed(Duration::seconds(3)).unwrap();

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, three_seconds_ago.to_rfc3339())
                .header(header::EXPIRES, now.to_rfc3339()),
        ),
    );

    assert_eq!(policy.max_age(), 3);
}

#[test]
fn test_cache_with_expires_always_relative_to_date() {
    let now = Utc::now();
    let three_seconds_ago = Utc::now().checked_sub_signed(Duration::seconds(3)).unwrap();

    let policy = CacheOptions::new_with_trust_server_date_option(false).policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, three_seconds_ago.to_rfc3339())
                .header(header::EXPIRES, now.to_rfc3339()),
        ),
    );

    assert_eq!(policy.max_age(), 3);
}

#[test]
fn test_cache_expires_no_date() {
    let one_hour_later = Utc::now().checked_add_signed(Duration::hours(1)).unwrap();

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public")
                .header(header::EXPIRES, one_hour_later.to_rfc3339()),
        ),
    );

    assert!(!policy.is_stale());
    assert!(policy.max_age() > 3595);
    assert!(policy.max_age() < 3605);
}

/*
#[test]
fn test_ages() {
    // TODO: Need to figure out how "subclassing" works in Rust
    // Link to function in JS: https://github.com/kornelski/http-cache-semantics/blob/master/test/responsetest.js#L158
    assert!(false);
}
*/

#[test]
fn test_age_can_make_stale() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "max-age=100")
                .header(header::AGE, "101"),
        ),
    );

    assert!(policy.is_stale());
    assert!(policy.is_storable());
}

#[test]
fn test_age_not_always_stale() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "max-age=20")
                .header(header::AGE, "15"),
        ),
    );

    assert!(!policy.is_stale());
    assert!(policy.is_storable());
}

#[test]
fn test_bogus_age_ignored() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "max-age=20")
                .header(header::AGE, "golden"),
        ),
    );

    assert!(!policy.is_stale());
    assert!(policy.is_storable());
}

#[test]
fn test_cache_old_files() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, Utc::now().to_rfc3339())
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
    );

    assert!(!policy.is_stale());
    assert!(policy.max_age() > 100);
}

#[test]
fn test_immutable_simple_hit() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "immutable, max-age=999999"),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 999999);
}

#[test]
fn test_immutable_can_expire() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, "immutable, max-age=0")),
    );

    assert!(policy.is_stale());
    assert_eq!(policy.max_age(), 0);
}

#[test]
fn test_cache_immutable_files() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, Utc::now().to_rfc3339())
                .header(header::CACHE_CONTROL, "immutable")
                .header(header::LAST_MODIFIED, Utc::now().to_rfc3339()),
        ),
    );

    assert!(!policy.is_stale());
    assert!(policy.max_age() > 100);
}

#[test]
fn test_immutable_can_be_off() {
    let policy = CacheOptions::new_with_immutable_min_time_to_live_option(0).policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::DATE, Utc::now().to_rfc3339())
                .header(header::CACHE_CONTROL, "immutable")
                .header(header::LAST_MODIFIED, Utc::now().to_rfc3339()),
        ),
    );

    assert!(policy.is_stale());
    assert_eq!(policy.max_age(), 0);
}

#[test]
fn test_pragma_no_cache() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::PRAGMA, "no-cache")
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
    );

    assert!(policy.is_stale());
}

#[test]
fn test_blank_cache_control_and_pragma_no_cache() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "")
                .header(header::PRAGMA, "no-cache")
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
    );

    assert!(!policy.is_stale());
}

#[test]
fn test_no_store() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "no-store, public, max-age=1"),
        ),
    );

    assert!(policy.is_stale());
    assert_eq!(policy.max_age(), 0);
}

#[test]
fn test_observe_private_cache() {
    let private_header = "private, max-age=1234";

    let request = request_parts(Request::builder().method(Method::GET));
    let response =
        response_parts(Response::builder().header(header::CACHE_CONTROL, private_header));

    let shared_policy = CacheOptions::default().policy_for(&request, &response);

    let unshared_policy = CacheOptions::new_unshared().policy_for(&request, &response);

    assert!(shared_policy.is_stale());
    assert_eq!(shared_policy.max_age(), 0);
    assert!(!unshared_policy.is_stale());
    assert_eq!(unshared_policy.max_age(), 1234);
}

#[test]
fn test_do_not_share_cookies() {
    let request = request_parts(Request::builder().method(Method::GET));
    let response = response_parts(
        Response::builder()
            .header(header::SET_COOKIE, "foo=bar")
            .header(header::CACHE_CONTROL, "max-age=99"),
    );

    let shared_policy = CacheOptions::default().policy_for(&request, &response);

    let unshared_policy = CacheOptions::new_unshared().policy_for(&request, &response);

    assert!(shared_policy.is_stale());
    assert_eq!(shared_policy.max_age(), 0);
    assert!(!unshared_policy.is_stale());
    assert_eq!(unshared_policy.max_age(), 99);
}

#[test]
fn test_do_share_cookies_if_immutable() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::SET_COOKIE, "foo=bar")
                .header(header::CACHE_CONTROL, "immutable, max-age=99"),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 99);
}

#[test]
fn test_cache_explicitly_public_cookie() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::SET_COOKIE, "foo=bar")
                .header(header::CACHE_CONTROL, "max-age=5, public"),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 5);
}

#[test]
fn test_miss_max_age_equals_zero() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, "public, max-age=0")),
    );

    assert!(policy.is_stale());
    assert_eq!(policy.max_age(), 0);
}

#[test]
fn test_uncacheable_503() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(503)
                .header(header::CACHE_CONTROL, "public, max-age=0"),
        ),
    );

    assert!(policy.is_stale());
    assert_eq!(policy.max_age(), 0);
}

#[test]
fn test_cacheable_301() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(301)
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
    );

    assert!(!policy.is_stale());
}

#[test]
fn test_uncacheable_303() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(303)
                .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
        ),
    );

    assert!(policy.is_stale());
    assert_eq!(policy.max_age(), 0);
}

#[test]
fn test_cacheable_303() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(303)
                .header(header::CACHE_CONTROL, "max-age=1000"),
        ),
    );

    assert!(!policy.is_stale());
}

#[test]
fn test_uncacheable_412() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .status(412)
                .header(header::CACHE_CONTROL, "public, max-age=1000"),
        ),
    );

    assert!(policy.is_stale());
    assert_eq!(policy.max_age(), 0);
}

#[test]
fn test_expired_expires_cache_with_max_age() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public, max-age=9999")
                .header(header::EXPIRES, "Sat, 07 May 2016 15:35:18 GMT"),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 9999);
}

#[test]
fn test_expired_expires_cached_with_s_maxage() {
    let request = request_parts(Request::builder().method(Method::GET));
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public, s-maxage=9999")
            .header(header::EXPIRES, "Sat, 07 May 2016 15:35:18 GMT"),
    );

    let shared_policy = CacheOptions::default().policy_for(&request, &response);

    let unshared_policy = CacheOptions::new_unshared().policy_for(&request, &response);

    assert!(!shared_policy.is_stale());
    assert_eq!(shared_policy.max_age(), 9999);
    assert!(unshared_policy.is_stale());
    assert_eq!(unshared_policy.max_age(), 0);
}

#[test]
fn test_max_age_wins_over_future_expires() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public, max-age=333")
                .header(
                    header::EXPIRES,
                    Utc::now()
                        .checked_add_signed(Duration::hours(1))
                        .unwrap()
                        .to_rfc3339(),
                ),
        ),
    );

    assert!(!policy.is_stale());
    assert_eq!(policy.max_age(), 333);
}

/*
#[test]
fn test_remove_hop_headers() {
    // TODO: Need to figure out how "subclassing" works in Rust
    // Link to JavaScript function: https://github.com/kornelski/http-cache-semantics/blob/master/test/responsetest.js#L472
}
*/
