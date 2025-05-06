use http::{header, Method, Request, Response};
use http_cache_semantics::CachePolicy;
use std::time::SystemTime;
use time::format_description::well_known::Rfc2822;
use time::Duration;
use time::OffsetDateTime;

use crate::private_opts;
use crate::request_parts;
use crate::response_parts;

#[test]
fn when_urls_match() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );

    let policy = CachePolicy::new(&request_parts(Request::builder()), response);

    assert!(policy
        .before_request(&mut request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn when_expires_is_present() {
    let now = SystemTime::now();
    let two_seconds_later = OffsetDateTime::now_utc()
        .checked_add(Duration::seconds(2))
        .unwrap()
        .format(&Rfc2822)
        .unwrap();
    let response = &response_parts(
        Response::builder()
            .status(302)
            .header(header::EXPIRES, two_seconds_later),
    );

    let policy = CachePolicy::new(&request_parts(Request::builder()), response);

    assert!(policy
        .before_request(&mut request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn when_methods_match() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder()),
        response,
    );

    assert!(policy
        .before_request(&request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn must_revalidate_allows_not_revalidating_fresh() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=200, must-revalidate"),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        response,
    );

    assert!(policy
        .before_request(&request_parts(Request::builder().method(Method::GET)), now)
        .satisfies_without_revalidation());

    let later = now + std::time::Duration::from_secs(300);
    assert!(!policy
        .before_request(
            &request_parts(Request::builder().method(Method::GET)),
            later
        )
        .satisfies_without_revalidation());
}

#[test]
fn must_revalidate_disallows_stale() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=200, must-revalidate"),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        response,
    );

    let later = now + std::time::Duration::from_secs(300);
    assert!(!policy
        .before_request(
            &request_parts(Request::builder().method(Method::GET)),
            later
        )
        .satisfies_without_revalidation());

    let later = now + std::time::Duration::from_secs(300);
    assert!(!policy
        .before_request(
            &request_parts(
                Request::builder()
                    .header("cache-control", "max-stale")
                    .method(Method::GET)
            ),
            later
        )
        .satisfies_without_revalidation());
}

#[test]
fn not_when_hosts_mismatch() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder().header(header::HOST, "foo")),
        response,
    );

    assert!(policy
        .before_request(
            &request_parts(Request::builder().header(header::HOST, "foo")),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header(header::HOST, "foofoo")),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn when_methods_match_head() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::HEAD)),
        response,
    );

    assert!(policy
        .before_request(&request_parts(Request::builder().method(Method::HEAD)), now)
        .satisfies_without_revalidation());
}

#[test]
fn not_when_proxy_revalidating() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=2, proxy-revalidate "),
    );
    let policy = CachePolicy::new(&request_parts(Request::builder()), response);

    assert!(!policy
        .before_request(&request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn when_not_a_proxy_revalidating() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=2, proxy-revalidate "),
    );
    let policy = CachePolicy::new_options(
        &request_parts(Request::builder()),
        response,
        now,
        private_opts(),
    );

    assert!(policy
        .before_request(&request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn not_when_no_cache_requesting() {
    let now = SystemTime::now();
    let response = &response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=2"));
    let policy = CachePolicy::new(&request_parts(Request::builder()), response);

    assert!(policy
        .before_request(
            &request_parts(Request::builder().header(header::CACHE_CONTROL, "fine")),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header(header::CACHE_CONTROL, "no-cache")),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header(header::PRAGMA, "no-cache")),
            now
        )
        .satisfies_without_revalidation());
}
