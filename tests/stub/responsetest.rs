use http::*;
use http_cache_semantics::*;
use std::time::Duration;
use std::time::SystemTime;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

use crate::private_opts;
use crate::request_parts;
use crate::Harness;

macro_rules! headers(
    { $($key:tt : $value:expr),* $(,)? } => {
        {
            let mut m = Response::builder();
            $(
                m = m.header($key, $value);
            )+
            m.body(()).unwrap()
        }
     };
);

fn req() -> request::Parts {
    Request::get("http://test.example.com/").body(()).unwrap().into_parts().0
}

fn harness() -> Harness {
    Harness::default()
        .request(req())
}

#[test]
fn simple_miss() {
    harness()
        .stale_and_store()
        .test_with_response(Response::new(()));
}

#[test]
fn simple_hit() {
    harness()
        .assert_time_to_live(999999)
        .test_with_cache_control("public, max-age=999999");
}

#[test]
fn weird_syntax() {
    harness()
        .assert_time_to_live(456)
        .test_with_cache_control(",,,,max-age =  456      ,");
}

#[test]
fn quoted_syntax() {
    harness()
        .assert_time_to_live(678)
        .test_with_cache_control("  max-age = \"678\"      ");
}

#[test]
fn iis() {
    harness()
        .assert_time_to_live(259200)
        .options(private_opts())
        .test_with_cache_control("private, public, max-age=259200");
}

#[test]
fn pre_check_tolerated() {
    let now = SystemTime::now();
    let cc = "pre-check=0, post-check=0, no-store, no-cache, max-age=100";
    let cache = harness()
        .no_store()
        .time(now)
        .test_with_cache_control(cc);
    let mut later_req = req();
    later_req.headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("max-stale"));
    assert_eq!(
        get_cached_response(
            &cache,
            &later_req,
            now
        )
        .headers()[header::CACHE_CONTROL],
        cc
    );
}

#[test]
fn pre_check_poison() {
    let now = SystemTime::now();
    let orig_cc = "pre-check=0, post-check=0, no-cache, no-store, max-age=100, custom, foo=bar";
    let res = &headers! { "cache-control": orig_cc, "pragma": "no-cache"};
    let options = CacheOptions {
        ignore_cargo_cult: true,
        ..Default::default()
    };
    let cache = harness()
        .assert_time_to_live(100)
        .time(now)
        .options(options)
        .test_with_response(res.clone());

    let cc = get_cached_response(&cache, &req(), now);
    let cc = cc.headers()[header::CACHE_CONTROL].to_str().unwrap();
    assert!(!cc.contains("pre-check"));
    assert!(!cc.contains("post-check"));
    assert!(!cc.contains("no-store"));

    assert!(cc.contains("max-age=100"));
    assert!(cc.contains(", custom") || cc.contains("custom, "));
    assert!(cc.contains("foo=bar"));

    assert!(get_cached_response(
        &cache,
        &request_parts(
            Request::get("http://test.example.com/")
                .header(header::CACHE_CONTROL, "max-stale")
        ),
        now
    )
    .headers()
    .get(header::PRAGMA)
    .is_none());
}

#[test]
fn pre_check_poison_undefined_header() {
    let now = SystemTime::now();
    let orig_cc = "pre-check=0, post-check=0, no-cache, no-store";
    let options = CacheOptions {
        ignore_cargo_cult: true,
        ..Default::default()
    };
    let cache = harness()
        .stale_and_store()
        .options(options)
        .time(now)
        .test_with_response(headers! { "cache-control": orig_cc, "expires": "yesterday!"});

    let res = &get_cached_response(
        &cache,
        &Request::get("http://test.example.com/")
            .header("cache-control", "max-stale")
            .body(())
            .unwrap(),
        now,
    );
    let _cc = &res.headers()[header::CACHE_CONTROL];

    assert!(res.headers().get(header::EXPIRES).is_none());
}

#[test]
fn cache_with_expires() {
    let now = SystemTime::now();
    let response = headers! {
        "date": date_str(now),
        "expires": date_str(now + Duration::from_millis(2001)),
    };
    harness()
        .assert_time_to_live(2)
        .test_with_response(response);
}

#[test]
fn cache_with_expires_relative_to_date() {
    let now = SystemTime::now();
    let response = headers! {
        "date": date_str(now - Duration::from_secs(30)),
        "expires": date_str(now),
    };
    harness()
        .assert_time_to_live(30)
        .time(now)
        .test_with_response(response);
}

#[test]
fn cache_with_expires_always_relative_to_date() {
    let now = SystemTime::now();
    let response = headers! {
        "date": date_str(now - Duration::from_secs(3)),
        "expires": date_str(now),
    };
    harness()
        .assert_time_to_live(3)
        .time(now)
        .test_with_response(response);
}

#[test]
fn cache_expires_no_date() {
    let now = SystemTime::now();
    let response = headers! {
        "cache-control": "public",
        "expires": date_str(now + Duration::from_secs(3600)),
    };
    let cache = harness()
        .time(now)
        .test_with_response(response);
    assert!(cache.time_to_live(now).as_secs() > 3595);
    assert!(cache.time_to_live(now).as_secs() < 3605);
}

#[test]
fn ages() {
    let mut now = SystemTime::now();
    let response = headers! {
        "cache-control": "max-age=100",
        "age": "50",
    };
    let cache = harness()
        .assert_time_to_live(50)
        .time(now)
        .test_with_response(response);

    now += Duration::from_secs(48);
    assert_eq!(2, cache.time_to_live(now).as_secs());
    assert!(!cache.is_stale(now));

    now += Duration::from_secs(5);
    assert!(cache.is_stale(now));
    assert_eq!(0, cache.time_to_live(now).as_secs());
}

#[test]
fn age_can_make_stale() {
    let response = headers! {
        "cache-control": "max-age=100",
        "age": "101",
    };
    harness()
        .stale_and_store()
        .test_with_response(response);
}

#[test]
fn age_not_always_stale() {
    let response = headers! {
        "cache-control": "max-age=20",
        "age": "15",
    };
    harness().test_with_response(response);
}

#[test]
fn bogus_age_ignored() {
    let response = headers! {
        "cache-control": "max-age=20",
        "age": "golden",
    };
    harness().test_with_response(response);
}

#[test]
fn cache_old_files() {
    let now = SystemTime::now();
    let response = headers! {
        "date": date_str(now),
        "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
    };
    let policy = harness().time(now).test_with_response(response);
    assert!(policy.time_to_live(now).as_secs() > 100);
}

#[test]
fn immutable_simple_hit() {
    harness()
        .assert_time_to_live(999999)
        .test_with_cache_control("immutable, max-age=999999");
}

#[test]
fn immutable_can_expire() {
    harness()
        .stale_and_store()
        .test_with_cache_control("immutable, max-age=0");
}

#[test]
fn cache_immutable_files() {
    let now = SystemTime::now();
    let response = headers! {
        "date": date_str(now),
        "cache-control": "immutable",
        "last-modified": date_str(now),
    };
    let policy = harness().time(now).test_with_response(response);
    assert!(policy.time_to_live(now).as_secs() > 100);
}

#[test]
fn immutable_can_be_off() {
    let now = SystemTime::now();
    let response = headers! {
        "date": date_str(now),
        "cache-control": "immutable",
        "last-modified": date_str(now),
    };
    harness()
        .stale_and_store()
        .time(now)
        .options(CacheOptions {
            immutable_min_time_to_live: Duration::from_secs(0),
            ..Default::default()
        })
        .test_with_response(response);
}

#[test]
fn pragma_no_cache() {
    let response = headers! {
        "pragma": "no-cache",
        "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
    };
    harness().stale_and_store().test_with_response(response);
}

#[test]
fn blank_cache_control_and_pragma_no_cache() {
    let now = SystemTime::now();
    let response = headers! {
        "cache-control": "",
        "pragma": "no-cache",
        "last-modified": date_str(now - Duration::from_secs(10)),
    };
    harness().time(now).test_with_response(response);
}

#[test]
fn no_store() {
    harness()
        .no_store()
        .test_with_cache_control("no-store, public, max-age=1");
}

#[test]
fn observe_private_cache() {
    let cc = "private, max-age=1234";
    harness()
        .no_store()
        .test_with_cache_control(cc);
    harness()
        .assert_time_to_live(1234)
        .options(private_opts())
        .test_with_cache_control(cc);
}

#[test]
fn don_t_share_cookies() {
    let response = headers! {
        "set-cookie": "foo=bar",
        "cache-control": "max-age=99",
    };

    let _proxy = harness()
        .stale_and_store()
        .test_with_response(response.clone());
    let _ua = harness()
        .assert_time_to_live(99)
        .options(private_opts())
        .test_with_response(response);
}

#[test]
fn do_share_cookies_if_immutable() {
    let response = headers! {
        "set-cookie": "foo=bar",
        "cache-control": "immutable, max-age=99",
    };
    harness().assert_time_to_live(99).test_with_response(response);
}

#[test]
fn cache_explicitly_public_cookie() {
    let response = headers! {
        "set-cookie": "foo=bar",
        "cache-control": "max-age=5, public",
    };
    harness().assert_time_to_live(5).test_with_response(response);
}

#[test]
fn miss_max_age_0() {
    harness()
        .stale_and_store()
        .test_with_cache_control("public, max-age=0");
}

#[test]
fn uncacheable_503_service_unavailable() {
    let mut res = headers! { "cache-control": "public, max-age=1000" };
    *res.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
    harness().no_store().test_with_response(res);
}

#[test]
fn cacheable_301_moved_permanently() {
    let mut res = headers! { "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT", };
    *res.status_mut() = StatusCode::MOVED_PERMANENTLY;
    harness().test_with_response(res);
}

#[test]
fn uncacheable_303_see_other() {
    let mut res = headers! { "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT", };
    *res.status_mut() = StatusCode::SEE_OTHER;
    harness().no_store().test_with_response(res);
}

#[test]
fn cacheable_303_see_other() {
    let mut res = headers! { "cache-control": "max-age=1000", };
    *res.status_mut() = StatusCode::SEE_OTHER;
    harness().test_with_response(res);
}

#[test]
fn uncacheable_412_precondition_failed() {
    let mut res = headers! { "cache-control": "public, max-age=1000", };
    *res.status_mut() = StatusCode::PRECONDITION_FAILED;
    harness().no_store().test_with_response(res);
}

#[test]
fn expired_expires_cached_with_max_age() {
    let response = headers! {
        "cache-control": "public, max-age=9999",
        "expires": "Sat, 07 May 2016 15:35:18 GMT",
    };
    harness().assert_time_to_live(9999).test_with_response(response);
}

#[test]
fn expired_expires_cached_with_s_maxage() {
    let response = headers! {
        "cache-control": "public, s-maxage=9999",
        "expires": "Sat, 07 May 2016 15:35:18 GMT",
    };
    let _proxy = harness().assert_time_to_live(9999).test_with_response(response.clone());
    let _ua = harness()
        .stale_and_store()
        .options(private_opts())
        .test_with_response(response);
}

#[test]
fn max_age_wins_over_future_expires() {
    let now = SystemTime::now();
    let response = headers! {
        "cache-control": "public, max-age=333",
        "expires": date_str(now + Duration::from_secs(3600)),
    };
    harness().assert_time_to_live(333).time(now).test_with_response(response);
}

#[test]
fn remove_hop_headers() {
    let mut now = SystemTime::now();
    let res = headers! {
        "te": "deflate",
        "date": "now",
        "custom": "header",
        "oompa": "lumpa",
        "connection": "close, oompa, header",
        "age": "10",
        "cache-control": "public, max-age=333",
    };
    let cache = harness().time(now).test_with_response(res.clone());

    now += Duration::from_millis(1005);
    let h = get_cached_response(&cache, &req(), now);
    let h = h.headers();
    assert!(!h.contains_key("connection"));
    assert!(!h.contains_key("te"));
    assert!(!h.contains_key("oompa"));
    assert_eq!(h["cache-control"], "public, max-age=333");
    assert_ne!(
        h["date"],
        "now",
        "updated age requires updated date"
    );
    assert_eq!(h["custom"].to_str().unwrap(), "header");
    assert_eq!(h["age"].to_str().unwrap(), "11");
}

fn date_str(now: SystemTime) -> String {
    let timestamp = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let date = OffsetDateTime::from_unix_timestamp(timestamp as i64).unwrap();
    date.format(&Rfc2822).unwrap()
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
