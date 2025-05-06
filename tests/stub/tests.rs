//! Determines whether a given HTTP response can be cached and whether a
//! cached response can be reused, following the rules specified in [RFC
//! 7234](https://httpwg.org/specs/rfc7234.html).

use http::header;
use http::header::HeaderValue;
use http::Method;
use http::Request;
use http::Response;
use http_cache_semantics::*;
use std::time::SystemTime;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

use crate::private_opts;
use crate::request_parts;
use crate::resp_cache_control;

fn assert_cached(should_put: bool, response_code: u16) {
    let now = SystemTime::now();
    let mut response = Response::builder()
        .status(response_code)
        .header(header::LAST_MODIFIED, format_date(-105, 1))
        .header(header::EXPIRES, format_date(1, 3600))
        .header(header::WWW_AUTHENTICATE, "challenge");

    if 407 == response_code {
        response = response.header(header::PROXY_AUTHENTICATE, "Basic realm=\"protected area\"");
    } else if 401 == response_code {
        response
            .headers_mut()
            .unwrap()
            .insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"protected area\""),
            );
    }

    let policy = CachePolicy::new_options(
        &Request::get("http://example.com").body(()).unwrap(),
        &response.body(()).unwrap(),
        now,
        private_opts(),
    );

    assert_eq!(
        should_put,
        policy.is_storable(),
        "{should_put}; {response_code}; {policy:#?}"
    );
}

#[test]
fn ok_http_response_caching_by_response_code() {
    assert_cached(false, 100);
    assert_cached(false, 101);
    assert_cached(false, 102);
    assert_cached(true, 200);
    assert_cached(false, 201);
    assert_cached(false, 202);
    assert_cached(true, 203);
    assert_cached(true, 204);
    assert_cached(false, 205);
    // 206: electing to not cache partial responses
    assert_cached(false, 206);
    assert_cached(false, 207);
    assert_cached(true, 300);
    assert_cached(true, 301);
    assert_cached(true, 302);
    assert_cached(false, 304);
    assert_cached(false, 305);
    assert_cached(false, 306);
    assert_cached(true, 307);
    assert_cached(true, 308);
    assert_cached(false, 400);
    assert_cached(false, 401);
    assert_cached(false, 402);
    assert_cached(false, 403);
    assert_cached(true, 404);
    assert_cached(true, 405);
    assert_cached(false, 406);
    assert_cached(false, 408);
    assert_cached(false, 409);
    // 410: the HTTP spec permits caching 410s, but the RI doesn't
    assert_cached(true, 410);
    assert_cached(false, 411);
    assert_cached(false, 412);
    assert_cached(false, 413);
    assert_cached(true, 414);
    assert_cached(false, 415);
    assert_cached(false, 416);
    assert_cached(false, 417);
    assert_cached(false, 418);
    assert_cached(false, 429);
    assert_cached(false, 500);
    assert_cached(true, 501);
    assert_cached(false, 502);
    assert_cached(false, 503);
    assert_cached(false, 504);
    assert_cached(false, 505);
    assert_cached(false, 506);
}

fn format_date(delta: i64, unit: i64) -> String {
    let now = OffsetDateTime::now_utc();
    let timestamp = now.unix_timestamp() + delta * unit;

    let date = OffsetDateTime::from_unix_timestamp(timestamp).unwrap();
    date.format(&Rfc2822).unwrap()
}

#[test]
fn proxy_cacheable_auth_is_ok() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().header(header::AUTHORIZATION, "test")),
        &resp_cache_control("max-age=0,s-maxage=12"),
    );

    assert!(!policy.is_stale(now));
    assert!(policy.is_storable());

    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&policy).unwrap();
        let policy: CachePolicy = serde_json::from_str(&json).unwrap();

        assert!(!policy.is_stale(now));
        assert!(policy.is_storable());
    }
}

#[test]
fn not_when_urls_mismatch() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().uri("/foo")),
        &resp_cache_control("max-age=2"),
    );

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().uri("/foo?bar")),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn not_when_methods_mismatch() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::POST)),
        &resp_cache_control("max-age=2"),
    );

    assert!(!policy
        .before_request(
            &Request::new(()),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn not_when_methods_mismatch_head() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::HEAD)),
        &resp_cache_control("max-age=2"),
    );

    assert!(
        !policy
            .before_request(
                &request_parts(Request::builder()),
                now
            )
            .satisfies_without_revalidation()
    );
}
