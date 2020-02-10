use http::{header, HeaderMap, HeaderValue, Method, Request, Response};
use http_cache_semantics::CachePolicy;
use std::time::SystemTime;

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn simple_request() -> http::request::Parts {
    request_parts(simple_request_builder())
}

fn simple_request_builder() -> http::request::Builder {
    Request::builder()
        .method(Method::GET)
        .header(header::HOST, "www.w3c.org")
        .header(header::CONNECTION, "close")
        .header("x-custom", "yes")
        .uri("/Protocols/rfc2616/rfc2616-sec14.html")
}

fn cacheable_response_builder() -> http::response::Builder {
    Response::builder().header(header::CACHE_CONTROL, cacheable_header())
}

fn simple_request_with_etagged_response() -> CachePolicy {
    CachePolicy::new(
        &simple_request(),
        &response_parts(cacheable_response_builder().header(header::ETAG, etag_value())),
        Default::default(),
    )
}

fn simple_request_with_cacheable_response() -> CachePolicy {
    CachePolicy::new(
        &simple_request(),
        &response_parts(cacheable_response_builder()),
        Default::default(),
    )
}

fn simple_request_with_always_variable_response() -> CachePolicy {
    CachePolicy::new(
        &simple_request(),
        &response_parts(cacheable_response_builder().header(header::VARY, "*")),
        Default::default(),
    )
}

fn etag_value() -> &'static str {
    "\"123456789\""
}

fn cacheable_header() -> &'static str {
    "max-age=111"
}

fn very_old_date() -> &'static str {
    "Tue, 15 Nov 1994 12:45:26 GMT"
}

fn assert_headers_passed(headers: &HeaderMap<HeaderValue>) {
    assert!(!headers.contains_key(header::CONNECTION));
    assert_eq!(headers.get("x-custom").unwrap(), "yes");
}

fn assert_no_validators(headers: &HeaderMap<HeaderValue>) {
    assert!(!headers.contains_key(header::IF_NONE_MATCH));
    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}

#[test]
fn test_ok_if_method_changes_to_head() {
    let policy = simple_request_with_etagged_response();

    let headers = policy
        .revalidation_request(&mut request_parts(
            simple_request_builder().method(Method::HEAD),
        ))
        .headers;

    assert_headers_passed(&headers);
    assert_eq!(headers.get(header::IF_NONE_MATCH).unwrap(), "\"123456789\"");
}

#[test]
fn test_not_if_method_mismatch_other_than_head() {
    let policy = simple_request_with_etagged_response();

    let incoming_request = request_parts(simple_request_builder().method(Method::POST));
    let headers = policy.revalidation_request(&incoming_request).headers;

    assert_headers_passed(&headers);
    assert_no_validators(&headers);
}

#[test]
fn test_not_if_url_mismatch() {
    let policy = simple_request_with_etagged_response();

    let incoming_request = request_parts(simple_request_builder().uri("/yomomma"));
    let headers = policy.revalidation_request(&incoming_request).headers;

    assert_headers_passed(&headers);
    assert_no_validators(&headers);
}

#[test]
fn test_not_if_host_mismatch() {
    let policy = simple_request_with_etagged_response();

    let mut incoming_request = request_parts(simple_request_builder());
    incoming_request
        .headers
        .insert(header::HOST, "www.w4c.org".parse().unwrap());
    let headers = policy.revalidation_request(dbg!(&incoming_request)).headers;

    assert_no_validators(&headers);
    assert!(headers.contains_key("x-custom"));
}

#[test]
fn test_not_if_vary_fields_prevent() {
    let policy = simple_request_with_always_variable_response();

    let headers = policy.revalidation_request(&simple_request()).headers;

    assert_headers_passed(&headers);
    assert_no_validators(&headers);
}

#[test]
fn test_when_entity_tag_validator_is_present() {
    let policy = simple_request_with_etagged_response();

    let headers = policy.revalidation_request(&simple_request()).headers;

    assert_headers_passed(&headers);
    assert_eq!(headers.get(header::IF_NONE_MATCH).unwrap(), "\"123456789\"");
}

#[test]
fn test_skips_weak_validators_on_post() {
    let mut post_request = request_parts(
        simple_request_builder()
            .method(Method::POST)
            .header(header::IF_NONE_MATCH, "W/\"weak\", \"strong\", W/\"weak2\""),
    );
    let policy = CachePolicy::new(
        &post_request,
        &response_parts(
            cacheable_response_builder()
                .header(header::LAST_MODIFIED, very_old_date())
                .header(header::ETAG, etag_value()),
        ),
        Default::default(),
    );

    let headers = policy.revalidation_request(&mut post_request).headers;

    assert_eq!(
        headers.get(header::IF_NONE_MATCH).unwrap(),
        "\"strong\", \"123456789\""
    );
    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}

#[test]
fn test_skips_weak_validators_on_post_2() {
    let mut post_request = request_parts(
        simple_request_builder()
            .method(Method::POST)
            .header(header::IF_NONE_MATCH, "W/\"weak\""),
    );
    let policy = CachePolicy::new(
        &post_request,
        &response_parts(
            cacheable_response_builder().header(header::LAST_MODIFIED, very_old_date()),
        ),
        Default::default(),
    );

    let headers = policy.revalidation_request(&mut post_request).headers;

    assert!(!headers.contains_key(header::IF_NONE_MATCH));
    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}

#[test]
fn test_merges_validators() {
    let mut post_request = request_parts(
        simple_request_builder()
            .header(header::IF_NONE_MATCH, "W/\"weak\", \"strong\", W/\"weak2\""),
    );
    let policy = CachePolicy::new(
        &post_request,
        &response_parts(
            cacheable_response_builder()
                .header(header::LAST_MODIFIED, very_old_date())
                .header(header::ETAG, etag_value()),
        ),
        Default::default(),
    );

    let headers = policy.revalidation_request(&mut post_request).headers;

    assert_eq!(
        headers.get(header::IF_NONE_MATCH).unwrap(),
        "W/\"weak\", \"strong\", W/\"weak2\", \"123456789\""
    );
    assert_eq!(
        headers.get(header::IF_MODIFIED_SINCE).unwrap(),
        very_old_date()
    );
}

#[test]
fn test_when_last_modified_validator_is_present() {
    let policy = CachePolicy::new(
        &simple_request(),
        &response_parts(
            cacheable_response_builder().header(header::LAST_MODIFIED, very_old_date()),
        ),
        Default::default(),
    );

    let headers = policy.revalidation_request(&simple_request()).headers;

    assert_headers_passed(&headers);

    assert_eq!(
        headers.get(header::IF_MODIFIED_SINCE).unwrap(),
        very_old_date()
    );
    let warn = headers.get(header::WARNING);
    assert!(warn.is_none() || !warn.unwrap().to_str().unwrap().contains("113"));
}

#[test]
fn test_not_without_validators() {
    let policy = simple_request_with_cacheable_response();
    let headers = policy.revalidation_request(&simple_request()).headers;

    assert_headers_passed(&headers);
    assert_no_validators(&headers);

    let warn = headers.get(header::WARNING);

    assert!(warn.is_none() || !warn.unwrap().to_str().unwrap().contains("113"));
}

#[test]
fn test_113_added() {
    let now = SystemTime::now();
    let very_old_response = response_parts(
        Response::builder()
            .header(header::AGE, 3600 * 72)
            .header(header::LAST_MODIFIED, very_old_date()),
    );
    let policy = CachePolicy::new(&simple_request(), &very_old_response, Default::default());

    let headers = policy.cached_response(now).headers;

    assert!(headers
        .get(header::WARNING)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("113"));
}

#[test]
fn test_removes_warnings() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &request_parts(Request::builder()),
        &response_parts(Response::builder().header(header::WARNING, "199 test danger")),
        Default::default(),
    );

    assert!(!policy
        .cached_response(now)
        .headers
        .contains_key(header::WARNING));
}

#[test]
fn test_must_contain_any_etag() {
    let policy = CachePolicy::new(
        &simple_request(),
        &response_parts(
            cacheable_response_builder()
                .header(header::LAST_MODIFIED, very_old_date())
                .header(header::ETAG, etag_value()),
        ),
        Default::default(),
    );

    let headers = policy.revalidation_request(&simple_request()).headers;

    assert_eq!(headers.get(header::IF_NONE_MATCH).unwrap(), etag_value());
}

#[test]
fn test_merges_etags() {
    let policy = simple_request_with_etagged_response();

    let headers = policy
        .revalidation_request(&mut request_parts(
            simple_request_builder()
                .header(header::HOST, "www.w3c.org")
                .header(header::IF_NONE_MATCH, "\"foo\", \"bar\""),
        ))
        .headers;

    assert_eq!(
        headers.get(header::IF_NONE_MATCH).unwrap(),
        &format!("\"foo\", \"bar\", {}", etag_value())[..]
    );
}

#[test]
fn test_should_send_the_last_modified_value() {
    let policy = CachePolicy::new(
        &simple_request(),
        &response_parts(
            cacheable_response_builder()
                .header(header::LAST_MODIFIED, very_old_date())
                .header(header::ETAG, etag_value()),
        ),
        Default::default(),
    );

    let headers = policy.revalidation_request(&simple_request()).headers;

    assert_eq!(
        headers.get(header::IF_MODIFIED_SINCE).unwrap(),
        very_old_date()
    );
}

#[test]
fn test_should_not_send_the_last_modified_value_for_post() {
    let mut post_request = request_parts(
        Request::builder()
            .method(Method::POST)
            .header(header::IF_MODIFIED_SINCE, "yesterday"),
    );

    let policy = CachePolicy::new(
        &post_request,
        &response_parts(
            cacheable_response_builder().header(header::LAST_MODIFIED, very_old_date()),
        ),
        Default::default(),
    );

    let headers = policy.revalidation_request(&mut post_request).headers;

    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}

#[test]
fn test_should_not_send_the_last_modified_value_for_range_request() {
    let mut range_request = request_parts(
        Request::builder()
            .method(Method::GET)
            .header(header::ACCEPT_RANGES, "1-3")
            .header(header::IF_MODIFIED_SINCE, "yesterday"),
    );

    let policy = CachePolicy::new(
        &range_request,
        &response_parts(
            cacheable_response_builder().header(header::LAST_MODIFIED, very_old_date()),
        ),
        Default::default(),
    );

    let headers = policy.revalidation_request(&mut range_request).headers;

    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}
