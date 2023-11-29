//! Determines whether a given HTTP response can be cached and whether a
//! cached response can be reused, following the rules specified in [RFC
//! 7234](https://httpwg.org/specs/rfc7234.html).

use http::header::HeaderName;
use http::header::HeaderValue;
use http::Request;
use http::Response;
use http_cache_semantics::*;
use serde_json::json;
use serde_json::Value;
use std::time::SystemTime;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

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
    let now = SystemTime::now();
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

    let policy = CachePolicy::new_options(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(response),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );

    assert_eq!(
        should_put,
        policy.is_storable(),
        "{should_put}; {response_code}; {policy:#?}"
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
    let policy = CachePolicy::new_options(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "last-modified": format_date(-105, 1),
                "date": format_date(-5, 1),
            },
            "body": "A"
        })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.time_to_live(now).as_secs() >= 4);
}

#[test]
fn test_default_expiration_date_fully_cached_for_more_than_24_hours() {
    let now = SystemTime::now();
    let policy = CachePolicy::new_options(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "last-modified": format_date(-105, 3600 * 24),
                "date": format_date(-5, 3600 * 24),
            },
            "body": "A"
        })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!((policy.time_to_live(now) + policy.age(now)).as_secs() >= 10 * 3600 * 24);
    assert!(policy.time_to_live(now).as_secs() >= 5 * 3600 * 24 - 1);
}

#[test]
fn test_max_age_preferred_over_lower_shared_max_age() {
    let now = SystemTime::now();
    let policy = CachePolicy::new_options(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "date": format_date(-2, 60),
                "cache-control": "s-maxage=60, max-age=180",
            }
        })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 180);
}

fn request_method_not_cached(method: String) {
    // 1. seed the cache (potentially)
    // 2. expect a cache hit or miss
    let now = SystemTime::now();
    let policy = CachePolicy::new_options(
        &req(json!({
            "method": method,
            "headers": {}
        })),
        &res(json!({
            "headers": {
                "expires": format_date(1, 3600),
            }
        })),
        now,
        CacheOptions {
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
    let policy = CachePolicy::new_options(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "etag": "v1",
                "last-modified": format_date(-2, 3600),
                "expires": format_date(1, 3600),
            }
        })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy.time_to_live(now).as_secs() > 0);
}

#[test]
fn test_client_side_no_store() {
    let now = SystemTime::now();
    let policy = CachePolicy::new_options(
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
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(policy.is_storable(), false);
}

#[test]
fn test_request_min_fresh() {
    let now = SystemTime::now();
    let policy = CachePolicy::new_options(
        &Request::get("http://example.com").body(()).unwrap(),
        &res(json!({
            "headers": {
                "cache-control": "max-age=60",
            }
        })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(policy.is_stale(now), false);

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "cache-control": "min-fresh=10",
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
    );

    assert_eq!(policy.is_storable(), false);
}

fn format_date(delta: i64, unit: i64) -> String {
    let now = OffsetDateTime::now_utc();
    let timestamp = now.unix_timestamp() + delta * unit;

    let date = OffsetDateTime::from_unix_timestamp(timestamp).unwrap();
    date.format(&Rfc2822).unwrap()
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
    );

    assert!(policy.is_stale(now));
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
    );

    assert!(policy.is_stale(now));
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert!(policy.is_storable());
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert!(policy.is_storable());
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert!(policy.is_storable());

    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&policy).unwrap();
        let policy: CachePolicy = serde_json::from_str(&json).unwrap();

        assert!(!policy.is_stale(now));
        assert!(policy.is_storable());
    }
}

#[test]
fn test_private_auth_is_ok() {
    let now = SystemTime::now();
    let policy = CachePolicy::new_options(
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
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(policy.is_stale(now), false);
    assert!(policy.is_storable());
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
    );

    assert!(policy.is_storable());
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
    );

    assert!(policy.is_stale(now));
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
    );

    assert!(policy.is_stale(now));
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 999999);
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 456);

    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&policy).unwrap();
        let policy: CachePolicy = serde_json::from_str(&json).unwrap();

        assert_eq!(policy.is_stale(now), false);
        assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 456);
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 678);
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
    );

    assert!(policy.is_stale(now));
    assert!(policy.is_storable());
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert!(policy.is_storable());
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert!(policy.is_storable());
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 999999);
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
    );

    assert!(policy.is_stale(now));
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 0);
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
    );

    assert!(policy.is_stale(now));
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
    );

    assert!(policy.is_stale(now));
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 0);
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
    );

    assert!(proxy_policy.is_stale(now));
    assert_eq!((proxy_policy.time_to_live(now) + proxy_policy.age(now)).as_secs(), 0);

    let ua_cache = CachePolicy::new_options(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": private_header })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(ua_cache.is_stale(now), false);
    assert_eq!(ua_cache.time_to_live(now).as_secs(), 1234);
}

#[test]
fn test_do_not_share_cookies() {
    let now = SystemTime::now();
    let cookie_header = json!({
        "set-cookie": "foo=bar",
        "cache-control": "max-age=99",
    });

    let proxy_policy = CachePolicy::new_options(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": cookie_header })),
        now,
        CacheOptions {
            shared: true,
            ..Default::default()
        },
    );

    assert!(proxy_policy.is_stale(now));
    assert_eq!((proxy_policy.time_to_live(now) + proxy_policy.age(now)).as_secs(), 0);

    let ua_cache = CachePolicy::new_options(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": cookie_header })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert_eq!(ua_cache.is_stale(now), false);
    assert_eq!(ua_cache.time_to_live(now).as_secs(), 99);
}

#[test]
fn test_do_share_cookies_if_immutable() {
    let now = SystemTime::now();
    let cookie_header = json!({
        "set-cookie": "foo=bar",
        "cache-control": "immutable, max-age=99",
    });

    let proxy_policy = CachePolicy::new_options(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": cookie_header })),
        now,
        CacheOptions {
            shared: true,
            ..Default::default()
        },
    );

    assert_eq!(proxy_policy.is_stale(now), false);
    assert_eq!((proxy_policy.time_to_live(now) + proxy_policy.age(now)).as_secs(), 99);
}

#[test]
fn test_cache_explicitly_public_cookie() {
    let now = SystemTime::now();
    let cookie_header = json!({
        "set-cookie": "foo=bar",
        "cache-control": "max-age=5, public",
    });

    let proxy_policy = CachePolicy::new_options(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({ "headers": cookie_header })),
        now,
        CacheOptions {
            shared: true,
            ..Default::default()
        },
    );

    assert_eq!(proxy_policy.is_stale(now), false);
    assert_eq!((proxy_policy.time_to_live(now) + proxy_policy.age(now)).as_secs(), 5);
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
    );

    assert!(policy.is_stale(now));
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 0);
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
    );

    assert!(policy.is_stale(now));
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 0);
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
    );

    assert!(policy.is_stale(now));
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 0);
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
    );

    assert!(policy.is_stale(now));
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 0);
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
    );

    assert_eq!(policy.is_stale(now), false);
    assert_eq!((policy.time_to_live(now) + policy.age(now)).as_secs(), 9999);
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
    );

    assert_eq!(proxy_policy.is_stale(now), false);
    assert_eq!((proxy_policy.time_to_live(now) + proxy_policy.age(now)).as_secs(), 9999);

    let ua_policy = CachePolicy::new_options(
        &req(json!({
            "method": "GET",
            "headers": {},
        })),
        &res(json!({
            "headers": s_max_age_headers,
        })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(ua_policy.is_stale(now));
    assert_eq!((ua_policy.time_to_live(now) + ua_policy.age(now)).as_secs(), 0);
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
    );

    assert!(policy
        .before_request(
            &req(json!({
                "uri": "/",
                "headers": {},
            })),
            now
        )
        .satisfies_without_revalidation());
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
        "{policy:?}"
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
    );

    assert!(policy
        .before_request(
            &req(json!({
                "method": "HEAD",
                "headers": {},
            })),
            now
        )
        .satisfies_without_revalidation());
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
    let policy = CachePolicy::new_options(
        &req(json!({
            "headers": {},
        })),
        &res(json!({
            "status": 200,
            "headers": {
                "cache-control": "max-age=2, proxy-revalidate ",
            },
        })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy
        .before_request(
            &req(json!({
                "headers": {},
            })),
            now
        )
        .satisfies_without_revalidation());
}

#[test]
fn test_not_when_no_cache_requesting() {
    let now = SystemTime::now();
    let policy = CachePolicy::new_options(
        &req(json!({
            "headers": {},
        })),
        &res(json!({
            "headers": {
                "cache-control": "max-age=2",
            },
        })),
        now,
        CacheOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "cache-control": "fine",
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
    );

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "weather": "nice",
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
    );

    assert!(policy_one.is_stale(now));
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
    );

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "weather": "BAD",
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
    );

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "weather": "bad",
                },
            })),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "weather": "shining",
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
    );

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "weather": "nice",
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
    );

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
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
    );

    assert!(policy
        .before_request(
            &req(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
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
    );

    assert!(policy_one
        .before_request(
            &req(json!({
                "headers": {
                    "weather": "nice",
                    "sun": "shining",
                },
            })),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy_one
        .before_request(
            &req(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            })),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy_two
        .before_request(
            &req(json!({
                "headers": {
                    "weather": "nice",
                    "sun": "shining",
                },
            })),
            now
        )
        .satisfies_without_revalidation());

    assert!(policy_two
        .before_request(
            &req(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            })),
            now
        )
        .satisfies_without_revalidation());
}
