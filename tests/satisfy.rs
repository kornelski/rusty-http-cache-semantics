use chrono::offset::Utc;
use chrono::Duration;
use http::{header, Method, Request, Response};
use http_cache_semantics::CacheOptions;

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

#[test]
fn test_when_urls_match() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );

    let policy =
        CacheOptions::default().policy_for(&request_parts(Request::builder().uri("/")), &response);

    assert!(
        policy.is_cached_response_fresh(&mut request_parts(Request::builder().uri("/")), &response)
    );
}

#[test]
fn test_when_expires_is_present() {
    let two_seconds_later = Utc::now()
        .checked_add_signed(Duration::seconds(2))
        .unwrap()
        .to_rfc3339();
    let response = &response_parts(
        Response::builder()
            .status(302)
            .header(header::EXPIRES, two_seconds_later),
    );

    let policy = CacheOptions::default().policy_for(&request_parts(Request::builder()), &response);

    assert!(policy.is_cached_response_fresh(&mut request_parts(Request::builder()), &response));
}

#[test]
fn test_not_when_urls_mismatch() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CacheOptions::default()
        .policy_for(&request_parts(Request::builder().uri("/foo")), &response);

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().uri("/foo?bar")),
        &response,
    ));
}

#[test]
fn test_when_methods_match() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::GET)),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().method(Method::GET)),
        &response,
    ));
}

#[test]
fn test_not_when_hosts_mismatch() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().header(header::HOST, "foo")),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::HOST, "foo")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::HOST, "foofoo")),
        &response,
    ));
}

#[test]
fn test_when_methods_match_head() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::HEAD)),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().method(Method::HEAD)),
        &response,
    ));
}

#[test]
fn test_not_when_methods_mismatch() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::POST)),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().method(Method::GET)),
        &response,
    ));
}

#[test]
fn test_not_when_methods_mismatch_head() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2"),
    );
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().method(Method::HEAD)),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().method(Method::GET)),
        &response,
    ));
}

#[test]
fn test_not_when_proxy_revalidating() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2, proxy-revalidate "),
    );
    let policy = CacheOptions::default().policy_for(&request_parts(Request::builder()), &response);

    assert!(!policy.is_cached_response_fresh(&mut request_parts(Request::builder()), &response));
}

#[test]
fn test_when_not_a_proxy_revalidating() {
    let response = &response_parts(
        Response::builder()
            .status(200)
            .header(header::CACHE_CONTROL, "max-age=2, proxy-revalidate "),
    );
    let policy =
        CacheOptions::new_unshared().policy_for(&request_parts(Request::builder()), &response);

    assert!(policy.is_cached_response_fresh(&mut request_parts(Request::builder()), &response));
}

#[test]
fn test_not_when_no_cache_requesting() {
    let response = &response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=2"));
    let policy = CacheOptions::default().policy_for(&request_parts(Request::builder()), &response);

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "fine")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "no-cache")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::PRAGMA, "no-cache")),
        &response,
    ));
}
