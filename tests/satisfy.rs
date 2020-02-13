use chrono::offset::Utc;
use chrono::Duration;
use http::{header, Method, Request, Response};
use http_cache_semantics::CachePolicy;
use http_cache_semantics::CachePolicyOptions;
use std::time::SystemTime;

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

#[test]
fn test_when_urls_match() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().uri("/")),
        response,
        Default::default(),
    );

    assert!(policy
        .before_request(&mut request_parts(Request::builder().uri("/")), now)
        .satisfies_without_revalidation());
}

#[test]
fn test_when_expires_is_present() {
    let now = SystemTime::now();
    let two_seconds_later = Utc::now()
        .checked_add_signed(Duration::seconds(2))
        .unwrap()
        .to_rfc2822();
    let response = &response_parts(
        Response::builder()
            .status(302)
            .header(header::EXPIRES, two_seconds_later),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder()),
        response,
        Default::default(),
    );

    assert!(policy
        .before_request(&mut request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn test_when_methods_match() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::GET)),
        response,
        Default::default(),
    );

    assert!(policy
        .before_request(&request_parts(Request::builder().method(Method::GET)), now)
        .satisfies_without_revalidation());
}

#[test]
fn test_not_when_hosts_mismatch() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder().header(header::HOST, "foo")),
        response,
        Default::default(),
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
fn test_when_methods_match_head() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::HEAD)),
        response,
        Default::default(),
    );

    assert!(policy
        .before_request(&request_parts(Request::builder().method(Method::HEAD)), now)
        .satisfies_without_revalidation());
}

#[test]
fn test_not_when_proxy_revalidating() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2, proxy-revalidate "),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder()),
        response,
        Default::default(),
    );

    assert!(!policy
        .before_request(&mut request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn test_when_not_a_proxy_revalidating() {
    let now = SystemTime::now();
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2, proxy-revalidate "),
    );
    let policy = CachePolicy::new(
        &request_parts(Request::builder()),
        response,
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );

    assert!(policy
        .before_request(&mut request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn test_not_when_no_cache_requesting() {
    let now = SystemTime::now();
    let response = &response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=2"));
    let policy = CachePolicy::new(
        &request_parts(Request::builder()),
        response,
        Default::default(),
    );

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
