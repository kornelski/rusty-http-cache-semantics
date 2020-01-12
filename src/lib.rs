//! Determines whether a given HTTP response can be cached and whether a cached response can be
//! reused, following the rules specified in [RFC 7234](https://httpwg.org/specs/rfc7234.html).

#![warn(missing_docs)]
// TODO: turn these warnings back on once everything is implemented
#![allow(unused_variables)]

use http::request::Parts as Request;
use http::response::Parts as Response;
use http::{HeaderMap, HeaderValue};
use lazy_static::lazy_static;
use std::collections::HashSet;

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
    pub fn new_unshared() -> Self {
        Self {
            shared: false,
            ..Self::default()
        }
    }

    pub fn new_with_trust_server_date_option(trust_server_date: bool) -> Self {
        Self {
            trust_server_date,
            ..Self::default()
        }
    }

    pub fn new_with_ignore_cargo_cult_option(ignore_cargo_cult: bool) -> Self {
        Self {
            ignore_cargo_cult,
            ..Self::default()
        }
    }

    pub fn new_with_immutable_min_time_to_live_option(immutable_min_time_to_live: u32) -> Self {
        Self {
            immutable_min_time_to_live,
            ..Self::default()
        }
    }

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

    pub fn is_stale(&self) -> bool {
        unimplemented!();
    }

    pub fn revalidation_headers(&self, request: &mut Request) -> HeaderMap<HeaderValue> {
        unimplemented!();
    }

    pub fn response_headers(&self) -> HeaderMap<HeaderValue> {
        unimplemented!();
    }

    pub fn age(&self) -> u32 {
        unimplemented!();
    }

    pub fn max_age(&self) -> u32 {
        unimplemented!();
    }
}
