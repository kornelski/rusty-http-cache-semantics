use std::time::SystemTime;

use crate::request_parts;
use crate::response_parts;

use http::{header, Request, Response};
use http_cache_semantics::CachePolicy;

#[test]
fn vary_basic() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "weather"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
    );

    assert!(policy
        .before_request(
            &request_parts(Request::builder().header("weather", "nice")),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header("weather", "bad")),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn asterisks_does_not_match() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "*"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "ok")),
        &response,
    );

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header("weather", "ok")),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn asterisks_is_stale() {
    let now = SystemTime::now();
    let policy_one = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "ok")),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public,max-age=99")
                .header(header::VARY, "*"),
        ),
    );

    let policy_two = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "ok")),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public,max-age=99")
                .header(header::VARY, "weather"),
        ),
    );

    assert!(policy_one.is_stale(now));
    assert!(!policy_two.is_stale(now));
}

#[test]
fn values_are_case_sensitive() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public,max-age=5")
            .header(header::VARY, "weather"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "BAD")),
        &response,
    );

    assert!(policy
        .before_request(
            &request_parts(Request::builder().header("weather", "BAD")),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header("weather", "bad")),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn irrelevant_headers_ignored() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "moon-phase"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
    );

    assert!(policy
        .before_request(
            &request_parts(Request::builder().header("weather", "bad")),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy
        .before_request(
            &request_parts(Request::builder().header("weather", "shining")),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header("moon-phase", "full")),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn absence_is_meaningful() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "moon-phase, weather"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
    );

    assert!(policy
        .before_request(
            &request_parts(Request::builder().header("weather", "nice")),
            now,
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(
                Request::builder()
                    .header("weather", "nice")
                    .header("moon-phase", "")
            ),
            now,
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(&request_parts(Request::builder()), now)
        .satisfies_without_revalidation());
}

#[test]
fn all_values_must_match() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "weather, sun"),
    );

    let policy = CachePolicy::new(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response,
    );

    assert!(policy
        .before_request(
            &request_parts(
                Request::builder()
                    .header("sun", "shining")
                    .header("weather", "nice")
            ),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(
                Request::builder()
                    .header("sun", "shining")
                    .header("weather", "bad")
            ),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn whitespace_is_okay() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "    weather       ,     sun     "),
    );

    let policy = CachePolicy::new(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response,
    );

    assert!(policy
        .before_request(
            &request_parts(
                Request::builder()
                    .header("sun", "shining")
                    .header("weather", "nice")
            ),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header("weather", "nice")),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &request_parts(Request::builder().header("sun", "shining")),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn order_is_irrelevant() {
    let now = SystemTime::now();
    let response_one = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "weather, sun"),
    );

    let response_two = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "sun, weather"),
    );

    let policy_one = CachePolicy::new(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response_one,
    );

    let policy_two = CachePolicy::new(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response_two,
    );

    assert!(policy_one
        .before_request(
            &request_parts(
                Request::builder()
                    .header("weather", "nice")
                    .header("sun", "shining")
            ),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy_one
        .before_request(
            &request_parts(
                Request::builder()
                    .header("sun", "shining")
                    .header("weather", "nice")
            ),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy_two
        .before_request(
            &request_parts(
                Request::builder()
                    .header("weather", "nice")
                    .header("sun", "shining")
            ),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy_two
        .before_request(
            &request_parts(
                Request::builder()
                    .header("sun", "shining")
                    .header("weather", "nice")
            ),
            now
        )
        .satisfies_without_revalidation());
}
