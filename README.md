# Can I cache this?

`CachePolicy` tells when responses can be reused from a cache, taking into account [HTTP RFC 7234](http://httpwg.org/specs/rfc7234.html) rules for user agents and shared caches. It's aware of many tricky details such as the `Vary` header, proxy revalidation, and authenticated responses.

## Usage

Cacheability of an HTTP response depends on how it was requested, so both `request` and `response` are required to create the policy.

It may be surprising, but it's not enough for an HTTP response to be [fresh](#yo-fresh) to satisfy a request. It may need to match request headers specified in `Vary`. Even a matching fresh response may still not be usable if the new request restricted cacheability, etc.

The key method is `satisfies_without_revalidation(new_request)`, which checks whether the `new_request` is compatible with the original request and whether all caching conditions are met.

### Options

If `options.shared` is `true` (default), then the response is evaluated from a perspective of a shared cache (i.e. `private` is not cacheable and `s-maxage` is respected). If `options.shared` is `false`, then the response is evaluated from a perspective of a single-user cache (i.e. `private` is cacheable and `s-maxage` is ignored). `shared: true` is recommended for HTTP clients.

`options.cache_heuristic` is a fraction of response's age that is used as a fallback cache duration. The default is 0.1 (10%), e.g. if a file hasn't been modified for 100 days, it'll be cached for 100\*0.1 = 10 days.

`options.immutable_min_time_to_live` is a duration to assume as the default time to cache responses with `Cache-Control: immutable`. Note that [per RFC](http://httpwg.org/http-extensions/immutable.html) these can become stale, so `max-age` still overrides the default.

If `options.ignore_cargo_cult` is true, common anti-cache directives will be completely ignored if the non-standard `pre-check` and `post-check` directives are present. These two useless directives are most commonly found in bad StackOverflow answers and PHP's "session limiter" defaults.

If `options.trust_server_date` is false, then server's `Date` header won't be used as the base for `max-age`. This is against the RFC, but it's useful if you want to cache responses with very short `max-age`, but your local clock is not exactly in sync with the server's.

### `storable()`

Returns `true` if the response can be stored in a cache. If it's `false` then you MUST NOT store either the request or the response.

### `satisfies_without_revalidation(new_request)`

This is the most important method. Use this method to check whether the cached response is still fresh in the context of the new request.

If it returns `true`, then the given `request` matches the original response this cache policy has been created with, and the response can be reused without contacting the server. Note that the old response can't be returned without being updated, see `response_headers()`.

If it returns `false`, then the response may not be matching at all (e.g. it's for a different URL or method), or may require to be refreshed first (see `revalidation_headers()`).

### `response_headers()`

Returns updated, filtered set of response headers to return to clients receiving the cached response. This function is necessary, because proxies MUST always remove hop-by-hop headers (such as `TE` and `Connection`) and update response's `Age` to avoid doubling cache time.

### `time_to_live()`

Returns approximate time until the response becomes stale (i.e. not fresh).

After that time (when `time_to_live() <= 0`) the response might not be usable without revalidation. However, there are exceptions, e.g. a client can explicitly allow stale responses, so always check with `satisfies_without_revalidation()`.

### Refreshing stale cache (revalidation)

When a cached response has expired, it can be made fresh again by making a request to the origin server. The server may respond with status 304 (Not Modified) without sending the response body again, saving bandwidth.

The following methods help perform the update efficiently and correctly.

#### `revalidation_headers(new_request)`

Returns updated, filtered set of request headers to send to the origin server to check if the cached response can be reused. These headers allow the origin server to return status 304 indicating the response is still fresh. All headers unrelated to caching are passed through as-is.

Use this method when updating cache from the origin server.

#### `revalidated_policy(revalidation_request, revalidation_response)`

Use this method to update the cache after receiving a new response from the origin server. It returns an object with:

-   `policy` — A new `CachePolicy` with HTTP headers updated from `revalidation_response`. You can always replace the old cached `CachePolicy` with the new one.
-   `modified` — Boolean indicating whether the response body has changed.
    -   If `false`, then a valid 304 Not Modified response has been received, and you can reuse the old cached response body.
    -   If `true`, you should use new response's body (if present), or make another request to the origin server without any conditional headers (i.e. don't use `revalidation_headers()` this time) to get the new resource.


# Yo, FRESH

![satisfies_without_revalidation](fresh.jpg)

## Implemented

-   `Cache-Control` response header with all the quirks.
-   `Expires` with check for bad clocks.
-   `Pragma` response header.
-   `Age` response header.
-   `Vary` response header.
-   Default cacheability of statuses and methods.
-   Requests for stale data.
-   Filtering of hop-by-hop headers.
-   Basic revalidation request

## Unimplemented

-   Merging of range requests, If-Range (but correctly supports them as non-cacheable)
-   Revalidation of multiple representations
