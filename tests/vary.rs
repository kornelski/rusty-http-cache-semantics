use http::{header, Request, Response};
use http_cache_semantics::CachePolicy;

use std::time::SystemTime;

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

#[test]
fn test_vary_basic() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "weather"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
        Default::default(),
    );

    assert!(policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "nice")),
        now
    ));

    assert!(!policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "bad")),
        now
    ));
}

#[test]
fn test_asterisks_does_not_match() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "*"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "ok")),
        &response,
        Default::default(),
    );

    assert!(!policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "ok")),
        now
    ));
}

#[test]
fn test_asterisks_is_stale() {
    let now = SystemTime::now();
    let policy_one = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "ok")),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public,max-age=99")
                .header(header::VARY, "*"),
        ),
        Default::default(),
    );

    let policy_two = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "ok")),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public,max-age=99")
                .header(header::VARY, "weather"),
        ),
        Default::default(),
    );

    assert!(policy_one.is_stale(now));
    assert!(!policy_two.is_stale(now));
}

#[test]
fn test_values_are_case_sensitive() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public,max-age=5")
            .header(header::VARY, "weather"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "BAD")),
        &response,
        Default::default(),
    );

    assert!(policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "BAD")),
        now
    ));

    assert!(!policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "bad")),
        now
    ));
}

#[test]
fn test_irrelevant_headers_ignored() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "moon-phase"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
        Default::default(),
    );

    assert!(policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "bad")),
        now
    ));

    assert!(policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "shining")),
        now
    ));

    assert!(!policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("moon-phase", "full")),
        now
    ));
}

#[test]
fn test_absence_is_meaningful() {
    let now = SystemTime::now();
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "moon-phase, weather"),
    );

    let policy = CachePolicy::new(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
        Default::default(),
    );

    assert!(policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "nice")),
        now,
    ));

    assert!(!policy.satisfies_without_revalidation(
        &mut request_parts(
            Request::builder()
                .header("weather", "nice")
                .header("moon-phase", "")
        ),
        now,
    ));

    assert!(!policy.satisfies_without_revalidation(&mut request_parts(Request::builder()), now));
}

#[test]
fn test_all_values_must_match() {
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
        Default::default(),
    );

    assert!(policy.satisfies_without_revalidation(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice")
        ),
        now
    ));

    assert!(!policy.satisfies_without_revalidation(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "bad")
        ),
        now
    ));
}

#[test]
fn test_whitespace_is_okay() {
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
        Default::default(),
    );

    assert!(policy.satisfies_without_revalidation(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice")
        ),
        now
    ));

    assert!(!policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("weather", "nice")),
        now
    ));

    assert!(!policy.satisfies_without_revalidation(
        &mut request_parts(Request::builder().header("sun", "shining")),
        now
    ));
}

#[test]
fn test_order_is_irrelevant() {
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
        Default::default(),
    );

    let policy_two = CachePolicy::new(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response_two,
        Default::default(),
    );

    assert!(policy_one.satisfies_without_revalidation(
        &mut request_parts(
            Request::builder()
                .header("weather", "nice")
                .header("sun", "shining")
        ),
        now
    ));

    assert!(policy_one.satisfies_without_revalidation(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice")
        ),
        now
    ));

    assert!(policy_two.satisfies_without_revalidation(
        &mut request_parts(
            Request::builder()
                .header("weather", "nice")
                .header("sun", "shining")
        ),
        now
    ));

    assert!(policy_two.satisfies_without_revalidation(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice")
        ),
        now
    ));
}
