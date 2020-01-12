use http::{header, Method, Request, Response};
use http_cache_semantics::CacheOptions;

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
    let policy = CacheOptions::default().policy_for(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::CACHE_CONTROL, "no-store"),
        ),
        &public_cacheable_response(),
    );

    assert!(policy.is_stale());
    assert!(!policy.is_storable());
}

#[test]
fn test_post_not_cacheable_by_default() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::POST)),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, "public")),
    );

    assert!(policy.is_stale());
    assert!(!policy.is_storable());
}

#[test]
fn test_post_cacheable_explicitly() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::POST)),
        &public_cacheable_response(),
    );

    assert!(!policy.is_stale());
    assert!(policy.is_storable());
}

#[test]
fn test_public_cacheable_auth_is_ok() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::AUTHORIZATION, "test"),
        ),
        &public_cacheable_response(),
    );

    assert!(!policy.is_stale());
    assert!(policy.is_storable());
}

/*
#[test]
fn test_proxy_cacheable_auth_is_ok() {
    let policy = CachePolicy::new(
        json!({
            "method": "GET",
            "headers": {
                "authorization": "test",
            }
        }),
        json!({
            "headers": {
                "cache-control": "max-age=0,s-maxage=12",
            }
        }),
    );

    assert_eq!(policy.is_stale(), false);
    assert_eq!(policy.is_storable(), true);

    let policy_two = CachePolicy::from_object(HashMap::new());
    // TODO: assert(cache2 instanceof CachePolicy);

    assert_eq!(!policy_two.is_stale(), true);
    assert_eq!(policy_two.is_storable(), true);
}
*/

#[test]
fn test_private_auth_is_ok() {
    let policy = CacheOptions::new_unshared().policy_for(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::AUTHORIZATION, "test"),
        ),
        &cacheable_response(),
    );

    assert!(!policy.is_stale());
    assert!(policy.is_storable());
}

#[test]
fn test_revalidate_auth_is_ok() {
    let policy = CacheOptions::default().policy_for(
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
    let policy = CacheOptions::default().policy_for(
        &request_parts(
            Request::builder()
                .method(Method::GET)
                .header(header::AUTHORIZATION, "test"),
        ),
        &cacheable_response(),
    );

    assert_eq!(policy.is_stale(), true);
    assert_eq!(policy.is_storable(), false);
}
