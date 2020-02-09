use chrono::prelude::*;
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

// rfc7231 6.1
const STATUS_CODE_CACHEABLE_BY_DEFAULT: &[u16] =
    &[200, 203, 204, 206, 300, 301, 404, 405, 410, 414, 501];

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
        for part in h.split(',') { // TODO: lame parsing
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
                },
                Entry::Vacant(e) => {
                    e.insert(v.map(|v| v.trim_matches('"')).map(From::from)); // TODO: bad unquoting
                },
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
            let needs_quote = v.is_empty() || v.as_bytes().iter().any(|b| !b.is_ascii_alphanumeric());
            if needs_quote { out.push('"'); }
            out.push_str(v);
            if needs_quote { out.push('"'); }
        }
    }
    out
}

#[derive(Debug, Copy, Clone)]
pub struct CachePolicyOptions {
    pub shared: bool,
    pub cache_heuristic: f32,
    pub immutable_min_time_to_live: Duration,
    pub ignore_cargo_cult: bool,
    pub trust_server_date: bool,
    pub response_time: SystemTime,
}

impl Default for CachePolicyOptions {
    fn default() -> Self {
        Self {
            shared: true,
            cache_heuristic: 0.1, // 10% matches IE
            immutable_min_time_to_live: Duration::from_secs(24 * 3600),
            ignore_cargo_cult: false,
            trust_server_date: true,
            response_time: SystemTime::now(),
        }
    }
}

#[derive(Debug)]
pub struct CachePolicy {
    req: HeaderMap,
    res: HeaderMap,
    uri: Uri,
    status: StatusCode,
    method: Method,
    opts: CachePolicyOptions,
    res_cc: CacheControl,
    req_cc: CacheControl,
}

impl CachePolicy {
    pub fn new<ReqBody, ResBody>(
        req: &Request<ReqBody>,
        res: &Response<ResBody>,
        opts: CachePolicyOptions,
    ) -> Self {
        let uri = req.uri().clone();
        let status = res.status();
        let method = req.method().clone();
        let mut res = res.headers().clone();
        let req = req.headers().clone();
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

        Self {
            status,
            method,
            res,
            req,
            res_cc,
            req_cc,
            opts,
            uri,
        }
    }

    pub fn storable(&self) -> bool {
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
                self.res_cc.contains_key("public") ||
                self.res_cc.contains_key("max-age") ||
                self.res_cc.contains_key("s-maxage") ||
                // has a status code that is defined as cacheable by default
                STATUS_CODE_CACHEABLE_BY_DEFAULT.contains(&self.status.as_u16()))
    }

    fn has_explicit_expiration(&self) -> bool {
        // 4.2.1 Calculating Freshness Lifetime
        (self.opts.shared && self.res_cc.contains_key("s-maxage"))
            || self.res_cc.contains_key("max-age")
            || self.res.contains_key("expires")
    }

    pub fn satisfies_without_revalidation<Body>(
        &self,
        req: &Request<Body>,
        now: SystemTime,
    ) -> bool {
        // When presented with a request, a cache MUST NOT reuse a stored response, unless:
        // the presented request does not contain the no-cache pragma (Section 5.4), nor the no-cache cache directive,
        // unless the stored response is successfully validated (Section 4.3), and
        let req_cc = parse_cache_control(req.headers().get_all("cache-control"));
        if req_cc.contains_key("no-cache")
            || req
                .headers()
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
        if let Some(max_stale) = req_cc.get("max-stale") {
            if !self.res_cc.contains_key("must-revalidate") && self.stale(now) {
                let max_stale = max_stale.as_ref().and_then(|s| s.parse().ok());
                let allows_stale = max_stale.map_or(true, |val| {
                    Duration::from_secs(val) > self.age(now) - self.max_age()
                });
                if !allows_stale {
                    return false;
                }
            }
        }

        self.request_matches(req, false)
    }

    fn request_matches<Body>(&self, req: &Request<Body>, allow_head_method: bool) -> bool {
        // The presented effective request URI and that of the stored response match, and
        &self.uri == req.uri() &&
            self.req.get("host") == req.headers().get("host") &&
            // the request method associated with the stored response allows it to be used for the presented request, and
            (
                self.method == req.method() ||
                (allow_head_method && Method::HEAD == req.method())) &&
            // selecting header fields nominated by the stored response (if any) match those presented, and
            self.vary_matches(req)
    }

    fn allows_storing_authenticated(&self) -> bool {
        //  following Cache-Control response directives (Section 5.2.2) have such an effect: must-revalidate, public, and s-maxage.
        self.res_cc.contains_key("must-revalidate")
            || self.res_cc.contains_key("public")
            || self.res_cc.contains_key("s-maxage")
    }

    fn vary_matches<Body>(&self, req: &Request<Body>) -> bool {
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

        for (h, v) in in_headers.iter().filter(|(h, _)| !HOP_BY_HOP_HEADERS.contains(&h.as_str()))
        {
            headers.insert(h.to_owned(), v.to_owned());
        }

        // 9.1.  Connection
        for name in get_all_comma(in_headers.get_all("connection")) {
            headers.remove(name);
        }

        let new_warnings = join(
            get_all_comma(in_headers.get_all("warning")).filter(|warning| {
                warning.trim_start().starts_with('1') // FIXME: match 100-199, not 1 or 1000
            }),
        );
        if new_warnings.is_empty() {
            headers.remove("warning");
        } else {
            headers.insert("warning", HeaderValue::from_str(&new_warnings).unwrap());
        }
        headers
    }

    pub fn response_headers(&self, now: SystemTime) -> HeaderMap {
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
        let timestamp = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let date = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp as _, 0), Utc);
        headers.insert(
            "age",
            HeaderValue::from_str(&format!("{}", age.as_secs() as u32)).unwrap(),
        );
        headers.insert("date", HeaderValue::from_str(&date.to_rfc2822()).unwrap());
        headers
    }

    /// Value of the Date response header or current time if Date was demed invalid
    ///
    fn date(&self) -> SystemTime {
        if self.opts.trust_server_date {
            self.server_date()
        } else {
            self.opts.response_time
        }
    }

    fn server_date(&self) -> SystemTime {
        let date = self
            .res
            .get_str("date")
            .and_then(|d| DateTime::parse_from_rfc2822(d).ok())
            .and_then(|d| {
                SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(d.timestamp() as _))
            });
        if let Some(date) = date {
            let max_clock_drift = Duration::from_secs(8 * 3600);
            let clock_drift = if self.opts.response_time > date {
                self.opts.response_time.duration_since(date)
            } else {
                date.duration_since(self.opts.response_time)
            }
            .unwrap();
            if clock_drift < max_clock_drift {
                return date;
            }
        }
        self.opts.response_time
    }

    /// Value of the Age header, in seconds, updated for the current time.
    fn age(&self, now: SystemTime) -> Duration {
        let mut age = self.age_header();
        if let Ok(since_date) = self.opts.response_time.duration_since(self.date()) {
            age = age.max(since_date);
        }

        if let Ok(resident_time) = now.duration_since(self.opts.response_time) {
            age += resident_time;
        }
        age
    }

    fn age_header(&self) -> Duration {
        Duration::from_secs(
            self.res
                .get_str("age")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        )
    }

    /// Value of applicable max-age (or heuristic equivalent) in seconds. This counts since response's `Date`.
    ///
    /// For an up-to-date value, see `time_to_live()`.
    pub fn max_age(&self) -> Duration {
        if !self.storable() || self.res_cc.contains_key("no-cache") {
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

        let server_date = self.server_date();
        if let Some(expires) = self.res.get_str("expires") {
            return match DateTime::parse_from_rfc2822(expires) {
                // A cache recipient MUST interpret invalid date formats, especially the value "0", as representing a time in the past (i.e., "already expired").
                Err(_) => Duration::from_secs(0),
                Ok(expires) => {
                    let expires = SystemTime::UNIX_EPOCH
                        + Duration::from_secs(expires.timestamp().max(0) as _);
                    return default_min_ttl
                        .max(expires.duration_since(server_date).unwrap_or_default());
                }
            };
        }

        if let Some(last_modified) = self.res.get_str("last-modified") {
            if let Ok(last_modified) = DateTime::parse_from_rfc2822(last_modified) {
                let last_modified = SystemTime::UNIX_EPOCH + Duration::from_secs(last_modified.timestamp().max(0) as _);
                if let Ok(diff) = server_date.duration_since(last_modified) {
                    let secs_left = diff.as_secs() as f64 * self.opts.cache_heuristic as f64;
                    return default_min_ttl.max(Duration::from_secs(secs_left as _));
                }
            }
        }

        default_min_ttl
    }

    pub fn time_to_live(&self, now: SystemTime) -> Duration {
        self.max_age().checked_sub(self.age(now)).unwrap_or_default()
    }

    pub fn stale(&self, now: SystemTime) -> bool {
        self.max_age() <= self.age(now)
    }

    /// Headers for sending to the origin server to revalidate stale response.
    /// Allows server to return 304 to allow reuse of the previous response.
    ///
    /// Hop by hop headers are always stripped.
    /// Revalidation headers may be added or removed, depending on request.
    ///
    pub fn revalidation_headers<Body>(&self, incoming_req: &Request<Body>) -> HeaderMap {
        let mut headers = Self::copy_without_hop_by_hop_headers(incoming_req.headers());

        // This implementation does not understand range requests
        headers.remove("if-range");

        if !self.request_matches(incoming_req, true) || !self.storable() {
            // revalidation allowed via HEAD
            // not for the same resource, or wasn't allowed to be cached anyway
            headers.remove("if-none-match");
            headers.remove("if-modified-since");
            return headers;
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
                    HeaderValue::from_str(&last_modified).unwrap(),
                );
            }
        }

        headers
    }

    /// Creates `CachePolicy` with information combined from the previews response,
    /// and the new revalidation response.
    ///
    /// Returns `{policy, modified}` where modified is a boolean indicating
    /// whether the response body has been modified, and old cached body can't be used.
    pub fn revalidated_policy<ReqB, ResB>(
        &self,
        request: Request<ReqB>,
        mut response: Response<ResB>,
    ) -> RevalidatedPolicy {
        let old_etag = self.res.get_str("etag").map(str::trim);
        let old_last_modified = response.headers().get_str("last-modified").map(str::trim);
        let new_etag = response.headers().get_str("new_etag").map(str::trim);
        let new_last_modified = response.headers().get_str("last-modified").map(str::trim);

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

        let modified = response.status() != StatusCode::NOT_MODIFIED;
        if matches {
            // use other header fields provided in the 304 (Not Modified) response to replace all instances
            // of the corresponding header fields in the stored response.
            for (h, v) in &self.res {
                if !EXCLUDED_FROM_REVALIDATION_UPDATE.contains(&&h.as_str()) {
                    response.headers_mut().insert(h.to_owned(), v.to_owned());
                }
            }
            *response.status_mut() = self.status;
        }

        RevalidatedPolicy {
            policy: CachePolicy::new(&request, &response, self.opts),
            // Client receiving 304 without body, even if it's invalid/mismatched has no option
            // but to reuse a cached body. We don't have a good way to tell clients to do
            // error recovery in such case.
            modified,
            matches,
        }
    }
}

pub struct RevalidatedPolicy {
    pub policy: CachePolicy,
    pub modified: bool,
    pub matches: bool,
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
