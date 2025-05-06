use http::Method;
use http::{header, HeaderValue, Request, Response};
use http_cache_semantics::CachePolicy;
use std::time::SystemTime;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

use crate::private_opts;
use crate::req_cache_control;
use crate::request_parts;
use crate::response_parts;
use crate::Harness;

fn assert_cached(should_put: bool, response_code: u16) {
    let now = SystemTime::now();
    let options = private_opts();

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

    let policy = CachePolicy::new_options(&request, &response, now, options);

    assert_eq!(should_put, policy.is_storable());
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

#[test]
fn default_expiration_date_fully_cached_for_less_than_24_hours() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::LAST_MODIFIED, format_date(-105, 1))
            .header(header::DATE, format_date(-5, 1)),
    );

    let policy = Harness::default()
        .time(now)
        .test_with_response(response);

    assert!(policy.time_to_live(now).as_secs() > 4);
}

#[test]
fn default_expiration_date_fully_cached_for_more_than_24_hours() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::LAST_MODIFIED, format_date(-105, 3600 * 24))
            .header(header::DATE, format_date(-5, 3600 * 24)),
    );
    let policy = Harness::default()
        .time(now)
        .options(private_opts())
        .test_with_response(response);

    assert!(policy.time_to_live(now).as_secs() >= 5 * 3600 * 24 - 1);
}

#[test]
fn max_age_in_the_past_with_date_header_but_no_last_modified_header() {
    // Chrome interprets max-age relative to the local clock. Both our cache
    // and Firefox both use the earlier of the local and server's clock.
    let response = response_parts(
        Response::builder()
            .header(header::AGE, 120)
            .header(header::CACHE_CONTROL, "max-age=60"),
    );
    Harness::default()
        .stale_and_store()
        .options(private_opts())
        .test_with_response(response);
}

#[test]
fn max_age_preferred_over_lower_shared_max_age() {
    let response = response_parts(
        Response::builder()
            .header(header::DATE, format_date(-2, 60))
            .header(header::CACHE_CONTROL, "s-maxage=60, max-age=180"),
    );

    Harness::default()
        .assert_time_to_live(180)
        .options(private_opts())
        .test_with_response(response);
}

#[test]
fn max_age_preferred_over_higher_max_age() {
    let response = response_parts(
        Response::builder()
            .header(header::AGE, 3 * 60)
            .header(header::CACHE_CONTROL, "s-maxage=60, max-age=180"),
    );
    Harness::default()
        .stale_and_store()
        .options(private_opts())
        .test_with_response(response);
}

fn request_method_not_cached(method: Method) {
    let request = request_parts(Request::builder().method(method));
    let response =
        response_parts(Response::builder().header(header::EXPIRES, format_date(1, 3600)));

    Harness::default()
        .no_store()
        .options(private_opts())
        .request(request)
        .test_with_response(response);
}

#[test]
fn request_method_options_is_not_cached() {
    request_method_not_cached(Method::OPTIONS);
}

#[test]
fn request_method_put_is_not_cached() {
    request_method_not_cached(Method::PUT);
}

#[test]
fn request_method_delete_is_not_cached() {
    request_method_not_cached(Method::DELETE);
}

#[test]
fn request_method_trace_is_not_cached() {
    request_method_not_cached(Method::TRACE);
}

#[test]
fn etag_and_expiration_date_in_the_future() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::ETAG, "v1")
            .header(header::LAST_MODIFIED, format_date(-2, 3600))
            .header(header::EXPIRES, format_date(1, 3600)),
    );

    let policy = Harness::default()
        .time(now)
        .options(private_opts())
        .test_with_response(response);

    assert!(policy.time_to_live(now).as_millis() > 0);
}

#[test]
fn client_side_no_store() {
    Harness::default()
        .no_store()
        .options(private_opts())
        .request(req_cache_control("no-store"))
        .test_with_cache_control("max-age=60");
}

#[test]
fn request_max_age() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::LAST_MODIFIED, format_date(-2, 3600))
            .header(header::DATE, format_date(9, 60))
            .header(header::AGE, 60)
            .header(header::EXPIRES, format_date(1, 3600)),
    );

    let policy = Harness::default()
        .assert_age(60)
        .assert_time_to_live(3000)
        .time(now)
        .options(private_opts())
        .test_with_response(response);

    assert!(policy
        .before_request(
            &req_cache_control("max-age=90"),
            now
        )
        .satisfies_without_revalidation());
    assert!(!policy
        .before_request(
            &req_cache_control("max-age=30"),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn request_min_fresh() {
    let now = SystemTime::now();

    let policy = Harness::default()
        .time(now)
        .options(private_opts())
        .test_with_cache_control("max-age=60");

    assert!(!policy
        .before_request(
            &req_cache_control("min-fresh=120"),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy
        .before_request(
            &req_cache_control("min-fresh=10"),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn request_max_stale() {
    let now = SystemTime::now();

    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=120")
            .header(header::AGE, 4 * 60),
    );
    let policy = Harness::default()
        .stale_and_store()
        .time(now)
        .options(private_opts())
        .test_with_response(response);

    assert!(policy
        .before_request(
            &req_cache_control("max-stale=180"),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy
        .before_request(
            &req_cache_control("max-stale"),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &req_cache_control("max-stale=10"),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn request_max_stale_not_honored_with_must_revalidate() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=120, must-revalidate")
            .header(header::DATE, format_date(15, 60))
            .header(header::AGE, 4 * 60),
    );

    let policy = Harness::default()
        .stale_and_store()
        .time(now)
        .options(private_opts())
        .test_with_response(response);

    assert!(!policy
        .before_request(
            &req_cache_control("max-stale=180"),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &req_cache_control("max-stale"),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn get_headers_deletes_cached_100_level_warnings() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header("cache-control", "immutable")
            .header(header::WARNING, "199 test danger, 200 ok ok"),
    );
    let policy = Harness::default()
        .time(now)
        .request(req_cache_control("max-stale"))
        .test_with_response(response);

    assert_eq!(
        "200 ok ok",
        get_cached_response(&policy, &request_parts(Request::builder()), now).headers
            [header::WARNING]
    );
}

#[test]
fn do_not_cache_partial_response() {
    let response = response_parts(
        Response::builder()
            .status(206)
            .header(header::CONTENT_RANGE, "bytes 100-100/200")
            .header(header::CACHE_CONTROL, "max-age=60"),
    );
    Harness::default()
        .no_store()
        .test_with_response(response);
}

fn format_date(delta: i64, unit: i64) -> String {
    let now = OffsetDateTime::now_utc();
    let timestamp = now.unix_timestamp() + delta * unit;

    let date = OffsetDateTime::from_unix_timestamp(timestamp).unwrap();
    date.format(&Rfc2822).unwrap()
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
