use http::{header, Method, Request, Response};
use http_cache_semantics::CacheOptions;
use http_cache_semantics::CachePolicy;
use std::time::SystemTime;
use time::format_description::well_known::Rfc2822;
use time::Duration;
use time::OffsetDateTime;

use crate::private_opts;
use crate::req_cache_control;
use crate::request_parts;
use crate::response_parts;
use crate::Harness;

fn now_rfc2822() -> String {
    OffsetDateTime::now_utc().format(&Rfc2822).unwrap()
}

#[test]
fn simple_miss() {
    Harness::default()
        .stale_and_store()
        .test_with_response(response_parts(Response::builder()));
}

#[test]
fn simple_hit() {
    Harness::default()
        .assert_time_to_live(999999)
        .test_with_cache_control("public, max-age=999999");
}

#[test]
fn quoted_syntax() {
    Harness::default()
        .assert_time_to_live(678)
        .test_with_cache_control("  max-age = \"678\"      ");
}

#[test]
fn iis() {
    Harness::default()
        .assert_time_to_live(259200)
        .options(private_opts())
        .test_with_cache_control("private, public, max-age=259200");
}

#[test]
fn pre_check_tolerated() {
    let now = SystemTime::now();
    let cache_control = "pre-check=0, post-check=0, no-store, no-cache, max-age=100";
    let policy = Harness::default()
        .no_store()
        .time(now)
        .test_with_cache_control(cache_control);

    assert_eq!(
        get_cached_response(
            &policy,
            &req_cache_control("max-stale"),
            now
        )
        .headers[header::CACHE_CONTROL],
        cache_control
    );
}

#[test]
fn pre_check_poison() {
    let now = SystemTime::now();
    let original_cache_control =
        "pre-check=0, post-check=0, no-cache, no-store, max-age=100, custom, foo=bar";
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, original_cache_control)
            .header(header::PRAGMA, "no-cache"),
    );

    let policy = Harness::default()
        .assert_time_to_live(100)
        .time(now)
        .options(CacheOptions {
            ignore_cargo_cult: true,
            ..Default::default()
        })
        .test_with_response(response);

    let res = get_cached_response(&policy, &request_parts(Request::builder()), now);
    let cache_control_header = &res.headers[header::CACHE_CONTROL].to_str().unwrap();
    assert!(!cache_control_header.contains("pre-check"));
    assert!(!cache_control_header.contains("post-check"));
    assert!(!cache_control_header.contains("no-store"));

    assert!(cache_control_header.contains("max-age=100"));
    assert!(cache_control_header.contains("custom"));
    assert!(cache_control_header.contains("foo=bar"));

    assert!(!res.headers.contains_key(header::PRAGMA));
}

#[test]
fn age_can_make_stale() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=100")
            .header(header::AGE, "101"),
    );
    Harness::default()
        .stale_and_store()
        .test_with_response(response);
}

#[test]
fn age_not_always_stale() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=20")
            .header(header::AGE, "15"),
    );
    Harness::default()
        .test_with_response(response);
}

#[test]
fn bogus_age_ignored() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=20")
            .header(header::AGE, "golden"),
    );
    Harness::default()
        .test_with_response(response);
}

#[test]
fn cache_old_files() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::DATE, now_rfc2822())
            .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
    );
    let policy = Harness::default()
        .time(now)
        .test_with_response(response);
    assert!(policy.time_to_live(now).as_secs() > 100);
}

#[test]
fn immutable_simple_hit() {
    Harness::default()
        .assert_time_to_live(999999)
        .test_with_cache_control("immutable, max-age=999999");
}

#[test]
fn immutable_can_expire() {
    Harness::default()
        .stale_and_store()
        .test_with_cache_control("immutable, max-age=0");
}

#[test]
fn cache_immutable_files() {
    let response = response_parts(
        Response::builder()
            .header(header::DATE, now_rfc2822())
            .header(header::CACHE_CONTROL, "immutable")
            .header(header::LAST_MODIFIED, now_rfc2822()),
    );
    Harness::default()
        .assert_time_to_live(CacheOptions::default().immutable_min_time_to_live.as_secs())
        .test_with_response(response);
}

#[test]
fn immutable_can_be_off() {
    let response = response_parts(
        Response::builder()
            .header(header::DATE, now_rfc2822())
            .header(header::CACHE_CONTROL, "immutable")
            .header(header::LAST_MODIFIED, now_rfc2822()),
    );
    Harness::default()
        .stale_and_store()
        .options(CacheOptions {
            immutable_min_time_to_live: std::time::Duration::from_secs(0),
            ..Default::default()
        })
        .test_with_response(response);
}

#[test]
fn pragma_no_cache() {
    let response = response_parts(
        Response::builder()
            .header(header::PRAGMA, "no-cache")
            .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
    );
    Harness::default()
        .stale_and_store()
        .test_with_response(response);
}

#[test]
fn blank_cache_control_and_pragma_no_cache() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "")
            .header(header::PRAGMA, "no-cache")
            .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
    );
    Harness::default()
        .test_with_response(response);
}

#[test]
fn no_store() {
    Harness::default()
        .no_store()
        .test_with_cache_control("no-store, public, max-age=1");
}

#[test]
fn observe_private_cache() {
    let private_header = "private, max-age=1234";
    let response =
        response_parts(Response::builder().header(header::CACHE_CONTROL, private_header));

    let _shared = Harness::default()
        .no_store()
        .test_with_response(response.clone());

    let _private = Harness::default()
        .assert_time_to_live(1234)
        .options(private_opts())
    .test_with_response(response);
}

#[test]
fn do_not_share_cookies() {
    let response = response_parts(
        Response::builder()
            .header(header::SET_COOKIE, "foo=bar")
            .header(header::CACHE_CONTROL, "max-age=99"),
    );

    let _shared = Harness::default()
        .stale_and_store()
        .test_with_response(response.clone());

    let _private = Harness::default()
        .assert_time_to_live(99)
        .options(private_opts())
        .test_with_response(response);
}

#[test]
fn do_share_cookies_if_immutable() {
    let response = response_parts(
        Response::builder()
            .header(header::SET_COOKIE, "foo=bar")
            .header(header::CACHE_CONTROL, "immutable, max-age=99"),
    );
    Harness::default()
        .assert_time_to_live(99)
        .test_with_response(response);
}

#[test]
fn cache_explicitly_public_cookie() {
    let response = response_parts(
        Response::builder()
            .header(header::SET_COOKIE, "foo=bar")
            .header(header::CACHE_CONTROL, "max-age=5, public"),
    );
    Harness::default()
        .assert_time_to_live(5)
        .test_with_response(response);
}

#[test]
fn miss_max_age_equals_zero() {
    Harness::default()
        .stale_and_store()
        .test_with_cache_control("public, max-age=0");
}

#[test]
fn uncacheable_503() {
    let response = response_parts(
        Response::builder()
            .status(503)
            .header(header::CACHE_CONTROL, "public, max-age=0"),
    );
    Harness::default()
        .no_store()
        .test_with_response(response);
}

#[test]
fn cacheable_301() {
    let response = response_parts(
        Response::builder()
            .status(301)
            .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
    );
    Harness::default().test_with_response(response);
}

#[test]
fn uncacheable_303() {
    let response = response_parts(
        Response::builder()
            .status(303)
            .header(header::LAST_MODIFIED, "Mon, 07 Mar 2016 11:52:56 GMT"),
    );
    Harness::default()
        .no_store()
        .test_with_response(response);
}

#[test]
fn cacheable_303() {
    let response = response_parts(
        Response::builder()
            .status(303)
            .header(header::CACHE_CONTROL, "max-age=1000"),
    );
    Harness::default().test_with_response(response);
}

#[test]
fn uncacheable_412() {
    let response = response_parts(
        Response::builder()
            .status(412)
            .header(header::CACHE_CONTROL, "public, max-age=1000"),
    );
    Harness::default()
        .no_store()
        .test_with_response(response);
}

#[test]
fn expired_expires_cache_with_max_age() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public, max-age=9999")
            .header(header::EXPIRES, "Sat, 07 May 2016 15:35:18 GMT"),
    );
    Harness::default()
        .assert_time_to_live(9999)
        .test_with_response(response);
}

#[test]
fn request_mismatches() {
    let now = SystemTime::now();
    let mut req = request_parts(Request::builder().uri("/test"));
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public, max-age=9999")
            .header(header::EXPIRES, "Sat, 07 May 2016 15:35:18 GMT"),
    );
    let policy = Harness::default()
        .time(now)
        .request(req.clone())
        .test_with_response(response);

    req.method = Method::POST;
    let mismatch = policy.before_request(&req, now);
    assert!(matches!(mismatch, http_cache_semantics::BeforeRequest::Stale {matches, ..} if !matches));
}

#[test]
fn request_matches() {
    let now = SystemTime::now();
    let req = request_parts(Request::builder().uri("/test"));
    let policy = Harness::default()
        .stale_and_store()
        .time(now)
        .request(req.clone())
        .test_with_cache_control("public, max-age=0");

    let mismatch = policy.before_request(&req, now);
    assert!(matches!(mismatch, http_cache_semantics::BeforeRequest::Stale {matches, ..} if matches));
}

#[test]
fn expired_expires_cached_with_s_maxage() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public, s-maxage=9999")
            .header(header::EXPIRES, "Sat, 07 May 2016 15:35:18 GMT"),
    );

    let _shared = Harness::default()
        .assert_time_to_live(9999)
        .time(now)
        .test_with_response(response.clone());

    let _private = Harness::default()
        .stale_and_store()
        .time(now)
        .options(private_opts())
        .test_with_response(response);
}

#[test]
fn max_age_wins_over_future_expires() {
    let in_one_hour = OffsetDateTime::now_utc()
        .checked_add(Duration::hours(1))
        .unwrap()
        .format(&Rfc2822)
        .unwrap();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public, max-age=333")
            .header(header::EXPIRES, in_one_hour),
    );
    Harness::default()
        .assert_time_to_live(333)
        .test_with_response(response);
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
