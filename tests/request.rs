use http::{header, Method, Request, Response};
use http_cache_semantics::CacheOptions;
use http_cache_semantics::CachePolicy;
use std::time::SystemTime;

fn public_cacheable_response() -> http::response::Parts {
    response_parts(Response::builder().header(header::CACHE_CONTROL, "public, max-age=222"))
}

fn cacheable_response() -> http::response::Parts {
    response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=111"))
}

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

#[test]
fn test_no_store_kills_cache() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::CACHE_CONTROL, "no-store"),
        ),
        &public_cacheable_response(),
    );

    assert!(policy.is_stale(now));
    assert!(!policy.is_storable());
}

#[test]
fn test_post_not_cacheable_by_default() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::POST)),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, "public")),
    );

    assert!(policy.is_stale(now));
    assert!(!policy.is_storable());
}

#[test]
fn test_post_cacheable_explicitly() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder().method(Method::POST)),
        &public_cacheable_response(),
    );

    assert!(!policy.is_stale(now));
    assert!(policy.is_storable());
}

#[test]
fn test_public_cacheable_auth_is_ok() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::AUTHORIZATION, "test"),
        ),
        &public_cacheable_response(),
    );

    assert!(!policy.is_stale(now));
    assert!(policy.is_storable());
}

#[test]
fn test_private_auth_is_ok() {
    let now = SystemTime::now();
    let policy = CachePolicy::new_options(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::AUTHORIZATION, "test"),
        ),
        &cacheable_response(),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );

    assert!(!policy.is_stale(now));
    assert!(policy.is_storable());
}

#[test]
fn test_revalidate_auth_is_ok() {
    let policy = CachePolicy::new(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::AUTHORIZATION, "test"),
        ),
        &response_parts(
            Response::builder().header(header::CACHE_CONTROL, "max-age=88,must-revalidate"),
        ),
    );

    assert!(policy.is_storable());
}

#[test]
fn test_auth_prevents_caching_by_default() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::AUTHORIZATION, "test"),
        ),
        &cacheable_response(),
    );

    assert!(policy.is_stale(now));
    assert_eq!(policy.is_storable(), false);
}
