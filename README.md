# Can I cache this?

`CachePolicy` tells when responses can be reused from a cache, taking into account [HTTP RFC 7234/9111](http://httpwg.org/specs/rfc9111.html) rules for user agents and shared caches. It's aware of many tricky details such as the `Vary` header, age updates, proxy revalidation, and authenticated responses.

## Usage

Cacheability of an HTTP response depends on how it was requested, so both `request` and `response` are required to create the policy.

It may be surprising, but it's not enough for an HTTP response to be [fresh](#yo-fresh) to satisfy a request. It may need to match request headers specified in `Vary`. Even a matching fresh response may still not be usable if the new request restricted cacheability, etc.

The key method is `before_request(new_request)`, which checks whether the `new_request` is compatible with the original request and whether all caching conditions are met.

### Options

If `options.shared` is `true` (default), then the response is evaluated from a perspective of a shared cache (i.e. `private` is not cacheable and `s-maxage` is respected). If `options.shared` is `false`, then the response is evaluated from a perspective of a single-user cache (i.e. `private` is cacheable and `s-maxage` is ignored). `shared: true` is recommended for HTTP proxies, and `false` for single-user clients.

`options.cache_heuristic` is a fraction of response's age that is used as a fallback cache duration. The default is 0.1 (10%), e.g. if a file hasn't been modified for 100 days, it'll be cached for 100×0.1 = 10 days.

`options.immutable_min_time_to_live` is a duration to assume as the default time to cache responses with `Cache-Control: immutable`. Note that [per RFC](http://httpwg.org/http-extensions/immutable.html) these can become stale, so `max-age` still overrides the default.

If `options.ignore_cargo_cult` is true, common anti-cache directives will be completely ignored if the non-standard `pre-check` and `post-check` directives are present. These two useless directives are most commonly found in bad StackOverflow answers and PHP's "session limiter" defaults.

### `is_storable()`

Returns `true` if the response can be stored in a cache. If it's `false` then you MUST NOT store either the request or the response.

### `before_request(new_request)`

This is the most important method. Use this method to check whether the cached response is still fresh in the context of the new request.

If it returns `Fresh`, then the given `request` matches the original response this cache policy has been created with, and the response can be reused without contacting the server. This will contain an updated, filtered set of response headers to return to clients receiving the cached response. This processing is necessary, because proxies MUST always remove hop-by-hop headers (such as `TE` and `Connection`) and update response's `Age` to avoid doubling cache time.

If it returns `Stale`, then the response may not be matching at all (e.g. it's for a different URL or method), or may require to be refreshed first. The variant will contain HTTP headers for making a revalidation request to the server.

### `time_to_live()`

Returns approximate time until the response becomes stale (i.e. not fresh). This is equivalent of `max-age`, but with appropriate time correction applied.

After that time (when `time_to_live() == Duration::ZERO`) the response might not be usable without revalidation. However, there are exceptions, e.g. a client can explicitly allow stale responses, so always check with `before_request()`.

### Refreshing stale cache (revalidation)

When a cached response has expired, it can be made fresh again by making a request to the origin server. The server may respond with status 304 (Not Modified) without sending the response body again, saving bandwidth.

#### `after_response(revalidation_request, revalidation_response)`

Use this method to update the cache after receiving a new response from the origin server. It returns `Modified`/`NotModified` object with a new `CachePolicy` with HTTP headers updated from `revalidation_response`. You can always replace the old cached `CachePolicy` with the new one.

    -  If `NotModified`, then a valid 304 Not Modified response has been received, and you can reuse the old cached response body.
    -  If `Modified`, you should replace the old cached body with the new response's body.

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
