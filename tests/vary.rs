use http::{header, Request, Response};
use http_cache_semantics::CacheOptions;

fn request_parts(builder: http::request::Builder) -> http::request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

#[test]
fn test_vary_basic() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "weather"),
    );

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "nice")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "bad")),
        &response,
    ));
}

#[test]
fn test_asterisks_does_not_match() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "*"),
    );

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().header("weather", "ok")),
        &response,
    );

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "ok")),
        &response,
    ));
}

#[test]
fn test_asterisks_is_stale() {
    let policy_one = CacheOptions::default().policy_for(
        &request_parts(Request::builder().header("weather", "ok")),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public,max-age=99")
                .header(header::VARY, "*"),
        ),
    );

    let policy_two = CacheOptions::default().policy_for(
        &request_parts(Request::builder().header("weather", "ok")),
        &response_parts(
            Response::builder()
                .header(header::CACHE_CONTROL, "public,max-age=99")
                .header(header::VARY, "weather"),
        ),
    );

    assert!(policy_one.is_stale());
    assert!(!policy_two.is_stale());
}

#[test]
fn test_values_are_case_sensitive() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "public,max-age=5")
            .header(header::VARY, "weather"),
    );

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().header("weather", "BAD")),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "BAD")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "bad")),
        &response,
    ));
}

#[test]
fn test_irrelevant_headers_ignored() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "moon-phase"),
    );

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "bad")),
        &response,
    ));

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "shining")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("moon-phase", "full")),
        &response,
    ));
}

#[test]
fn test_absence_is_meaningful() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "moon-phase, weather"),
    );

    let policy = CacheOptions::default().policy_for(
        &request_parts(Request::builder().header("weather", "nice")),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "nice")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(
            Request::builder()
                .header("weather", "nice")
                .header("moon-phase", "")
        ),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(&mut request_parts(Request::builder()), &response));
}

#[test]
fn test_all_values_must_match() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "weather, sun"),
    );

    let policy = CacheOptions::default().policy_for(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice")
        ),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "bad")
        ),
        &response,
    ));
}

#[test]
fn test_whitespace_is_okay() {
    let response = response_parts(
        Response::builder()
            .header(header::CACHE_CONTROL, "max-age=5")
            .header(header::VARY, "    weather       ,     sun     "),
    );

    let policy = CacheOptions::default().policy_for(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response,
    );

    assert!(policy.is_cached_response_fresh(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice")
        ),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("weather", "nice")),
        &response,
    ));

    assert!(!policy.is_cached_response_fresh(
        &mut request_parts(Request::builder().header("sun", "shining")),
        &response,
    ));
}

#[test]
fn test_order_is_irrelevant() {
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

    let policy_one = CacheOptions::default().policy_for(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response_one,
    );

    let policy_two = CacheOptions::default().policy_for(
        &request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice"),
        ),
        &response_two,
    );

    assert!(policy_one.is_cached_response_fresh(
        &mut request_parts(
            Request::builder()
                .header("weather", "nice")
                .header("sun", "shining")
        ),
        &response_one,
    ));

    assert!(policy_one.is_cached_response_fresh(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice")
        ),
        &response_one,
    ));

    assert!(policy_two.is_cached_response_fresh(
        &mut request_parts(
            Request::builder()
                .header("weather", "nice")
                .header("sun", "shining")
        ),
        &response_two,
    ));

    assert!(policy_two.is_cached_response_fresh(
        &mut request_parts(
            Request::builder()
                .header("sun", "shining")
                .header("weather", "nice")
        ),
        &response_two,
    ));
}
