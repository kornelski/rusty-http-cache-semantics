//! Determines whether a given HTTP response can be cached and whether a
//! cached response can be reused, following the rules specified in [RFC
//! 7234](https://httpwg.org/specs/rfc7234.html).

use chrono::prelude::*;
use http::header::HeaderName;
use http::header::HeaderValue;
use http::Request;
use http::Response;
use http_cache_semantics::*;
use serde_json::json;
use serde_json::Value;
use std::time::SystemTime;

fn res(json: Value) -> Response<()> {
    let mut res = Response::builder()
        .status(json.get("status").and_then(|s| s.as_i64()).unwrap_or(200) as u16);
    if let Some(map) = json.get("headers").and_then(|s| s.as_object()) {
        for (k, v) in map {
            let v = v.as_str().unwrap();
            res = res.header(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
    }
    res.body(()).unwrap()
}

fn req(json: Value) -> Request<()> {
    let mut req = Request::builder()
        .method(json.get("method").and_then(|s| s.as_str()).unwrap_or("GET"))
        .uri(
            json.get("uri")
                .and_then(|s| s.as_str())
                .unwrap_or("http://example.com"),
        );
    if let Some(map) = json.get("headers").and_then(|s| s.as_object()) {
        for (k, v) in map {
            let v = v.as_str().unwrap();
            req = req.header(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
    }
    req.body(()).unwrap()
}

fn assert_cached(should_put: bool, response_code: i32) {
    let mut response = json!({
        "headers": {
            "last-modified": format_date(-105, 1),
            "expires": format_date(1, 3600),
            "www-authenticate": "challenge"
        },
        "status": response_code,
    });

    if 407 == response_code {
        response["headers"]["proxy-authenticate"] = json!("Basic realm=\"protected area\"");
    } else if 401 == response_code {
        response["headers"]["www-authenticate"] = json!("Basic realm=\"protected area\"");
    }

    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(response),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );

    assert_eq!(
        should_put,
        policy.is_storable(),
        "{}; {}; {:#?}",
        should_put,
        response_code,
        policy
    );
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
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "last-modified": format_date(-105, 1),
                "date": format_date(-5, 1),
            },
            "body": "A"
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.time_to_live(now).as_secs() >= 4);
}

#[test]
fn test_default_expiration_date_fully_cached_for_more_than_24_hours() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "last-modified": format_date(-105, 3600 * 24),
                "date": format_date(-5, 3600 * 24),
            },
            "body": "A"
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.max_age().as_secs() >= 10 * 3600 * 24);
    assert!(policy.time_to_live(now).as_secs() >= 5 * 3600 * 24 - 1);
}

#[test]
fn test_max_age_in_the_past_with_date_header_but_no_last_modified_header() {
    let now = SystemTime::now();
    // Chrome interprets max-age relative to the local clock. Both our cache
    // and Firefox both use the earlier of the local and server's clock.
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "date": format_date(-120, 1),
                "cache-control": "max-age=60",
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.is_stale(now));
}

#[test]
fn test_max_age_preferred_over_lower_shared_max_age() {
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "date": format_date(-2, 60),
                "cache-control": "s-maxage=60, max-age=180",
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(policy.max_age().as_secs(), 180);
}

#[test]
fn test_max_age_preferred_over_higher_max_age() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "date": format_date(-3, 60),
                "cache-control": "s-maxage=60, max-age=180",
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.is_stale(now));
}

fn request_method_not_cached(method: String) {
    // 1. seed the cache (potentially)
    // 2. expect a cache hit or miss
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": method,
            "headers": {}
        })),
        &res(json!({
            "headers": {
                "expires": format_date(1, 3600),
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.is_stale(now));
}

#[test]
fn test_request_method_options_is_not_cached() {
    request_method_not_cached("OPTIONS".to_string());
}

#[test]
fn test_request_method_put_is_not_cached() {
    request_method_not_cached("PUT".to_string());
}

#[test]
fn test_request_method_delete_is_not_cached() {
    request_method_not_cached("DELETE".to_string());
}

#[test]
fn test_request_method_trace_is_not_cached() {
    request_method_not_cached("TRACE".to_string());
}

#[test]
fn test_etag_and_expiration_date_in_the_future() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "etag": "v1",
                "last-modified": format_date(-2, 3600),
                "expires": format_date(1, 3600),
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.time_to_live(now).as_secs() > 0);
}

#[test]
fn test_client_side_no_store() {
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "cache-control": "no-store",
            }
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=60",
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(policy.is_storable(), false);
}

#[test]
fn test_request_max_age() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "last-modified": format_date(-2, 3600),
                "date": format_date(-1, 60),
                "expires": format_date(1, 3600),
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(policy.is_stale(now), false);
    assert!(policy.age(now).as_secs() >= 60);

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "max-age=90",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "max-age=30",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_request_min_fresh() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "cache-control": "max-age=60",
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(policy.is_stale(now), false);

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "min-fresh=10",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "min-fresh=120",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_request_max_stale() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "cache-control": "max-age=120",
                "date": format_date(-4, 60),
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.is_stale(now));

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "cache-control": "max-stale=180",
                },
            })),
            now
        )
        .satisfies_without_revalidation());

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "max-stale",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "max-stale=10",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_request_max_stale_not_honored_with_must_revalidate() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "cache-control": "max-age=120, must-revalidate",
                "date": format_date(-4, 60),
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.is_stale(now));

    assert!(!policy
        .before_request(
            &req(json!({
                "headers": {
                    "cache-control": "max-stale=180",
                },
            })),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &req(json!({
                "headers": {
                    "cache-control": "max-stale",
                },
            })),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn test_get_headers_deletes_cached_100_level_warnings() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "warning": "199 test danger, 200 ok ok",
            }
        })),
        Default::default(),
    );

    assert_eq!(
        "200 ok ok",
        policy.cached_response(now).headers()["warning"]
    );
}

#[test]
fn test_do_not_cache_partial_response() {
    let policy = CachePolicy::new(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "status": 206,
            "headers": {
                "content-range": "bytes 100-100/200",
                "cache-control": "max-age=60",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_storable(), false);
}

fn format_date(delta: i64, unit: i64) -> String {
    let now: DateTime<Utc> = Utc::now();
    let timestamp = now.timestamp() + delta * unit;

    let date = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp as _, 0), Utc);
    date.to_rfc2822()
}

#[test]
fn test_no_store_kills_cache() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {
                "cache-control": "no-store",
            }
        })),
        &res(json!({
            "headers": {
                "cache-control": "public, max-age=222",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.is_storable(), false);
}

#[test]
fn test_post_not_cacheable_by_default() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "POST",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "public",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.is_storable(), false);
}

#[test]
fn test_post_cacheable_explicitly() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "POST",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "public, max-age=222",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.is_storable(), true);
}

#[test]
fn test_public_cacheable_auth_is_ok() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {
                "authorization": "test",
            }
        })),
        &res(json!({
            "headers": {
                "cache-control": "public, max-age=222",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.is_storable(), true);
}

#[test]
fn test_proxy_cacheable_auth_is_ok() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {
                "authorization": "test",
            }
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=0,s-maxage=12",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.is_storable(), true);

    #[cfg(feature = "with_serde")]
    {
        let json = serde_json::to_string(&policy).unwrap();
        let policy: CachePolicy = serde_json::from_str(&json).unwrap();

        assert_eq!(!policy.is_stale(now), true);
        assert_eq!(policy.is_storable(), true);
    }
}

#[test]
fn test_private_auth_is_ok() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {
                "authorization": "test",
            }
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=111",
            }
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.is_storable(), true);
}

#[test]
fn test_revalidate_auth_is_ok() {
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {
                "authorization": "test",
            }
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=88,must-revalidate",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_storable(), true);
}

#[test]
fn test_auth_prevents_caching_by_default() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {
                "authorization": "test",
            }
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=111",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.is_storable(), false);
}

#[test]
fn test_simple_miss() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({})),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
}

#[test]
fn test_simple_hit() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({"headers": {
            "cache-control": "public, max-age=999999"
        }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.max_age().as_secs(), 999999);
}

#[test]
fn test_weird_syntax() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({"headers": {
            "cache-control": ",,,,max-age =  456      ,"
        }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.max_age().as_secs(), 456);

    #[cfg(feature = "with_serde")]
    {
        let json = serde_json::to_string(&policy).unwrap();
        let policy: CachePolicy = serde_json::from_str(&json).unwrap();

        assert_eq!(policy.is_stale(now), false);
        assert_eq!(policy.max_age().as_secs(), 456);
    }
}

#[test]
fn test_quoted_syntax() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({"headers": {
            "cache-control": "  max-age = \"678\"      "
        }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.max_age().as_secs(), 678);
}

#[test]
fn test_age_can_make_stale() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=100",
                "age": "101"
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.is_storable(), true);
}

#[test]
fn test_age_not_always_stale() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=20",
                "age": "15"
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.is_storable(), true);
}

#[test]
fn test_bogus_age_ignored() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=20",
                "age": "golden"
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.is_storable(), true);
}

#[test]
fn test_immutable_simple_hit() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "immutable, max-age=999999",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.max_age().as_secs(), 999999);
}

#[test]
fn test_immutable_can_expire() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "immutable, max-age=0",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_pragma_no_cache() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "pragma": "no-cache",
                "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
}

#[test]
fn test_no_store() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "no-store, public, max-age=1",
            }
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_observe_private_cache() {
    let now = SystemTime::now();
    let private_header = json!({
        "cache-control": "private, max-age=1234",
    });

    let proxy_policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": private_header })),
        Default::default(),
    );

    assert_eq!(proxy_policy.is_stale(now), true);
    assert_eq!(proxy_policy.max_age().as_secs(), 0);

    let ua_cache = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": private_header })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(ua_cache.is_stale(now), false);
    assert_eq!(ua_cache.max_age().as_secs(), 1234);
}

#[test]
fn test_do_not_share_cookies() {
    let now = SystemTime::now();
    let cookie_header = json!({
        "set-cookie": "foo=bar",
        "cache-control": "max-age=99",
    });

    let proxy_policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": cookie_header })),
        CachePolicyOptions {
            shared: true,
            ..Default::default()
        },
    );

    assert_eq!(proxy_policy.is_stale(now), true);
    assert_eq!(proxy_policy.max_age().as_secs(), 0);

    let ua_cache = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": cookie_header })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(ua_cache.is_stale(now), false);
    assert_eq!(ua_cache.max_age().as_secs(), 99);
}

#[test]
fn test_do_share_cookies_if_immutable() {
    let now = SystemTime::now();
    let cookie_header = json!({
        "set-cookie": "foo=bar",
        "cache-control": "immutable, max-age=99",
    });

    let proxy_policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": cookie_header })),
        CachePolicyOptions {
            shared: true,
            ..Default::default()
        },
    );

    assert_eq!(proxy_policy.is_stale(now), false);
    assert_eq!(proxy_policy.max_age().as_secs(), 99);
}

#[test]
fn test_cache_explicitly_public_cookie() {
    let now = SystemTime::now();
    let cookie_header = json!({
        "set-cookie": "foo=bar",
        "cache-control": "max-age=5, public",
    });

    let proxy_policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": cookie_header })),
        CachePolicyOptions {
            shared: true,
            ..Default::default()
        },
    );

    assert_eq!(proxy_policy.is_stale(now), false);
    assert_eq!(proxy_policy.max_age().as_secs(), 5);
}

#[test]
fn test_miss_max_age_equals_zero() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "public, max-age=0",
            },
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_uncacheable_503() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "status": 503,
            "headers": {
                "cache-control": "public, max-age=0",
            },
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_cacheable_301() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "status": 301,
            "headers": {
                "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
            },
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
}

#[test]
fn test_uncacheable_303() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "status": 303,
            "headers": {
                "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
            },
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_cacheable_303() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "status": 303,
            "headers": {
                "cache-control": "max-age=1000",
            },
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
}

#[test]
fn test_uncacheable_412() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "status": 412,
            "headers": {
                "cache-control": "public, max-age=1000",
            },
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), true);
    assert_eq!(policy.max_age().as_secs(), 0);
}

#[test]
fn test_expired_expires_cache_with_max_age() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "public, max-age=9999",
                "expires": "Sat, 07 May 2016 15:35:18 GMT",
            },
        })),
        Default::default(),
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!(policy.max_age().as_secs(), 9999);
}

#[test]
fn test_expired_expires_cached_with_s_maxage() {
    let now = SystemTime::now();
    let s_max_age_headers = json!({
        "cache-control": "public, s-maxage=9999",
        "expires": "Sat, 07 May 2016 15:35:18 GMT",
    });

    let proxy_policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": s_max_age_headers,
        })),
        Default::default(),
    );

    assert_eq!(proxy_policy.is_stale(now), false);
    assert_eq!(proxy_policy.max_age().as_secs(), 9999);

    let ua_policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": s_max_age_headers,
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(ua_policy.is_stale(now), true);
    assert_eq!(ua_policy.max_age().as_secs(), 0);
}

#[test]
fn test_when_urls_match() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "uri": "/",
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "uri": "/",
                    "headers": {},
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );
}

#[test]
fn test_not_when_urls_mismatch() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "uri": "/foo",
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        Default::default(),
    );

    assert!(!policy
        .before_request(
            &req(json!({
                "uri": "/foo?bar",
                "headers": {},
            })),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn test_when_methods_match() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        Default::default(),
    );

    assert!(
        policy
            .before_request(
                &req(json!({
                    "method": "GET",
                    "headers": {},
                })),
                now
            )
            .satisfies_without_revalidation(),
        "{:?}",
        policy
    );
}

#[test]
fn test_not_when_hosts_mismatch() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "host": "foo",
            },
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        Default::default(),
    );

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "host": "foo",
                },
            })),
            now
        )
        .satisfies_without_revalidation());

    assert!(!policy
        .before_request(
            &req(json!({
                "headers": {
                    "host": "foofoo",
                },
            })),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn test_when_methods_match_head() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "HEAD",
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "method": "HEAD",
                    "headers": {},
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );
}

#[test]
fn test_not_when_methods_mismatch() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "POST",
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        Default::default(),
    );

    assert!(!policy
        .before_request(
            &req(json!({
                "method": "GET",
                "headers": {},
            })),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn test_not_when_methods_mismatch_head() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "method": "HEAD",
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "method": "GET",
                    "headers": {},
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_not_when_proxy_revalidating() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2, proxy-revalidate ",
            },
        })),
        Default::default(),
    );

    assert!(!policy
        .before_request(
            &req(json!({
                "headers": {},
            })),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn test_when_not_a_proxy_revalidating() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2, proxy-revalidate ",
            },
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {},
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );
}

#[test]
fn test_not_when_no_cache_requesting() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "fine",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "no-cache",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "cache-control": "no-cache",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_vary_basic() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "weather": "nice",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=5",
                "vary": "weather",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "nice",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "bad",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_asterisks_does_not_match() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "weather": "ok",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=5",
                "vary": "*",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "ok",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_asterisks_is_stale() {
    let now = SystemTime::now();
    let policy_one = CachePolicy::new(
        &req(json!({
            "headers": {
                "weather": "ok",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "public,max-age=99",
                "vary": "*",
            },
        })),
        Default::default(),
    );

    let policy_two = CachePolicy::new(
        &req(json!({
            "headers": {
                "weather": "ok",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "public,max-age=99",
                "vary": "weather",
            },
        })),
        Default::default(),
    );

    assert_eq!(policy_one.is_stale(now), true);
    assert_eq!(policy_two.is_stale(now), false);
}

#[test]
fn test_values_are_case_sensitive() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "weather": "BAD",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "public,max-age=5",
                "vary": "Weather",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "BAD",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "bad",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_irrelevant_headers_ignored() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "weather": "nice",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=5",
                "vary": "moon-phase",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "bad",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "shining",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "moon-phase": "full",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_absence_is_meaningful() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "weather": "nice",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=5",
                "vary": "moon-phase, weather",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "nice",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "nice",
                        "moon-phase": "",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {},
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_all_values_must_match() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "sun": "shining",
                "weather": "nice",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=5",
                "vary": "weather, sun",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "sun": "shining",
                        "weather": "nice",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "sun": "shining",
                        "weather": "bad",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_whitespace_is_okay() {
    let now = SystemTime::now();
    let policy = CachePolicy::new(
        &req(json!({
            "headers": {
                "sun": "shining",
                "weather": "nice",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=5",
                "vary": "    weather       ,     sun     ",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "sun": "shining",
                        "weather": "nice",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "nice",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );

    assert_eq!(
        policy
            .before_request(
                &req(json!({
                    "headers": {
                        "sun": "shining",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        false
    );
}

#[test]
fn test_order_is_irrelevant() {
    let now = SystemTime::now();
    let policy_one = CachePolicy::new(
        &req(json!({
            "headers": {
                "sun": "shining",
                "weather": "nice",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=5",
                "vary": "weather, sun",
            },
        })),
        Default::default(),
    );

    let policy_two = CachePolicy::new(
        &req(json!({
            "headers": {
                "sun": "shining",
                "weather": "nice",
            },
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=5",
                "vary": "sun, weather",
            },
        })),
        Default::default(),
    );

    assert_eq!(
        policy_one
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "nice",
                        "sun": "shining",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy_one
            .before_request(
                &req(json!({
                    "headers": {
                        "sun": "shining",
                        "weather": "nice",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy_two
            .before_request(
                &req(json!({
                    "headers": {
                        "weather": "nice",
                        "sun": "shining",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );

    assert_eq!(
        policy_two
            .before_request(
                &req(json!({
                    "headers": {
                        "sun": "shining",
                        "weather": "nice",
                    },
                })),
                now
            )
            .satisfies_without_revalidation(),
        true
    );
}
