use http::{header, HeaderMap, HeaderValue, Method, Request, Response};
use http_cache_semantics::CachePolicy;
use std::time::Duration;
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

fn assert_no_connection(headers: &HeaderMap<HeaderValue>) {
    assert!(!headers.contains_key(header::CONNECTION), "{:#?}", headers);
}
fn assert_custom_header(headers: &HeaderMap<HeaderValue>) {
    assert!(headers.contains_key("x-custom"), "{:#?}", headers);
    assert_eq!(headers.get("x-custom").unwrap(), "yes");
}

fn assert_no_validators(headers: &HeaderMap<HeaderValue>) {
    assert!(!headers.contains_key(header::IF_NONE_MATCH));
    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}

#[test]
fn test_ok_if_method_changes_to_head() {
    let now = SystemTime::now();
    let policy = simple_request_with_etagged_response();

    let headers = get_revalidation_request(
        &policy,
        &mut request_parts(
            simple_request_builder()
                .method(Method::HEAD)
                .header("pragma", "no-cache"),
        ),
        now,
    )
    .headers;

    assert_custom_header(&headers);
    assert!(
        headers.contains_key(header::IF_NONE_MATCH),
        "{:#?}",
        headers
    );
    assert_eq!(headers.get(header::IF_NONE_MATCH).unwrap(), "\"123456789\"");
}

#[test]
fn test_not_if_method_mismatch_other_than_head() {
    let now = SystemTime::now();
    let policy = simple_request_with_etagged_response();

    let incoming_request = request_parts(simple_request_builder().method(Method::POST));
    let headers = get_revalidation_request(
        &policy,
        &incoming_request,
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_custom_header(&headers);
    assert_no_validators(&headers);
}

#[test]
fn test_not_if_url_mismatch() {
    let now = SystemTime::now();
    let policy = simple_request_with_etagged_response();

    let incoming_request = request_parts(simple_request_builder().uri("/yomomma"));
    let headers = get_revalidation_request(
        &policy,
        &incoming_request,
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_custom_header(&headers);
    assert_no_validators(&headers);
}

#[test]
fn test_not_if_host_mismatch() {
    let now = SystemTime::now();
    let policy = simple_request_with_etagged_response();

    let mut incoming_request = request_parts(simple_request_builder());
    incoming_request
        .headers
        .insert(header::HOST, "www.w4c.org".parse().unwrap());
    let headers = get_revalidation_request(
        &policy,
        dbg!(&incoming_request),
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_no_validators(&headers);
    assert!(headers.contains_key("x-custom"));
}

#[test]
fn test_not_if_vary_fields_prevent() {
    let now = SystemTime::now();
    let policy = simple_request_with_always_variable_response();

    let headers = get_revalidation_request(
        &policy,
        &simple_request(),
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_custom_header(&headers);
    assert_no_validators(&headers);
}

#[test]
fn test_when_entity_tag_validator_is_present() {
    let now = SystemTime::now();
    let policy = simple_request_with_etagged_response();

    let headers = get_revalidation_request(
        &policy,
        &simple_request(),
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_custom_header(&headers);
    assert_no_connection(&headers);
    assert_eq!(headers.get(header::IF_NONE_MATCH).unwrap(), "\"123456789\"");
}

#[test]
fn test_skips_weak_validators_on_post() {
    let now = SystemTime::now();
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

    let headers = get_revalidation_request(
        &policy,
        &mut post_request,
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_eq!(
        headers.get(header::IF_NONE_MATCH).unwrap(),
        "\"strong\", \"123456789\""
    );
    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}

#[test]
fn test_skips_weak_validators_on_post_2() {
    let now = SystemTime::now();
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

    let headers = get_revalidation_request(
        &policy,
        &mut post_request,
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert!(!headers.contains_key(header::IF_NONE_MATCH));
    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}

#[test]
fn test_merges_validators() {
    let now = SystemTime::now();
    let mut post_request = request_parts(
        simple_request_builder()
            .header(header::IF_NONE_MATCH, "W/\"weak\", \"strong\", W/\"weak2\""),
    );
    let policy = CachePolicy::new(
        &post_request,
        &response_parts(
            cacheable_response_builder()
                .header(header::LAST_MODIFIED, very_old_date())
                .header(header::ETAG, etag_value())
                .header(header::CACHE_CONTROL, "must-revalidate"),
        ),
        Default::default(),
    );

    let headers = get_revalidation_request(
        &policy,
        &mut post_request,
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

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
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &simple_request(),
        &response_parts(
            cacheable_response_builder().header(header::LAST_MODIFIED, very_old_date()),
        ),
        Default::default(),
    );

    let headers = get_revalidation_request(
        &policy,
        &simple_request(),
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_custom_header(&headers);
    assert_no_connection(&headers);

    assert_eq!(
        headers.get(header::IF_MODIFIED_SINCE).unwrap(),
        very_old_date()
    );
    let warn = headers.get(header::WARNING);
    assert!(warn.is_none() || !warn.unwrap().to_str().unwrap().contains("113"));
}

#[test]
fn test_not_without_validators() {
    let now = SystemTime::now();
    let policy = simple_request_with_cacheable_response();
    let headers = get_revalidation_request(
        &policy,
        &simple_request(),
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_custom_header(&headers);
    assert_no_connection(&headers);
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
    let req = simple_request();
    let policy = CachePolicy::new(&req, &very_old_response, Default::default());

    let headers = get_cached_response(&policy, &req, now).headers;

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
    let req = request_parts(Request::builder());
    let policy = CachePolicy::new(
        &req,
        &response_parts(
            Response::builder()
                .header("cache-control", "max-age=2")
                .header(header::WARNING, "199 test danger"),
        ),
        Default::default(),
    );

    assert!(!get_cached_response(&policy, &req, now)
        .headers
        .contains_key(header::WARNING));
}

#[test]
fn test_must_contain_any_etag() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &simple_request(),
        &response_parts(
            cacheable_response_builder()
                .header(header::LAST_MODIFIED, very_old_date())
                .header(header::ETAG, etag_value()),
        ),
        Default::default(),
    );

    let headers = get_revalidation_request(
        &policy,
        &simple_request(),
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_eq!(headers.get(header::IF_NONE_MATCH).unwrap(), etag_value());
}

#[test]
fn test_merges_etags() {
    let now = SystemTime::now();
    let policy = simple_request_with_etagged_response();

    let headers = get_revalidation_request(
        &policy,
        &mut request_parts(
            simple_request_builder()
                .header(header::HOST, "www.w3c.org")
                .header(header::IF_NONE_MATCH, "\"foo\", \"bar\""),
        ),
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_eq!(
        headers.get(header::IF_NONE_MATCH).unwrap(),
        &format!("\"foo\", \"bar\", {}", etag_value())[..]
    );
}

#[test]
fn test_should_send_the_last_modified_value() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &simple_request(),
        &response_parts(
            cacheable_response_builder()
                .header(header::LAST_MODIFIED, very_old_date())
                .header(header::ETAG, etag_value()),
        ),
        Default::default(),
    );

    let headers = get_revalidation_request(
        &policy,
        &simple_request(),
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert_eq!(
        headers.get(header::IF_MODIFIED_SINCE).unwrap(),
        very_old_date()
    );
}

#[test]
fn test_should_not_send_the_last_modified_value_for_post() {
    let now = SystemTime::now();
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

    let headers = get_revalidation_request(
        &policy,
        &mut post_request,
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
}

#[test]
fn test_should_not_send_the_last_modified_value_for_range_request() {
    let now = SystemTime::now();
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

    let headers = get_revalidation_request(
        &policy,
        &mut range_request,
        now + Duration::from_secs(3600 * 24),
    )
    .headers;

    assert!(!headers.contains_key(header::IF_MODIFIED_SINCE));
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

fn get_revalidation_request(
    policy: &CachePolicy,
    req: &(impl http_cache_semantics::RequestLike + std::fmt::Debug),
    now: SystemTime,
) -> http::request::Parts {
    match policy.before_request(req, now) {
        http_cache_semantics::BeforeRequest::Stale { request, matches } => {
            if !matches {
                eprintln!("warning: req doesn't match {:#?} vs {:#?}", req, policy);
            }
            request
        }
        _ => panic!("no revalidation needed {:#?} vs {:#?}", req, policy),
    }
}
