#![warn(missing_docs)]
#![deny(unconditional_recursion)]
//! Tells when responses can be reused from a cache, taking into account [HTTP RFC 7234](http://httpwg.org/specs/rfc7234.html) rules for user agents and shared caches.
//! It's aware of many tricky details such as the `Vary` header, proxy revalidation, and authenticated responses.

use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use http::Request;
use http::Response;
use http::StatusCode;
use http::Uri;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::time::Duration;
use std::time::SystemTime;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

// rfc7231 6.1
const STATUS_CODE_CACHEABLE_BY_DEFAULT: &[u16] =
    &[200, 203, 204, 206, 300, 301, 308, 404, 405, 410, 414, 501];

// This implementation does not understand partial responses (206)
const UNDERSTOOD_STATUSES: &[u16] = &[
    200, 203, 204, 300, 301, 302, 303, 307, 308, 404, 405, 410, 414, 501,
];

const HOP_BY_HOP_HEADERS: &[&str] = &[
    "date", // included, because we add Age update Date
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

const EXCLUDED_FROM_REVALIDATION_UPDATE: &[&str] = &[
    // Since the old body is reused, it doesn't make sense to change properties of the body
    "content-length",
    "content-encoding",
    "transfer-encoding",
    "content-range",
];

type CacheControl = HashMap<Box<str>, Option<Box<str>>>;

fn parse_cache_control<'a>(headers: impl IntoIterator<Item = &'a HeaderValue>) -> CacheControl {
    let mut cc = CacheControl::new();
    let mut is_valid = true;

    for h in headers.into_iter().filter_map(|v| v.to_str().ok()) {
        for part in h.split(',') {
            // TODO: lame parsing
            if part.trim().is_empty() {
                continue;
            }
            let mut kv = part.splitn(2, '=');
            let k = kv.next().unwrap().trim();
            if k.is_empty() {
                continue;
            }
            let v = kv.next().map(str::trim);
            match cc.entry(k.into()) {
                Entry::Occupied(e) => {
                    // When there is more than one value present for a given directive (e.g., two Expires header fields, multiple Cache-Control: max-age directives),
                    // the directive's value is considered invalid. Caches are encouraged to consider responses that have invalid freshness information to be stale
                    if e.get().as_deref() != v {
                        is_valid = false;
                    }
                }
                Entry::Vacant(e) => {
                    e.insert(v.map(|v| v.trim_matches('"')).map(From::from)); // TODO: bad unquoting
                }
            }
        }
    }
    if !is_valid {
        cc.insert("must-revalidate".into(), None);
    }
    cc
}

fn format_cache_control(cc: &CacheControl) -> String {
    let mut out = String::new();
    for (k, v) in cc {
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(k);
        if let Some(v) = v {
            out.push('=');
            let needs_quote =
                v.is_empty() || v.as_bytes().iter().any(|b| !b.is_ascii_alphanumeric());
            if needs_quote {
                out.push('"');
            }
            out.push_str(v);
            if needs_quote {
                out.push('"');
            }
        }
    }
    out
}

/// Configuration options which control behavior of the cache. Use with `CachePolicy::new_options()`.
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CacheOptions {
    /// If `true` (default), then the response is evaluated from a
    /// perspective of a shared cache (i.e. `private` is not cacheable and
    /// `s-maxage` is respected). If `shared` is `false`, then the response is
    /// evaluated from a perspective of a single-user cache (i.e. `private` is
    /// cacheable and `s-maxage` is ignored). `shared: true` is required
    /// for proxies and multi-user caches.
    pub shared: bool,
    /// `cache_heuristic` is a fraction of response's age that is used as a
    /// fallback cache duration. The default is 0.1 (10%), e.g. if a file
    /// hasn't been modified for 100 days, it'll be cached for 100×0.1 = 10
    /// days.
    pub cache_heuristic: f32,
    /// `immutable_min_time_to_live` is a duration to assume as the
    /// default time to cache responses with `Cache-Control: immutable`. Note
    /// that per RFC these can become stale, so `max-age` still overrides the
    /// default.
    pub immutable_min_time_to_live: Duration,
    /// If `ignore_cargo_cult` is `true`, common anti-cache directives will be
    /// completely ignored if the non-standard `pre-check` and `post-check`
    /// directives are present. These two useless directives are most commonly
    /// found in bad StackOverflow answers and PHP's "session limiter"
    /// defaults.
    pub ignore_cargo_cult: bool,
}

impl Default for CacheOptions {
    fn default() -> Self {
        Self {
            shared: true,
            cache_heuristic: 0.1, // 10% matches IE
            immutable_min_time_to_live: Duration::from_secs(24 * 3600),
            ignore_cargo_cult: false,
        }
    }
}

/// Identifies when responses can be reused from a cache, taking into account
/// HTTP RFC 7234 rules for user agents and shared caches. It's aware of many
/// tricky details such as the Vary header, proxy revalidation, and
/// authenticated responses.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CachePolicy {
    #[cfg_attr(feature = "serde", serde(with = "http_serde::header_map"))]
    req: HeaderMap,
    #[cfg_attr(feature = "serde", serde(with = "http_serde::header_map"))]
    res: HeaderMap,
    #[cfg_attr(feature = "serde", serde(with = "http_serde::uri"))]
    uri: Uri,
    #[cfg_attr(feature = "serde", serde(with = "http_serde::status_code"))]
    status: StatusCode,
    #[cfg_attr(feature = "serde", serde(with = "http_serde::method"))]
    method: Method,
    opts: CacheOptions,
    res_cc: CacheControl,
    req_cc: CacheControl,
    response_time: SystemTime,
}

impl CachePolicy {
    /// Cacheability of an HTTP response depends on how it was requested, so
    /// both request and response are required to create the policy.
    #[inline]
    pub fn new<Req: RequestLike, Res: ResponseLike>(req: &Req, res: &Res) -> Self {
        let uri = req.uri();
        let status = res.status();
        let method = req.method().clone();
        let res = res.headers().clone();
        let req = req.headers().clone();
        Self::from_details(
            uri,
            method,
            status,
            req,
            res,
            SystemTime::now(),
            Default::default(),
        )
    }

    /// Caching with customized behavior. See `CacheOptions` for details.
    ///
    /// `response_time` is a timestamp when the response has been received, usually `SystemTime::now()`.
    #[inline]
    pub fn new_options<Req: RequestLike, Res: ResponseLike>(
        req: &Req,
        res: &Res,
        response_time: SystemTime,
        opts: CacheOptions,
    ) -> Self {
        let uri = req.uri();
        let status = res.status();
        let method = req.method().clone();
        let res = res.headers().clone();
        let req = req.headers().clone();
        Self::from_details(uri, method, status, req, res, response_time, opts)
    }

    fn from_details(
        uri: Uri,
        method: Method,
        status: StatusCode,
        req: HeaderMap,
        mut res: HeaderMap,
        response_time: SystemTime,
        opts: CacheOptions,
    ) -> Self {
        let mut res_cc = parse_cache_control(res.get_all("cache-control"));
        let req_cc = parse_cache_control(req.get_all("cache-control"));

        // Assume that if someone uses legacy, non-standard uncecessary options they don't understand caching,
        // so there's no point stricly adhering to the blindly copy&pasted directives.
        if opts.ignore_cargo_cult
            && res_cc.get("pre-check").is_some()
            && res_cc.get("post-check").is_some()
        {
            res_cc.remove("pre-check");
            res_cc.remove("post-check");
            res_cc.remove("no-cache");
            res_cc.remove("no-store");
            res_cc.remove("must-revalidate");
            res.insert(
                "cache-control",
                HeaderValue::from_str(&format_cache_control(&res_cc)).unwrap(),
            );
            res.remove("expires");
            res.remove("pragma");
        }

        // When the Cache-Control header field is not present in a request, caches MUST consider the no-cache request pragma-directive
        // as having the same effect as if "Cache-Control: no-cache" were present (see Section 5.2.1).
        if !res.contains_key("cache-control")
            && res
                .get_str("pragma")
                .map_or(false, |p| p.contains("no-cache"))
        {
            res_cc.insert("no-cache".into(), None);
        }

        Self { req, res, uri, status, method, opts, res_cc, req_cc, response_time }
    }

    /// Returns `true` if the response can be stored in a cache. If it's
    /// `false` then you MUST NOT store either the request or the response.
    pub fn is_storable(&self) -> bool {
        // The "no-store" request directive indicates that a cache MUST NOT store any part of either this request or any response to it.
        !self.req_cc.contains_key("no-store") &&
            // A cache MUST NOT store a response to any request, unless:
            // The request method is understood by the cache and defined as being cacheable, and
            (Method::GET == self.method ||
                Method::HEAD == self.method ||
                (Method::POST == self.method && self.has_explicit_expiration())) &&
            // the response status code is understood by the cache, and
            UNDERSTOOD_STATUSES.contains(&self.status.as_u16()) &&
            // the "no-store" cache directive does not appear in request or response header fields, and
            !self.res_cc.contains_key("no-store") &&
            // the "private" response directive does not appear in the response, if the cache is shared, and
            (!self.opts.shared || !self.res_cc.contains_key("private")) &&
            // the Authorization header field does not appear in the request, if the cache is shared,
            (!self.opts.shared ||
                !self.req.contains_key("authorization") ||
                self.allows_storing_authenticated()) &&
            // the response either:
            // contains an Expires header field, or
            (self.res.contains_key("expires") ||
                // contains a max-age response directive, or
                // contains a s-maxage response directive and the cache is shared, or
                // contains a public response directive.
                self.res_cc.contains_key("max-age") ||
                (self.opts.shared && self.res_cc.contains_key("s-maxage")) ||
                self.res_cc.contains_key("public") ||
                // has a status code that is defined as cacheable by default
                STATUS_CODE_CACHEABLE_BY_DEFAULT.contains(&self.status.as_u16()))
    }

    fn has_explicit_expiration(&self) -> bool {
        // 4.2.1 Calculating Freshness Lifetime
        (self.opts.shared && self.res_cc.contains_key("s-maxage"))
            || self.res_cc.contains_key("max-age")
            || self.res.contains_key("expires")
    }

    /// Returns whether the cached response is still fresh in the context of
    /// the new request.
    ///
    /// If it returns `Fresh`, then the given request matches the original
    /// response this cache policy has been created with, and the response can
    /// be reused without contacting the server.
    ///
    /// If it returns `Stale`, then the response may not be matching at all
    /// (e.g. it's for a different URL or method), or may require to be
    /// refreshed first. Either way, the new request's headers will have been
    /// updated for sending it to the origin server.
    pub fn before_request<Req: RequestLike>(&self, req: &Req, now: SystemTime) -> BeforeRequest {
        let req_headers = req.headers();

        // revalidation allowed via HEAD
        let (matches, may_revalidate) = self.request_matches(req);

        if matches && self.satisfies_without_revalidation(req_headers, now) {
            BeforeRequest::Fresh(self.cached_response(now))
        } else if may_revalidate {
            BeforeRequest::Stale {
                request: self.revalidation_request(req),
                matches,
            }
        } else {
            BeforeRequest::Stale {
                request: self.request_from_headers(req_headers.clone()),
                matches,
            }
        }
    }

    fn satisfies_without_revalidation(&self, req_headers: &HeaderMap, now: SystemTime) -> bool {
        // When presented with a request, a cache MUST NOT reuse a stored response, unless:
        // the presented request does not contain the no-cache pragma (Section 5.4), nor the no-cache cache directive,
        // unless the stored response is successfully validated (Section 4.3), and
        let req_cc = parse_cache_control(req_headers.get_all("cache-control"));
        if req_cc.contains_key("no-cache")
            || req_headers
                .get_str("pragma")
                .map_or(false, |v| v.contains("no-cache"))
        {
            return false;
        }

        if let Some(max_age) = req_cc
            .get("max-age")
            .and_then(|v| v.as_ref())
            .and_then(|p| p.parse().ok())
        {
            if self.age(now) > Duration::from_secs(max_age) {
                return false;
            }
        }

        if let Some(min_fresh) = req_cc
            .get("min-fresh")
            .and_then(|v| v.as_ref())
            .and_then(|p| p.parse().ok())
        {
            if self.time_to_live(now) < Duration::from_secs(min_fresh) {
                return false;
            }
        }

        // the stored response is either:
        // fresh, or allowed to be served stale
        if self.is_stale(now) {
            // If no value is assigned to max-stale, then the client is willing to accept a stale response of any age.
            let max_stale = req_cc.get("max-stale");
            let has_max_stale = max_stale.is_some();
            let max_stale = max_stale
                .and_then(|m| m.as_ref())
                .and_then(|s| s.parse().ok());
            let allows_stale = !self.res_cc.contains_key("must-revalidate")
                && has_max_stale
                && max_stale.map_or(true, |val| {
                    Duration::from_secs(val) > self.age(now) - self.max_age()
                });
            if !allows_stale {
                return false;
            }
        }

        true
    }

    /// returns: matches including method, matches allowing head
    fn request_matches<Req: RequestLike>(&self, req: &Req) -> (bool, bool) {
        // The presented effective request URI and that of the stored response match, and
        let matches = req.is_same_uri(&self.uri) &&
            (self.req.get("host") == req.headers().get("host")) &&
            // selecting header fields nominated by the stored response (if any) match those presented, and
            self.vary_matches(req);
        let exact_match = matches && self.method == req.method();

        // the request method associated with the stored response allows it to be used for the presented request, and
        (exact_match, exact_match || Method::HEAD == req.method())
    }

    fn allows_storing_authenticated(&self) -> bool {
        //  following Cache-Control response directives (Section 5.2.2) have such an effect: must-revalidate, public, and s-maxage.
        self.res_cc.contains_key("must-revalidate")
            || self.res_cc.contains_key("public")
            || self.res_cc.contains_key("s-maxage")
    }

    fn vary_matches<Req: RequestLike>(&self, req: &Req) -> bool {
        for name in get_all_comma(self.res.get_all("vary")) {
            // A Vary header field-value of "*" always fails to match
            if name == "*" {
                return false;
            }
            let name = name.trim().to_ascii_lowercase();
            if req.headers().get(&name) != self.req.get(&name) {
                return false;
            }
        }
        true
    }

    fn copy_without_hop_by_hop_headers(in_headers: &HeaderMap) -> HeaderMap {
        let mut headers = HeaderMap::with_capacity(in_headers.len());

        for (h, v) in in_headers
            .iter()
            .filter(|(h, _)| !HOP_BY_HOP_HEADERS.contains(&h.as_str()))
        {
            headers.insert(h.clone(), v.clone());
        }

        // 9.1.  Connection
        for name in get_all_comma(in_headers.get_all("connection")) {
            headers.remove(name);
        }

        let new_warnings = join(
            get_all_comma(in_headers.get_all("warning")).filter(|warning| {
                !warning.trim_start().starts_with('1') // FIXME: match 100-199, not 1 or 1000
            }),
        );
        if new_warnings.is_empty() {
            headers.remove("warning");
        } else {
            headers.insert("warning", HeaderValue::from_str(&new_warnings).unwrap());
        }
        headers
    }

    /// Updates and filters the response headers for a cached response before
    /// returning it to a client. This function is necessary, because proxies
    /// MUST always remove hop-by-hop headers (such as TE and Connection) and
    /// update response's Age to avoid doubling cache time.
    ///
    /// It returns response "parts" without a body. You can upgrade it to a full
    /// response with `Response::from_parts(parts, BYOB)`
    fn cached_response(&self, now: SystemTime) -> http::response::Parts {
        let mut headers = Self::copy_without_hop_by_hop_headers(&self.res);
        let age = self.age(now);
        let day = Duration::from_secs(3600 * 24);

        // A cache SHOULD generate 113 warning if it heuristically chose a freshness
        // lifetime greater than 24 hours and the response's age is greater than 24 hours.
        if age > day && !self.has_explicit_expiration() && self.max_age() > day {
            headers.append(
                "warning",
                HeaderValue::from_static(r#"113 - "rfc7234 5.5.4""#),
            );
        }
        let date = OffsetDateTime::from(now);
        headers.insert(
            "age",
            HeaderValue::from_str(&age.as_secs().to_string()).unwrap(),
        );
        headers.insert(
            "date",
            HeaderValue::from_str(&date.format(&Rfc2822).unwrap()).unwrap(),
        );

        let mut parts = Response::builder()
            .status(self.status)
            .body(())
            .unwrap()
            .into_parts().0;
        parts.headers = headers;
        parts
    }

    fn raw_server_date(&self) -> SystemTime {
        let date = self
            .res
            .get_str("date")
            .and_then(|d| OffsetDateTime::parse(d, &Rfc2822).ok())
            .and_then(|d| {
                SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(d.unix_timestamp() as u64))
            });
        date.unwrap_or(self.response_time)
    }

    /// Tells how long the response has been sitting in cache(s).
    ///
    /// Value of the `Age` header, updated for the current time.
    pub fn age(&self, now: SystemTime) -> Duration {
        let mut age = self.age_header_value();

        if let Ok(resident_time) = now.duration_since(self.response_time) {
            age += resident_time;
        }
        age
    }

    fn age_header_value(&self) -> Duration {
        Duration::from_secs(
            self.res
                .get_str("age")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        )
    }

    /// Value of applicable max-age (or heuristic equivalent) in seconds.
    ///
    /// This counts since response's `Date` - `Age`.
    ///
    /// For an up-to-date value, see `time_to_live()`.
    fn max_age(&self) -> Duration {
        if !self.is_storable() || self.res_cc.contains_key("no-cache") {
            return Duration::from_secs(0);
        }

        // Shared responses with cookies are cacheable according to the RFC, but IMHO it'd be unwise to do so by default
        // so this implementation requires explicit opt-in via public header
        if self.opts.shared
            && (self.res.contains_key("set-cookie")
                && !self.res_cc.contains_key("public")
                && !self.res_cc.contains_key("immutable"))
        {
            return Duration::from_secs(0);
        }

        if self.res.get_str("vary").map(str::trim) == Some("*") {
            return Duration::from_secs(0);
        }

        if self.opts.shared {
            if self.res_cc.contains_key("proxy-revalidate") {
                return Duration::from_secs(0);
            }
            // if a response includes the s-maxage directive, a shared cache recipient MUST ignore the Expires field.
            if let Some(s_max) = self.res_cc.get("s-maxage").and_then(|v| v.as_ref()) {
                return Duration::from_secs(s_max.parse().unwrap_or(0));
            }
        }

        // If a response includes a Cache-Control field with the max-age directive, a recipient MUST ignore the Expires field.
        if let Some(max_age) = self.res_cc.get("max-age").and_then(|v| v.as_ref()) {
            return Duration::from_secs(max_age.parse().unwrap_or(0));
        }

        let default_min_ttl = if self.res_cc.contains_key("immutable") {
            self.opts.immutable_min_time_to_live
        } else {
            Duration::from_secs(0)
        };

        let server_date = self.raw_server_date();
        if let Some(expires) = self.res.get_str("expires") {
            return match OffsetDateTime::parse(expires, &Rfc2822) {
                // A cache recipient MUST interpret invalid date formats, especially the value "0", as representing a time in the past (i.e., "already expired").
                Err(_) => Duration::from_secs(0),
                Ok(expires) => {
                    let expires = SystemTime::UNIX_EPOCH
                        + Duration::from_secs(expires.unix_timestamp().max(0) as _);
                    return default_min_ttl
                        .max(expires.duration_since(server_date).unwrap_or_default());
                }
            };
        }

        if let Some(last_modified) = self.res.get_str("last-modified") {
            if let Ok(last_modified) = OffsetDateTime::parse(last_modified, &Rfc2822) {
                let last_modified = SystemTime::UNIX_EPOCH
                    + Duration::from_secs(last_modified.unix_timestamp().max(0) as _);
                if let Ok(diff) = server_date.duration_since(last_modified) {
                    let secs_left = diff.as_secs() as f64 * f64::from(self.opts.cache_heuristic);
                    return default_min_ttl.max(Duration::from_secs(secs_left as _));
                }
            }
        }

        default_min_ttl
    }

    /// Returns approximate time until the response becomes
    /// stale (i.e. not fresh). This is the correct way of getting the current `max-age` value.
    ///
    /// After that time (when `time_to_live() == Duration::ZERO`) the response might not be
    /// usable without revalidation. However, there are exceptions, e.g. a
    /// client can explicitly allow stale responses, so always check with
    /// `before_request()`.
    ///
    /// If you're storing responses in a cache/database, keep them approximately for
    /// the `time_to_live` duration plus some extra time to allow for revalidation
    /// (an expired response is still useful).
    pub fn time_to_live(&self, now: SystemTime) -> Duration {
        self.max_age()
            .checked_sub(self.age(now))
            .unwrap_or_default()
    }

    /// Stale responses shouldn't be used without contacting the server (revalidation)
    pub fn is_stale(&self, now: SystemTime) -> bool {
        self.max_age() <= self.age(now)
    }

    /// Headers for sending to the origin server to revalidate stale response.
    /// Allows server to return 304 to allow reuse of the previous response.
    ///
    /// Hop by hop headers are always stripped.
    /// Revalidation headers may be added or removed, depending on request.
    ///
    /// It returns request "parts" without a body. You can upgrade it to a full
    /// response with `Request::from_parts(parts, BYOB)` (the body is usually `()`).
    ///
    /// You don't need this if you use [`before_request()`]
    fn revalidation_request<Req: RequestLike>(&self, incoming_req: &Req) -> http::request::Parts {
        let mut headers = Self::copy_without_hop_by_hop_headers(incoming_req.headers());

        // This implementation does not understand range requests
        headers.remove("if-range");

        if !self.is_storable() {
            // not for the same resource, or wasn't allowed to be cached anyway
            headers.remove("if-none-match");
            headers.remove("if-modified-since");
            return self.request_from_headers(headers);
        }

        /* MUST send that entity-tag in any cache validation request (using If-Match or If-None-Match) if an entity-tag has been provided by the origin server. */
        if let Some(etag) = self.res.get_str("etag") {
            let if_none = join(get_all_comma(headers.get_all("if-none-match")).chain(Some(etag)));
            headers.insert("if-none-match", HeaderValue::from_str(&if_none).unwrap());
        }

        // Clients MAY issue simple (non-subrange) GET requests with either weak validators or strong validators. Clients MUST NOT use weak validators in other forms of request.
        let forbids_weak_validators = self.method != Method::GET
            || headers.contains_key("accept-ranges")
            || headers.contains_key("if-match")
            || headers.contains_key("if-unmodified-since");

        /* SHOULD send the Last-Modified value in non-subrange cache validation requests (using If-Modified-Since) if only a Last-Modified value has been provided by the origin server.
        Note: This implementation does not understand partial responses (206) */
        if forbids_weak_validators {
            headers.remove("if-modified-since");

            let etags = join(
                get_all_comma(headers.get_all("if-none-match"))
                    .filter(|etag| !etag.trim_start().starts_with("W/")),
            );
            if etags.is_empty() {
                headers.remove("if-none-match");
            } else {
                headers.insert("if-none-match", HeaderValue::from_str(&etags).unwrap());
            }
        } else if !headers.contains_key("if-modified-since") {
            if let Some(last_modified) = self.res.get_str("last-modified") {
                headers.insert(
                    "if-modified-since",
                    HeaderValue::from_str(last_modified).unwrap(),
                );
            }
        }
        self.request_from_headers(headers)
    }

    fn request_from_headers(&self, headers: HeaderMap) -> http::request::Parts {
        let mut parts = Request::builder()
            .method(self.method.clone())
            .uri(self.uri.clone())
            .body(())
            .unwrap()
            .into_parts().0;
        parts.headers = headers;
        parts
    }

    /// Creates `CachePolicy` with information combined from the previews response,
    /// and the new revalidation response.
    ///
    /// Returns `{policy, modified}` where modified is a boolean indicating
    /// whether the response body has been modified, and old cached body can't be used.
    pub fn after_response<Req: RequestLike, Res: ResponseLike>(
        &self,
        request: &Req,
        response: &Res,
        response_time: SystemTime,
    ) -> AfterResponse {
        let response_headers = response.headers();
        let mut response_status = response.status();

        let old_etag = &self.res.get_str("etag").map(str::trim);
        let old_last_modified = response_headers.get_str("last-modified").map(str::trim);
        let new_etag = response_headers.get_str("etag").map(str::trim);
        let new_last_modified = response_headers.get_str("last-modified").map(str::trim);

        // These aren't going to be supported exactly, since one CachePolicy object
        // doesn't know about all the other cached objects.
        let mut matches = false;
        if response.status() != StatusCode::NOT_MODIFIED {
            matches = false;
        } else if new_etag.map_or(false, |etag| !etag.starts_with("W/")) {
            // "All of the stored responses with the same strong validator are selected.
            // If none of the stored responses contain the same strong validator,
            // then the cache MUST NOT use the new response to update any stored responses."
            matches = old_etag.map(|e| e.trim_start_matches("W/")) == new_etag;
        } else if let (Some(old), Some(new)) = (old_etag, new_etag) {
            // "If the new response contains a weak validator and that validator corresponds
            // to one of the cache's stored responses,
            // then the most recent of those matching stored responses is selected for update."
            matches = old.trim_start_matches("W/") == new.trim_start_matches("W/");
        } else if old_last_modified.is_some() {
            matches = old_last_modified == new_last_modified;
        } else {
            // If the new response does not include any form of validator (such as in the case where
            // a client generates an If-Modified-Since request from a source other than the Last-Modified
            // response header field), and there is only one stored response, and that stored response also
            // lacks a validator, then that stored response is selected for update.
            if old_etag.is_none()
                && new_etag.is_none()
                && old_last_modified.is_none()
                && new_last_modified.is_none()
            {
                matches = true;
            }
        }

        let new_response_headers = if matches {
            let mut new_response_headers = HeaderMap::with_capacity(self.res.keys_len());
            // use other header fields provided in the 304 (Not Modified) response to replace all instances
            // of the corresponding header fields in the stored response.
            for (header, old_value) in &self.res {
                let header = header.clone();
                if let Some(new_value) = response_headers.get(&header) {
                    if !EXCLUDED_FROM_REVALIDATION_UPDATE.contains(&header.as_str()) {
                        new_response_headers.insert(header, new_value.clone());
                        continue;
                    }
                }
                new_response_headers.insert(header, old_value.clone());
            }
            response_status = self.status;
            new_response_headers
        } else {
            response_headers.clone()
        };

        let new_policy = CachePolicy::from_details(
            request.uri(),
            request.method().clone(),
            response_status,
            request.headers().clone(),
            new_response_headers,
            response_time,
            self.opts,
        );
        let new_response = new_policy.cached_response(response_time);

        if matches && response.status() == StatusCode::NOT_MODIFIED {
            AfterResponse::NotModified(new_policy, new_response)
        } else {
            AfterResponse::Modified(new_policy, new_response)
        }
    }
}

/// New policy and flags to act on `after_response()`
pub enum AfterResponse {
    /// You can use the cached body! Make sure to use these updated headers
    NotModified(CachePolicy, http::response::Parts),
    /// You need to update the body in the cache
    Modified(CachePolicy, http::response::Parts),
}

fn get_all_comma<'a>(
    all: impl IntoIterator<Item = &'a HeaderValue>,
) -> impl Iterator<Item = &'a str> {
    all.into_iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(',').map(str::trim))
}

trait GetHeaderStr {
    fn get_str(&self, k: &str) -> Option<&str>;
}

impl GetHeaderStr for HeaderMap {
    #[inline]
    fn get_str(&self, k: &str) -> Option<&str> {
        self.get(k).and_then(|v| v.to_str().ok())
    }
}

fn join<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for part in parts {
        out.reserve(2 + part.len());
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(part);
    }
    out
}

/// Next action suggested after `before_request()`
pub enum BeforeRequest {
    /// Good news! You can use it with body from the cache. No need to contact the server.
    Fresh(http::response::Parts),
    /// You must send the request to the server first.
    Stale {
        /// Send this request to the server (it has added revalidation headers when appropriate)
        request: http::request::Parts,
        /// If `false`, request was for some other resource that isn't
        /// semantically the same as previously cached request+response
        matches: bool,
    },
}

impl BeforeRequest {
    /// For backwards compatibility only.
    /// Don't forget to use request headers from `BeforeRequest::Fresh`
    pub fn satisfies_without_revalidation(&self) -> bool {
        matches!(self, Self::Fresh(_))
    }
}

/// Allows using either `Request` or `request::Parts`, or your own newtype.
pub trait RequestLike {
    /// Same as `req.uri().clone()`
    fn uri(&self) -> Uri;
    /// Whether the effective request URI matches the other URI
    ///
    /// It can be naive string comparison, nothing fancy
    fn is_same_uri(&self, other: &Uri) -> bool;
    /// Same as `req.method()`
    fn method(&self) -> &Method;
    /// Same as `req.headers()`
    fn headers(&self) -> &HeaderMap;
}

/// Allows using either `Response` or `response::Parts`, or your own newtype.
pub trait ResponseLike {
    /// Same as `res.status()`
    fn status(&self) -> StatusCode;
    /// Same as `res.headers()`
    fn headers(&self) -> &HeaderMap;
}

impl<Body> RequestLike for Request<Body> {
    fn uri(&self) -> Uri {
        self.uri().clone()
    }
    fn is_same_uri(&self, other: &Uri) -> bool {
        self.uri() == other
    }
    fn method(&self) -> &Method {
        self.method()
    }
    fn headers(&self) -> &HeaderMap {
        self.headers()
    }
}

impl RequestLike for http::request::Parts {
    fn uri(&self) -> Uri {
        self.uri.clone()
    }
    fn is_same_uri(&self, other: &Uri) -> bool {
        &self.uri == other
    }
    fn method(&self) -> &Method {
        &self.method
    }
    fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

impl<Body> ResponseLike for Response<Body> {
    fn status(&self) -> StatusCode {
        self.status()
    }
    fn headers(&self) -> &HeaderMap {
        self.headers()
    }
}

impl ResponseLike for http::response::Parts {
    fn status(&self) -> StatusCode {
        self.status
    }
    fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

#[cfg(feature = "reqwest")]
impl RequestLike for reqwest::Request {
    fn uri(&self) -> Uri {
        self.url().as_str().parse().expect("Uri and Url are incompatible!?")
    }
    fn is_same_uri(&self, other: &Uri) -> bool {
        self.url().as_str() == other
    }
    fn method(&self) -> &Method {
        self.method()
    }
    fn headers(&self) -> &HeaderMap {
        self.headers()
    }
}

#[cfg(feature = "reqwest")]
impl ResponseLike for reqwest::Response {
    fn status(&self) -> StatusCode {
        self.status()
    }
    fn headers(&self) -> &HeaderMap {
        self.headers()
    }
}
