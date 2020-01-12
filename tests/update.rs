use http::header::HeaderName;
use http::request::Parts as RequestParts;
use http::{header, HeaderMap, Request, Response};
use http_cache_semantics::CacheOptions;

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn simple_request_builder_for_update(
    additional_headers: Option<HeaderMap>,
) -> http::request::Builder {
    let mut builder = Request::builder()
        .header(header::HOST, "www.w3c.org")
        .header(header::CONNECTION, "close")
        .uri("/Protocols/rfc2616/rfc2616-sec14.html");

    let builder_headers = builder.headers_mut().unwrap();
    if additional_headers.is_some() {
        for (key, value) in additional_headers.unwrap() {
            builder_headers.insert(key.unwrap(), value);
        }
    }

    builder
}

fn cacheable_response_builder() -> http::response::Builder {
    Response::builder().header(header::CACHE_CONTROL, cacheable_header())
}

fn cacheable_response_builder_for_update() -> http::response::Builder {
    Response::builder().header(header::CACHE_CONTROL, "max-age=111")
}

fn cacheable_header() -> &'static str {
    "max-age=111"
}

fn etagged_response_builder() -> http::response::Builder {
    cacheable_response_builder_for_update().header(header::ETAG, "\"123456789\"")
}

fn weak_tagged_response_builder() -> http::response::Builder {
    cacheable_response_builder_for_update().header(header::ETAG, "W/\"123456789\"")
}

fn last_modified_response_builder() -> http::response::Builder {
    cacheable_response_builder_for_update()
        .header(header::LAST_MODIFIED, "Tue, 15 Nov 1994 12:45:26 GMT")
}

fn multivalidator_response_builder() -> http::response::Builder {
    cacheable_response_builder()
        .header(header::ETAG, "\"123456789\"")
        .header(header::LAST_MODIFIED, "Tue, 15 Nov 1994 12:45:26 GMT")
}

fn request_parts_from_headers(headers: HeaderMap) -> RequestParts {
    let mut builder = Request::builder();

    for (key, value) in headers {
        match key {
            Some(x) => {
                builder.headers_mut().unwrap().insert(x, value);
            }
            None => (),
        }
    }

    request_parts(builder)
}

fn not_modified_response_headers_for_update(
    first_request_builder: http::request::Builder,
    first_response_builder: http::response::Builder,
    second_request_builder: http::request::Builder,
    second_response_builder: http::response::Builder,
) -> Option<HeaderMap> {
    let policy = CacheOptions {
        ..CacheOptions::default()
    }
    .policy_for(
        &request_parts(first_request_builder),
        &response_parts(first_response_builder),
    );

    let headers = policy.revalidation_headers(&mut request_parts(second_request_builder));

    let (new_cache, is_modified) = revalidate(
        &mut request_parts_from_headers(headers),
        &mut response_parts(second_response_builder),
    );

    if is_modified {
        return None;
    }

    Some(new_cache.response_headers())
}

fn assert_updates(
    first_request_builder: http::request::Builder,
    first_response_builder: http::response::Builder,
    second_request_builder: http::request::Builder,
    second_response_builder: http::response::Builder,
) {
    let extended_second_response_builder = second_response_builder
        .header(HeaderName::from_static("foo"), "updated")
        .header(HeaderName::from_static("x-ignore-new"), "ignoreme");

    let headers_opt = not_modified_response_headers_for_update(
        first_request_builder,
        first_response_builder
            .header(HeaderName::from_static("foo"), "original")
            .header(HeaderName::from_static("x-other"), "original"),
        second_request_builder,
        extended_second_response_builder,
    );
    assert!(headers_opt.is_some());
    let headers = match headers_opt {
        Some(x) => x,
        None => panic!(),
    };
    assert_eq!(headers.get("foo").unwrap(), "updated");
    assert_eq!(headers.get("x-other").unwrap(), "original");
    assert!(headers.get("x-ignore-new").is_none());
    assert_eq!(
        headers.get(header::ETAG).unwrap(),
        extended_second_response_builder
            .borrow()
            .headers_ref()
            .unwrap()
            .get(header::ETAG)
            .unwrap()
    );
}

#[test]
fn test_matching_etags_are_updated() {
    assert_updates(
        simple_request_builder_for_update(None),
        etagged_response_builder(),
        simple_request_builder_for_update(None),
        etagged_response_builder(),
    );
}

/*
#[test]
fn test_matching_weak_etags_are_updated() {
    assert!(false);
}

#[test]
fn test_matching_last_mod_are_updated() {
    assert!(false);
}

#[test]
fn test_both_matching_are_updated() {
    assert!(false);
}

#[test]
fn test_check_status() {
    assert!(false);
}

#[test]
fn test_last_mod_ignored_if_etag_is_wrong() {
    assert!(false);
}

#[test]
fn test_ignored_if_validator_is_missing() {
    assert!(false);
}

#[test]
fn test_skips_update_of_content_length() {
    assert!(false);
}

#[test]
fn test_ignored_if_validator_is_different() {
    assert!(false);
}

#[test]
fn test_ignored_if_validator_does_not_match() {
    assert!(false);
}
*/
