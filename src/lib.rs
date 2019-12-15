//! Determines whether a given HTTP response can be cached and whether a cached response can be
//! reused, following the rules specified in [RFC 7234](https://httpwg.org/specs/rfc7234.html).

#![warn(missing_docs)]
// TODO: turn these warnings back on once everything is implemented
#![allow(unused_variables)]

use http::request::Parts as Request;
use http::response::Parts as Response;
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};

lazy_static! {
    static ref STATUS_CODE_CACHEABLE_BY_DEFAULT: HashSet<i32> = {
        let mut set = HashSet::new();
        set.insert(200);
        set.insert(203);
        set.insert(204);
        set.insert(206);
        set.insert(300);
        set.insert(301);
        set.insert(404);
        set.insert(405);
        set.insert(410);
        set.insert(414);
        set.insert(501);
        set
    };
}

lazy_static! {
    static ref UNDERSTOOD_STATUSES: HashSet<i32> = {
        let mut set = HashSet::new();
        set.insert(200);
        set.insert(203);
        set.insert(204);
        set.insert(300);
        set.insert(301);
        set.insert(302);
        set.insert(303);
        set.insert(307);
        set.insert(308);
        set.insert(404);
        set.insert(405);
        set.insert(410);
        set.insert(414);
        set.insert(501);
        set
    };
}

lazy_static! {
    static ref HOP_BY_HOP_HEADERS: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert("date");
        set.insert("connection");
        set.insert("keep-alive");
        set.insert("proxy-authentication");
        set.insert("proxy-authorization");
        set.insert("te");
        set.insert("trailer");
        set.insert("transfer-encoding");
        set.insert("upgrade");
        set
    };
}

lazy_static! {
    static ref EXCLUDED_FROM_REVALIDATION_UPDATE: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert("content-length");
        set.insert("content-encoding");
        set.insert("transfer-encoding");
        set.insert("content-range");
        set
    };
}

/// Holds configuration options which control the behavior of the cache and are independent of
/// any specific request or response.
#[derive(Debug, Clone)]
pub struct CacheOptions {
    /// If `shared` is `true` (default), then the response is evaluated from a perspective of a
    /// shared cache (i.e. `private` is not cacheable and `s-maxage` is respected). If `shared`
    /// is `false`, then the response is evaluated from a perspective of a single-user cache
    /// (i.e. `private` is cacheable and `s-maxage` is ignored). `shared: true` is recommended
    /// for HTTP clients.
    pub shared: bool,

    /// If `ignore_cargo_cult` is `true`, common anti-cache directives will be completely
    /// ignored if the non-standard `pre-check` and `post-check` directives are present. These
    /// two useless directives are most commonly found in bad StackOverflow answers and PHP's
    /// "session limiter" defaults.
    pub ignore_cargo_cult: bool,

    /// If `trust_server_date` is `false`, then server's `Date` header won't be used as the
    /// base for `max-age`. This is against the RFC, but it's useful if you want to cache
    /// responses with very short `max-age`, but your local clock is not exactly in sync with
    /// the server's.
    pub trust_server_date: bool,

    /// `cache_heuristic` is a fraction of response's age that is used as a fallback
    /// cache duration. The default is 0.1 (10%), e.g. if a file hasn't been modified for 100
    /// days, it'll be cached for 100*0.1 = 10 days.
    pub cache_heuristic: f32,

    /// `immutable_min_time_to_live` is a number of seconds to assume as the default time to
    /// cache responses with `Cache-Control: immutable`. Note that per RFC these can become
    /// stale, so `max-age` still overrides the default.
    pub immutable_min_time_to_live: u32,

    // Allow more fields to be added later without breaking callers.
    _hidden: (),
}

impl Default for CacheOptions {
    fn default() -> Self {
        CacheOptions {
            shared: true,
            ignore_cargo_cult: false,
            trust_server_date: true,
            cache_heuristic: 0.1, // 10% matches IE
            immutable_min_time_to_live: 86400,
            _hidden: (),
        }
    }
}

/// Identifies when responses can be reused from a cache, taking into account HTTP RFC 7234 rules
/// for user agents and shared caches. It's aware of many tricky details such as the Vary header,
/// proxy revalidation, and authenticated responses.
#[derive(Debug)]
pub struct CachePolicy;

impl CacheOptions {
    /// Cacheability of an HTTP response depends on how it was requested, so both request and
    /// response are required to create the policy.
    pub fn policy_for(&self, request: &Request, response: &Response) -> CachePolicy {
        CachePolicy
    }
}

// While these methods are all unimplemented, we don't expect them to all appear used.
#[allow(dead_code)]
impl CachePolicy {
    /// Returns `true` if the response can be stored in a cache. If it's `false` then you MUST NOT
    /// store either the request or the response.
    pub fn is_storable(&self) -> bool {
        unimplemented!();
    }

    /// Returns approximate time in _milliseconds_ until the response becomes stale (i.e. not
    /// fresh).
    ///
    /// After that time (when `time_to_live() <= 0`) the response might not be usable without
    /// revalidation. However, there are exceptions, e.g. a client can explicitly allow stale
    /// responses, so always check with `is_cached_response_fresh()`.
    pub fn time_to_live(&self) -> u32 {
        unimplemented!();
    }

    /// Returns whether the cached response is still fresh in the context of the new request.
    ///
    /// If it returns `true`, then the given request matches the original response this cache
    /// policy has been created with, and the response can be reused without contacting the server.
    ///
    /// If it returns `false`, then the response may not be matching at all (e.g. it's for a
    /// different URL or method), or may require to be refreshed first. Either way, the new
    /// request's headers will have been updated for sending it to the origin server.
    pub fn is_cached_response_fresh(
        &self,
        new_request: &mut Request,
        cached_response: &Response,
    ) -> bool {
        unimplemented!();
    }

    /// Use this method to update the policy state after receiving a new response from the origin
    /// server. The updated `CachePolicy` should be saved to the cache along with the new response.
    ///
    /// Returns whether the cached response body is still valid. If `true`, then a valid 304 Not
    /// Modified response has been received, and you can reuse the old cached response body. If
    /// `false`, you should use new response's body (if present), or make another request to the
    /// origin server without any conditional headers (i.e. don't use `is_cached_response_fresh`
    /// this time) to get the new resource.
    pub fn is_cached_response_valid(
        &mut self,
        new_request: &Request,
        cached_response: &Response,
        new_response: &Response,
    ) -> bool {
        unimplemented!();
    }

    /// Updates and filters the response headers for a cached response before returning it to a
    /// client. This function is necessary, because proxies MUST always remove hop-by-hop headers
    /// (such as TE and Connection) and update response's Age to avoid doubling cache time.
    pub fn update_response_headers(&self, headers: &mut Response) {
        unimplemented!();
    }

    fn is_stale(&self) -> bool {
        unimplemented!();
    }

    fn age(&self) -> u32 {
        unimplemented!();
    }

    fn max_age(&self) -> u32 {
        unimplemented!();
    }

    fn response_headers(&self) -> HashMap<String, String> {
        unimplemented!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;
    use http::response::Parts as ResponseParts;
    use http::{header, HeaderValue, Method, Request, Response};

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(Request::builder().header(header::CACHE_CONTROL, "no-store")),
            &response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=60")),
        );

        assert!(!policy.is_storable());
    }

    #[test]
    fn test_request_max_age() {
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

        let response =
            response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=60"));

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

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
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(Request::builder()),
            &response_parts(
                Response::builder().header(header::WARNING, "199 test danger, 200 ok ok"),
            ),
        );

        assert_eq!(
            "200 ok ok",
            policy.response_headers()[header::WARNING.as_str()]
        );
    }

    #[test]
    fn test_do_not_cache_partial_response() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };
        let policy = options.policy_for(
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

    fn public_cacheable_response() -> ResponseParts {
        response_parts(Response::builder().header(header::CACHE_CONTROL, "public, max-age=222"))
    }

    fn cacheable_response() -> ResponseParts {
        response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=111"))
    }

    #[test]
    fn test_no_store_kills_cache() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(
                Request::builder()
                    .method(Method::GET)
                    .header(header::CACHE_CONTROL, "no-store"),
            ),
            &public_cacheable_response(),
        );

        assert!(policy.is_stale());
        assert!(!policy.is_storable());
    }

    #[test]
    fn test_post_not_cacheable_by_default() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::POST)),
            &response_parts(Response::builder().header(header::CACHE_CONTROL, "public")),
        );

        assert!(policy.is_stale());
        assert!(!policy.is_storable());
    }

    #[test]
    fn test_post_cacheable_explicitly() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::POST)),
            &public_cacheable_response(),
        );

        assert!(!policy.is_stale());
        assert!(policy.is_storable());
    }

    #[test]
    fn test_public_cacheable_auth_is_ok() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(
                Request::builder()
                    .method(Method::GET)
                    .header(header::AUTHORIZATION, "test"),
            ),
            &public_cacheable_response(),
        );

        assert!(!policy.is_stale());
        assert!(policy.is_storable());
    }

    /*
    #[test]
    fn test_proxy_cacheable_auth_is_ok() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {
                    "authorization": "test",
                }
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=0,s-maxage=12",
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert_eq!(policy.is_storable(), true);

        let policy_two = CachePolicy::from_object(HashMap::new());
        // TODO: assert(cache2 instanceof CachePolicy);

        assert_eq!(!policy_two.is_stale(), true);
        assert_eq!(policy_two.is_storable(), true);
    }
    */

    #[test]
    fn test_private_auth_is_ok() {
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(
                Request::builder()
                    .method(Method::GET)
                    .header(header::AUTHORIZATION, "test"),
            ),
            &cacheable_response(),
        );

        assert!(!policy.is_stale());
        assert!(policy.is_storable());
    }

    #[test]
    fn test_revalidate_auth_is_ok() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(
                Request::builder()
                    .method(Method::GET)
                    .header(header::AUTHORIZATION, "test"),
            ),
            &response_parts(
                Response::builder().header(header::CACHE_CONTROL, "max-age=88,must-revalidate"),
            ),
        );

        assert!(policy.is_storable());
    }

    #[test]
    fn test_auth_prevents_caching_by_default() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(
                Request::builder()
                    .method(Method::GET)
                    .header(header::AUTHORIZATION, "test"),
            ),
            &cacheable_response(),
        );

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.is_storable(), false);
    }

    #[test]
    fn test_simple_miss() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::GET)),
            &response_parts(Response::builder()),
        );

        assert!(policy.is_stale());
    }

    #[test]
    fn test_simple_hit() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::GET)),
            &response_parts(
                Response::builder().header(header::CACHE_CONTROL, "public, max-age=999999"),
            ),
        );

        assert!(!policy.is_stale());
        assert_eq!(policy.max_age(), 999999);
    }

    /*
    #[test]
    fn test_weird_syntax() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "cache-control": ",,,,max-age =  456      ,"
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert_eq!(policy.max_age(), 456);

        let policy_two = CachePolicy::from_object(HashMap::new());
        // TODO: assert(cache2 instanceof CachePolicy);

        assert_eq!(policy_two.is_stale(), false);
        assert_eq!(policy_two.max_age(), 456);
    }
    */

    #[test]
    fn test_quoted_syntax() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::GET)),
            &response_parts(
                Response::builder().header(header::CACHE_CONTROL, "  max-age = \"678\"      "),
            ),
        );

        assert!(!policy.is_stale());
        assert_eq!(policy.max_age(), 678);
    }

    #[test]
    fn test_iis() {
        let options = CacheOptions {
            shared: false,
            ..CacheOptions::default()
        };

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::GET)),
            &response_parts(
                Response::builder()
                    .header(header::CACHE_CONTROL, "private, public, max-age=259200"),
            ),
        );

        assert!(!policy.is_stale());
        assert_eq!(policy.max_age(), 259200);
    }

    #[test]
    fn test_pre_check_tolerated() {
        let options = CacheOptions {
            ..CacheOptions::default()
        };
        let cache_control = "pre-check=0, post-check=0, no-store, no-cache, max-age=100";

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::GET)),
            &response_parts(Response::builder().header(header::CACHE_CONTROL, cache_control)),
        );

        assert!(policy.is_stale());
        assert!(!policy.is_storable());
        assert_eq!(policy.max_age(), 0);
        assert_eq!(
            policy.response_headers()[header::CACHE_CONTROL.as_str()],
            cache_control
        );
    }

    #[test]
    fn test_pre_check_poison() {
        let options = CacheOptions {
            ignore_cargo_cult: true,
            ..CacheOptions::default()
        };

        let original_cache_control =
            "pre-check=0, post-check=0, no-cache, no-store, max-age=100, custom, foo=bar";

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::GET)),
            &response_parts(
                Response::builder()
                    .header(header::CACHE_CONTROL, original_cache_control)
                    .header(header::PRAGMA, "no-cache"),
            ),
        );

        assert!(!policy.is_stale());
        assert!(policy.is_storable());
        assert_eq!(policy.max_age(), 100);

        let cache_control_header = &policy.response_headers()[header::CACHE_CONTROL.as_str()];
        assert!(!cache_control_header.contains("pre-check"));
        assert!(!cache_control_header.contains("post-check"));
        assert!(!cache_control_header.contains("no-store"));

        assert!(cache_control_header.contains("max-age=100"));
        assert!(cache_control_header.contains("custom"));
        assert!(cache_control_header.contains("foo=bar"));

        assert!(!policy
            .response_headers()
            .contains_key(header::PRAGMA.as_str()));
    }

    #[test]
    fn test_pre_check_poison_undefined_header() {
        let options = CacheOptions {
            ignore_cargo_cult: true,
            ..CacheOptions::default()
        };

        let original_cache_control = "pre-check=0, post-check=0, no-cache, no-store";

        let policy = options.policy_for(
            &request_parts(Request::builder().method(Method::GET)),
            &response_parts(
                Response::builder()
                    .header(header::CACHE_CONTROL, original_cache_control)
                    .header(header::EXPIRES, "yesterday!"),
            ),
        );

        assert!(policy.is_stale());
        assert!(policy.is_storable());
        assert_eq!(policy.max_age(), 0);

        assert!(!policy
            .response_headers()
            .contains_key(header::CACHE_CONTROL.as_str()));
        assert!(!policy
            .response_headers()
            .contains_key(header::EXPIRES.as_str()));
    }

    /*
    #[test]
    fn test_cache_with_expires() {
        let local_time = Local::now();
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "date": "TODO: How does time work??", // new Date(now).toGMTString()
                    "expires": "TODO: How does time work??" // new Date(now + 2000).toGMTString()
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert_eq!(policy.max_age(), 2);
    }

    #[test]
    fn test_cache_with_expires_always_relative_to_date() {
        let local_time = Local::now();
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "date": "TODO: How does time work??", // new Date(now - 3000).toGMTString()
                    "expires": "TODO: How does time work??" // new Date(now).toGMTString()
                }
            }),
        );

        assert_eq!(policy.max_age(), 3);
    }

    #[test]
    fn test_cache_expires_no_date() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "public",
                    "expires": "TODO: How does time work??" // new Date(Date.now() + 3600 * 1000).toGMTString()
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert!(policy.max_age() > 3595);
        assert!(policy.max_age() < 3605);
    }

    #[test]
    fn test_ages() {
        // TODO: Need to figure out how "subclassing" works in Rust
        // Link to function in JS: https://github.com/kornelski/http-cache-semantics/blob/master/test/responsetest.js#L158
        assert!(false);
    }

    #[test]
    fn test_age_can_make_stale() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=100",
                    "age": "101"
                }
            }),
        );

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.is_storable(), true);
    }

    #[test]
    fn test_age_not_always_stale() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=20",
                    "age": "15"
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert_eq!(policy.is_storable(), true);
    }

    #[test]
    fn test_bogus_age_ignored() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=20",
                    "age": "golden"
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert_eq!(policy.is_storable(), true);
    }

    #[test]
    fn test_cache_old_files() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "date": "TODO: How does time work??", // new Date().toGMTString()
                    "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT"
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert!(policy.max_age() > 100);
    }

    #[test]
    fn test_immutable_simple_hit() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "immutable, max-age=999999",
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert_eq!(policy.max_age(), 999999);
    }

    #[test]
    fn test_immutable_can_expire() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "immutable, max-age=0",
                }
            }),
        );

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.max_age(), 0);
    }

    #[test]
    fn test_cache_immutable_files() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "date": "TODO: How does time work??", // new Date().toGMTString()
                    "cache-control": "immutable",
                    "last-modified": "TODO: How does time work??", // new Date().toGMTString()
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert!(policy.max_age() > 100);
    }

    #[test]
    fn test_immutable_can_be_off() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "date": "TODO: How does time work??", // new Date().toGMTString()
                    "cache-control": "immutable",
                    "last-modified": "TODO: How does time work??", // new Date().toGMTString()
                }
            }),
        )
        .with_immutable_min_time_to_live(0);

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.max_age(), 0);
    }

    #[test]
    fn test_pragma_no_cache() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "pragma": "no-cache",
                    "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
                }
            }),
        );

        assert_eq!(policy.is_stale(), true);
    }

    #[test]
    fn test_blank_cache_control_and_pragma_no_cache() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "",
                    "pragma": "no-cache",
                    "last-modified": "TODO: How does time work??", // new Date().toGMTString()
                }
            }),
        );

        assert_eq!(policy.is_stale(), false);
    }

    #[test]
    fn test_no_store() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "no-store, public, max-age=1",
                }
            }),
        );

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.max_age(), 0);
    }

    #[test]
    fn test_observe_private_cache() {
        let private_header = json!({
            "cache-control": "private, max-age=1234",
        });

        let proxy_policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({ "headers": private_header }),
        );

        assert_eq!(proxy_policy.is_stale(), true);
        assert_eq!(proxy_policy.max_age(), 0);

        let ua_cache = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({ "headers": private_header }),
        )
        .with_shared(false);

        assert_eq!(ua_cache.is_stale(), false);
        assert_eq!(ua_cache.max_age(), 1234);
    }

    #[test]
    fn test_do_not_share_cookies() {
        let cookie_header = json!({
            "set-cookie": "foo=bar",
            "cache-control": "max-age=99",
        });

        let proxy_policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({ "headers": cookie_header }),
        )
        .with_shared(true);

        assert_eq!(proxy_policy.is_stale(), true);
        assert_eq!(proxy_policy.max_age(), 0);

        let ua_cache = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({ "headers": cookie_header }),
        )
        .with_shared(false);

        assert_eq!(ua_cache.is_stale(), false);
        assert_eq!(ua_cache.max_age(), 99);
    }

    #[test]
    fn test_do_share_cookies_if_immutable() {
        let cookie_header = json!({
            "set-cookie": "foo=bar",
            "cache-control": "immutable, max-age=99",
        });

        let proxy_policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({ "headers": cookie_header }),
        )
        .with_shared(true);

        assert_eq!(proxy_policy.is_stale(), false);
        assert_eq!(proxy_policy.max_age(), 99);
    }

    #[test]
    fn test_cache_explicitly_public_cookie() {
        let cookie_header = json!({
            "set-cookie": "foo=bar",
            "cache-control": "max-age=5, public",
        });

        let proxy_policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({ "headers": cookie_header }),
        )
        .with_shared(true);

        assert_eq!(proxy_policy.is_stale(), false);
        assert_eq!(proxy_policy.max_age(), 5);
    }

    #[test]
    fn test_miss_max_age_equals_zero() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "public, max-age=0",
                },
            }),
        );

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.max_age(), 0);
    }

    #[test]
    fn test_uncacheable_503() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "status": 503,
                "headers": {
                    "cache-control": "public, max-age=0",
                },
            }),
        );

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.max_age(), 0);
    }

    #[test]
    fn test_cacheable_301() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "status": 301,
                "headers": {
                    "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
                },
            }),
        );

        assert_eq!(policy.is_stale(), false);
    }

    #[test]
    fn test_uncacheable_303() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "status": 303,
                "headers": {
                    "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
                },
            }),
        );

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.max_age(), 0);
    }

    #[test]
    fn test_cacheable_303() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "status": 303,
                "headers": {
                    "cache-control": "max-age=1000",
                },
            }),
        );

        assert_eq!(policy.is_stale(), false);
    }

    #[test]
    fn test_uncacheable_412() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "status": 412,
                "headers": {
                    "cache-control": "public, max-age=1000",
                },
            }),
        );

        assert_eq!(policy.is_stale(), true);
        assert_eq!(policy.max_age(), 0);
    }

    #[test]
    fn test_expired_expires_cache_with_max_age() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "public, max-age=9999",
                    "expires": "Sat, 07 May 2016 15:35:18 GMT",
                },
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert_eq!(policy.max_age(), 9999);
    }

    #[test]
    fn test_expired_expires_cached_with_s_maxage() {
        let s_max_age_headers = json!({
            "cache-control": "public, s-maxage=9999",
            "expires": "Sat, 07 May 2016 15:35:18 GMT",
        });

        let proxy_policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": s_max_age_headers,
            }),
        );

        assert_eq!(proxy_policy.is_stale(), false);
        assert_eq!(proxy_policy.max_age(), 9999);

        let ua_policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": s_max_age_headers,
            }),
        )
        .with_shared(false);

        assert_eq!(ua_policy.is_stale(), true);
        assert_eq!(ua_policy.max_age(), 0);
    }

    #[test]
    fn test_max_age_wins_over_future_expires() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "public, max-age=333",
                    "expires": "How does time work???", // new Date(Date.now() + 3600 * 1000).toGMTString()
                },
            }),
        );

        assert_eq!(policy.is_stale(), false);
        assert_eq!(policy.max_age(), 333);
    }

    #[test]
    fn test_remove_hop_headers() {
        // TODO: Need to figure out how "subclassing" works in Rust
        // Link to JavaScript function: https://github.com/kornelski/http-cache-semantics/blob/master/test/responsetest.js#L472
    }

    lazy_static! {
        static ref SIMPLE_REQUEST_REVALIDATE: Value = {
            let request = json!({
                "method": "GET",
                "headers": {
                    "host": "www.w3c.org",
                    "connection": "close",
                    "x-custom": "yes",
                },
                "url": "/Protocols/rfc2616/rfc2616-sec14.html",
            });

            return request;
        };
    }

    fn assert_headers_passed(headers: Value) {
        assert_eq!(headers["connection"], json!(null));
        assert_eq!(headers["x-custom"], "yes");
    }

    fn assert_no_validators(headers: Value) {
        assert_eq!(headers["if-none-match"], json!(null));
        assert_eq!(headers["if-modified-since"], json!(null));
    }

    #[test]
    fn test_ok_if_method_changes_to_head() {
        assert!(false);
    }

    #[test]
    fn test_not_if_method_mismatch_other_than_head() {
        assert!(false);
    }

    #[test]
    fn test_not_if_url_mismatch() {
        assert!(false);
    }

    #[test]
    fn test_not_if_host_mismatch() {
        assert!(false);
    }

    #[test]
    fn test_not_if_vary_fields_prevent() {
        assert!(false);
    }

    #[test]
    fn test_when_entity_tag_validator_is_present() {
        assert!(false);
    }

    #[test]
    fn test_skips_weak_validators_on_post_2() {
        assert!(false);
    }

    #[test]
    fn test_merges_validators() {
        assert!(false);
    }

    #[test]
    fn test_when_last_modified_validator_is_present() {
        assert!(false);
    }

    #[test]
    fn test_not_without_validators() {
        assert!(false);
    }

    #[test]
    fn test_113_added() {
        assert!(false);
    }

    #[test]
    fn test_removes_warnings() {
        assert!(false);
    }

    #[test]
    fn test_must_contain_any_etag() {
        assert!(false);
    }

    #[test]
    fn test_merges_etags() {
        assert!(false);
    }

    #[test]
    fn test_should_send_the_last_modified_value() {
        assert!(false);
    }

    #[test]
    fn test_should_not_send_the_last_modified_value_for_post() {
        assert!(false);
    }

    #[test]
    fn test_should_not_send_the_last_modified_value_for_range_request() {
        assert!(false);
    }

    #[test]
    fn test_when_urls_match() {
        let policy = CachePolicy::new(
            json!({
                "url": "/",
                "headers": {},
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "max-age=2",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "url": "/",
                "headers": {},
            })),
            true
        );
    }

    #[test]
    fn test_when_expires_is_present() {
        let policy = CachePolicy::new(
            json!({
                "headers": {},
            }),
            json!({
                "status": 302,
                "headers": {
                    "expires": "How does time work??", // new Date(Date.now() + 2000).toGMTString()
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {},
            })),
            true
        );
    }

    #[test]
    fn test_not_when_urls_mismatch() {
        let policy = CachePolicy::new(
            json!({
                "url": "/foo",
                "headers": {},
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "max-age=2",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "url": "/foo?bar",
                "headers": {},
            })),
            true
        );
    }

    #[test]
    fn test_when_methods_match() {
        let policy = CachePolicy::new(
            json!({
                "method": "GET",
                "headers": {},
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "'max-age=2",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "method": "GET",
                "headers": {},
            })),
            true
        );
    }

    #[test]
    fn test_not_when_hosts_mismatch() {
        let policy = CachePolicy::new(
            json!({
                "headers": {
                    "host": "foo",
                },
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "max-age=2",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "host": "foo",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "host": "foofoo",
                },
            })),
            true
        );
    }

    #[test]
    fn test_when_methods_match_head() {
        let policy = CachePolicy::new(
            json!({
                "method": "HEAD",
                "headers": {},
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "max-age=2",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "method": "HEAD",
                "headers": {},
            })),
            true
        );
    }

    #[test]
    fn test_not_when_methods_mismatch() {
        let policy = CachePolicy::new(
            json!({
                "method": "POST",
                "headers": {},
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "max-age=2",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "method": "GET",
                "headers": {},
            })),
            true
        );
    }

    #[test]
    fn test_not_when_methods_mismatch_head() {
        let policy = CachePolicy::new(
            json!({
                "method": "HEAD",
                "headers": {},
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "max-age=2",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "method": "GET",
                "headers": {},
            })),
            false
        );
    }

    #[test]
    fn test_not_when_proxy_revalidating() {
        let policy = CachePolicy::new(
            json!({
                "headers": {},
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "max-age=2, proxy-revalidate ",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {},
            })),
            false
        );
    }

    #[test]
    fn test_when_not_a_proxy_revalidating() {
        let policy = CachePolicy::new(
            json!({
                "headers": {},
            }),
            json!({
                "status": 200,
                "headers": {
                    "cache-control": "max-age=2, proxy-revalidate ",
                },
            }),
        )
        .with_shared(false);

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {},
            })),
            true
        );
    }

    #[test]
    fn test_not_when_no_cache_requesting() {
        let policy = CachePolicy::new(
            json!({
                "headers": {},
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=2",
                },
            }),
        )
        .with_shared(false);

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "cache-control": "fine",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "cache-control": "no-cache",
                },
            })),
            false
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "cache-control": "no-cache",
                },
            })),
            false
        );
    }

    lazy_static! {
        static ref SIMPLE_REQUEST_UPDATE: Value = {
            let simple_request = json!({
                "method": "GET",
                "headers": {
                    "host": "www.w3c.org",
                    "connection": "close",
                },
                "url": "/Protocols/rfc2616/rfc2616-sec14.html",
            });

            return simple_request;
        };
    }

    lazy_static! {
        static ref CACHEABLE_RESPONSE: Value = {
            let response = json!({
                "headers": {
                    "cache-control": "max-age=111",
                },
            });

            return response;
        };
    }

    fn not_modified_response_headers() {
        assert!(false);
    }

    fn assert_updates() {
        assert!(false);
    }

    #[test]
    fn test_matching_etags_are_updated() {
        assert!(false);
    }

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

    #[test]
    fn test_vary_basic() {
        let policy = CachePolicy::new(
            json!({
                "headers": {
                    "weather": "nice",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=5",
                    "vary": "weather",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "nice",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "bad",
                },
            })),
            false
        );
    }

    #[test]
    fn test_asterisks_does_not_match() {
        let policy = CachePolicy::new(
            json!({
                "headers": {
                    "weather": "ok",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=5",
                    "vary": "*",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "ok",
                },
            })),
            false
        );
    }

    #[test]
    fn test_asterisks_is_stale() {
        let policy_one = CachePolicy::new(
            json!({
                "headers": {
                    "weather": "ok",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "public,max-age=99",
                    "vary": "*",
                },
            }),
        );

        let policy_two = CachePolicy::new(
            json!({
                "headers": {
                    "weather": "ok",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "public,max-age=99",
                    "vary": "weather",
                },
            }),
        );

        assert_eq!(policy_one.is_stale(), true);
        assert_eq!(policy_two.is_stale(), false);
    }

    #[test]
    fn test_values_are_case_sensitive() {
        let policy = CachePolicy::new(
            json!({
                "headers": {
                    "weather": "BAD",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "public,max-age=5",
                    "vary": "Weather",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "BAD",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "bad",
                },
            })),
            false
        );
    }

    #[test]
    fn test_irrelevant_headers_ignored() {
        let policy = CachePolicy::new(
            json!({
                "headers": {
                    "weather": "nice",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=5",
                    "vary": "moon-phase",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "bad",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "shining",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "moon-phase": "full",
                },
            })),
            false
        );
    }

    #[test]
    fn test_absence_is_meaningful() {
        let policy = CachePolicy::new(
            json!({
                "headers": {
                    "weather": "nice",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=5",
                    "vary": "moon-phase, weather",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "nice",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "nice",
                    "moon-phase": "",
                },
            })),
            false
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {},
            })),
            false
        );
    }

    #[test]
    fn test_all_values_must_match() {
        let policy = CachePolicy::new(
            json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=5",
                    "vary": "weather, sun",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "bad",
                },
            })),
            false
        );
    }

    #[test]
    fn test_whitespace_is_okay() {
        let policy = CachePolicy::new(
            json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=5",
                    "vary": "    weather       ,     sun     ",
                },
            }),
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            })),
            true
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "nice",
                },
            })),
            false
        );

        assert_eq!(
            policy.satisfies_without_revalidation(json!({
                "headers": {
                    "sun": "shining",
                },
            })),
            false
        );
    }

    #[test]
    fn test_order_is_irrelevant() {
        let policy_one = CachePolicy::new(
            json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=5",
                    "vary": "weather, sun",
                },
            }),
        );

        let policy_two = CachePolicy::new(
            json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            }),
            json!({
                "headers": {
                    "cache-control": "max-age=5",
                    "vary": "sun, weather",
                },
            }),
        );

        assert_eq!(
            policy_one.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "nice",
                    "sun": "shining",
                },
            })),
            true
        );

        assert_eq!(
            policy_one.satisfies_without_revalidation(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            })),
            true
        );

        assert_eq!(
            policy_two.satisfies_without_revalidation(json!({
                "headers": {
                    "weather": "nice",
                    "sun": "shining",
                },
            })),
            true
        );

        assert_eq!(
            policy_two.satisfies_without_revalidation(json!({
                "headers": {
                    "sun": "shining",
                    "weather": "nice",
                },
            })),
            true
        );
    }

    #[test]
    fn test_thaw_wrong_object() {
        assert!(false);
    }

    #[test]
    fn test_missing_headers() {
        assert!(false);
    }

    #[test]
    fn test_github_response_with_small_clock_skew() {
        assert!(false);
    }
    */
}
