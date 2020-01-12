use chrono::{DateTime, Utc};
use http::{header, HeaderValue, Request, Response};
use http_cache_semantics::CacheOptions;

fn format_date(delta: i64, unit: i64) -> String {
    let now: DateTime<Utc> = Utc::now();
    let result = now.timestamp_nanos() + delta * unit * 1000;

    return result.to_string();
}

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn assert_cached(should_put: bool, response_code: u16) {
    let options = CacheOptions::new_unshared();

    let mut response = response_parts(
        Response::builder()
            .header(header::LAST_MODIFIED, format_date(-105, 1))
            .header(header::EXPIRES, format_date(1, 3600))
            .header(header::WWW_AUTHENTICATE, "challenge")
            .status(response_code),
    );

    if 407 == response_code {
        response.headers.insert(
            header::PROXY_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"protected area\""),
        );
    } else if 401 == response_code {
        response.headers.insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"protected area\""),
        );
    }

    let request = request_parts(Request::get("/"));

    let policy = options.policy_for(&request, &response);

    assert_eq!(should_put, policy.is_storable());
}

#[test]
fn test_ok_http_response_caching_by_response_code() {
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
    assert_cached(false, 303);
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

#[test]
fn test_default_expiration_date_fully_cached_for_less_than_24_hours() {
    let options = CacheOptions::new_unshared();

    let policy = options.policy_for(
        &request_parts(Request::get("/")),
        &response_parts(
            Response::builder()
                .header(header::LAST_MODIFIED, format_date(-105, 1))
                .header(header::DATE, format_date(-5, 1)),
        ),
    );

    assert!(policy.time_to_live() > 4000);
}

#[test]
fn test_default_expiration_date_fully_cached_for_more_than_24_hours() {
    let options = CacheOptions::new_unshared();

    let policy = options.policy_for(
        &request_parts(Request::get("/")),
        &response_parts(
            Response::builder()
                .header(header::LAST_MODIFIED, format_date(-105, 3600 * 24))
                .header(header::DATE, format_date(-5, 3600 * 24)),
        ),
    );

    assert!(policy.max_age() >= 10 * 3600 * 24);
    assert!(policy.time_to_live() + 1000 >= 5 * 3600 * 24);
}

#[test]
fn test_max_age_in_the_past_with_date_header_but_no_last_modified_header() {
    let options = CacheOptions::new_unshared();

    // Chrome interprets max-age relative to the local clock. Both our cache
    // and Firefox both use the earlier of the local and server's clock.
    let request = request_parts(Request::get("/"));
    let response = response_parts(
        Response::builder()
            .header(header::DATE, format_date(-120, 1))
            .header(header::CACHE_CONTROL, "max-age=60"),
    );
    let policy = options.policy_for(&request, &response);

    assert!(policy.is_stale());
}

#[test]
fn test_max_age_preferred_over_lower_shared_max_age() {
    let options = CacheOptions::new_unshared();

    let policy = options.policy_for(
        &request_parts(Request::builder()),
        &response_parts(
            Response::builder()
                .header(header::DATE, format_date(-2, 60))
                .header(header::CACHE_CONTROL, "s-maxage=60, max-age=180"),
        ),
    );

    assert_eq!(policy.max_age(), 180);
}

#[test]
fn test_max_age_preferred_over_higher_max_age() {
    let options = CacheOptions::new_unshared();

    let request = request_parts(Request::get("/"));
    let response = response_parts(
        Response::builder()
            .header(header::DATE, format_date(-3, 60))
            .header(header::CACHE_CONTROL, "s-maxage=60, max-age=180"),
    );
    let policy = options.policy_for(&request, &response);

    assert!(policy.is_stale());
}

fn request_method_not_cached(method: &str) {
    let options = CacheOptions::new_unshared();

    // 1. seed the cache (potentially)
    // 2. expect a cache hit or miss
    let request = request_parts(Request::builder().method(method));

    let response =
        response_parts(Response::builder().header(header::EXPIRES, format_date(1, 3600)));

    let policy = options.policy_for(&request, &response);

    assert!(policy.is_stale());
}

#[test]
fn test_request_method_options_is_not_cached() {
    request_method_not_cached("OPTIONS");
}

#[test]
fn test_request_method_put_is_not_cached() {
    request_method_not_cached("PUT");
}

#[test]
fn test_request_method_delete_is_not_cached() {
    request_method_not_cached("DELETE");
}

#[test]
fn test_request_method_trace_is_not_cached() {
    request_method_not_cached("TRACE");
}

#[test]
fn test_etag_and_expiration_date_in_the_future() {
    let options = CacheOptions::new_unshared();

    let policy = options.policy_for(
        &request_parts(Request::builder()),
        &response_parts(
            Response::builder()
                .header(header::ETAG, "v1")
                .header(header::LAST_MODIFIED, format_date(-2, 3600))
                .header(header::EXPIRES, format_date(1, 3600)),
        ),
    );

    assert!(policy.time_to_live() > 0);
}

#[test]
fn test_client_side_no_store() {
    let options = CacheOptions::new_unshared();

    let policy = options.policy_for(
        &request_parts(Request::builder().header(header::CACHE_CONTROL, "no-store")),
        &response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=60")),
    );

    assert!(!policy.is_storable());
}

#[test]
fn test_request_max_age() {
    let options = CacheOptions::new_unshared();

    let first_request = request_parts(Request::builder());
    let response = response_parts(
        Response::builder()
            .header(header::LAST_MODIFIED, format_date(-2, 3600))
            .header(header::DATE, format_date(-1, 3600))
            .header(header::EXPIRES, format_date(1, 3600)),
    );

    let policy = options.policy_for(&first_request, &response);

    assert!(policy.is_stale());
    assert!(policy.age() >= 60);
    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "max-age=90")),
        &response,
    ));
    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "max-age=30")),
        &response,
    ));
}

#[test]
fn test_request_min_fresh() {
    let options = CacheOptions::new_unshared();

    let response = response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=60"));

    let policy = options.policy_for(&request_parts(Request::builder()), &response);

    assert!(!policy.is_stale());

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "min-fresh=120")),
        &response,
    ));

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "min-fresh=10")),
        &response,
    ));
}

#[test]
fn test_request_max_stale() {
    let options = CacheOptions::new_unshared();

    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=120")
            .header(header::DATE, format_date(-4, 60)),
    );

    let policy = options.policy_for(&request_parts(Request::builder()), &response);

    assert!(policy.is_stale());

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "max-stale=180")),
        &response,
    ));

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "max-stale")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "max-stale=10")),
        &response,
    ));
}

#[test]
fn test_request_max_stale_not_honored_with_must_revalidate() {
    let options = CacheOptions::new_unshared();

    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=120, must-revalidate")
            .header(header::DATE, format_date(-4, 60)),
    );

    let policy = options.policy_for(&request_parts(Request::builder()), &response);

    assert!(policy.is_stale());

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "max-stale=180")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header(header::CACHE_CONTROL, "max-stale")),
        &response,
    ));
}

#[test]
fn test_get_headers_deletes_cached_100_level_warnings() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder()),
        &response_parts(Response::builder().header(header::WARNING, "199 test danger, 200 ok ok")),
    );

    assert_eq!(
        "200 ok ok",
        policy.response_headers()[header::WARNING.as_str()]
    );
}

#[test]
fn test_do_not_cache_partial_response() {
    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder()),
        &response_parts(
            Response::builder()
                .status(206)
                .header(header::CONTENT_RANGE, "bytes 100-100/200")
                .header(header::CACHE_CONTROL, "max-age=60"),
        ),
    );

    assert!(!policy.is_storable());
}
