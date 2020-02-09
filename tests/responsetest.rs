use chrono::prelude::*;
use http::*;
use http_cache_semantics::*;
use std::time::Duration;
use std::time::SystemTime;

macro_rules! headers(
    { $($key:tt : $value:expr),* $(,)? } => {
        {
            let mut m = Response::builder();
            $(
                m = m.header($key, $value);
            )+
            m.body(()).unwrap()
        }
     };
);

fn req() -> Request<()> {
    Request::get("http://test.example.com/").body(()).unwrap()
}

#[test]
fn simple_miss() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(&req(), &Response::new(()), Default::default());
    assert!(cache.stale(now));
}

#[test]
fn simple_hit() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "public, max-age=999999" },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert_eq!(cache.max_age().as_secs(), 999999);
}

#[test]
fn weird_syntax() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": ",,,,max-age =  456      ," },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert_eq!(cache.max_age().as_secs(), 456);

    // let cache2 = CachePolicy.fromObject(
    //     JSON.parse(JSON.stringify(cache.toObject()))
    // );
    // assert!(cache2 instanceof CachePolicy);
    // assert!(!cache2.stale(now));
    // assert_eq!(cache2.max_age().as_secs(), 456);
}

#[test]
fn quoted_syntax() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "  max-age = \"678\"      " },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert_eq!(cache.max_age().as_secs(), 678);
}

#[test]
fn iis() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "private, public, max-age=259200"
        },
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(!cache.stale(now));
    assert_eq!(cache.max_age().as_secs(), 259200);
}

#[test]
fn pre_check_tolerated() {
    let now = SystemTime::now();
    let cc = "pre-check=0, post-check=0, no-store, no-cache, max-age=100";
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": cc
        },
        Default::default(),
    );
    assert!(cache.stale(now), "{:#?}", cache);
    assert!(!cache.storable());
    assert_eq!(cache.max_age().as_secs(), 0);
    assert_eq!(cache.response_headers(now)["cache-control"], cc);
}

#[test]
fn pre_check_poison() {
    let now = SystemTime::now();
    let orig_cc = "pre-check=0, post-check=0, no-cache, no-store, max-age=100, custom, foo=bar";
    let res = &headers! { "cache-control": orig_cc, "pragma": "no-cache"};
    let cache = CachePolicy::new(
        &req(),
        res,
        CachePolicyOptions {
            ignore_cargo_cult: true,
            ..Default::default()
        },
    );
    assert!(!cache.stale(now));
    assert!(cache.storable());
    assert_eq!(cache.max_age().as_secs(), 100);

    let cc = cache.response_headers(now);
    let cc = cc["cache-control"].to_str().unwrap();
    assert!(!cc.contains("pre-check"));
    assert!(!cc.contains("post-check"));
    assert!(!cc.contains("no-store"));

    assert!(cc.contains("max-age=100"));
    assert!(cc.contains(", custom") || cc.contains("custom, "));
    assert!(cc.contains("foo=bar"));

    assert!(cache.response_headers(now).get("pragma").is_none());
}

#[test]
fn pre_check_poison_undefined_header() {
    let now = SystemTime::now();
    let orig_cc = "pre-check=0, post-check=0, no-cache, no-store";
    let res = &headers! { "cache-control": orig_cc, "expires": "yesterday!"};
    let cache = CachePolicy::new(
        &req(),
        res,
        CachePolicyOptions {
            ignore_cargo_cult: true,
            ..Default::default()
        },
    );
    assert!(cache.stale(now));
    assert!(cache.storable());
    assert_eq!(cache.max_age().as_secs(), 0);

    let _cc = &cache.response_headers(now)["cache-control"];

    assert!(cache.response_headers(now).get("expires").is_none());
}

#[test]
fn cache_with_expires() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now),
            "expires": date_str(now + Duration::from_secs(2)),
        },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert_eq!(2, cache.max_age().as_secs());
}

#[test]
fn cache_with_expires_relative_to_date() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now - Duration::from_secs(3)),
            "expires": date_str(now),
        },
        Default::default(),
    );
    assert_eq!(3, cache.max_age().as_secs());
}

#[test]
fn cache_with_expires_always_relative_to_date() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now - Duration::from_secs(3)),
            "expires": date_str(now),
        },
        CachePolicyOptions {
            trust_server_date: false,
            ..Default::default()
        },
    );
    assert_eq!(3, cache.max_age().as_secs());
}

#[test]
fn cache_expires_no_date() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public",
            "expires": date_str(now + Duration::from_secs(3600)),
        },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert!(cache.max_age().as_secs() > 3595);
    assert!(cache.max_age().as_secs() < 3605);
}

#[test]
fn ages() {
    let mut now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "max-age=100",
            "age": "50",
        },
        Default::default(),
    );
    assert!(cache.storable());

    assert_eq!(50, cache.time_to_live(now).as_secs());
    assert!(!cache.stale(now));
    now += Duration::from_secs(48);
    assert_eq!(2, cache.time_to_live(now).as_secs());
    assert!(!cache.stale(now));
    now += Duration::from_secs(5);
    assert!(cache.stale(now));
    assert_eq!(0, cache.time_to_live(now).as_secs());
}

#[test]
fn age_can_make_stale() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "max-age=100",
            "age": "101",
        },
        Default::default(),
    );
    assert!(cache.stale(now));
    assert!(cache.storable());
}

#[test]
fn age_not_always_stale() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "max-age=20",
            "age": "15",
        },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert!(cache.storable());
}

#[test]
fn bogus_age_ignored() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "max-age=20",
            "age": "golden",
        },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert!(cache.storable());
}

#[test]
fn cache_old_files() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now),
            "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
        },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert!(cache.max_age().as_secs() > 100);
}

#[test]
fn immutable_simple_hit() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "immutable, max-age=999999" },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert_eq!(cache.max_age().as_secs(), 999999);
}

#[test]
fn immutable_can_expire() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "immutable, max-age=0"
        },
        Default::default(),
    );
    assert!(cache.stale(now));
    assert_eq!(cache.max_age().as_secs(), 0);
}

#[test]
fn cache_immutable_files() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now),
            "cache-control": "immutable",
            "last-modified": date_str(now),
        },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert!(cache.max_age().as_secs() > 100);
}

#[test]
fn immutable_can_be_off() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "date": date_str(now),
            "cache-control": "immutable",
            "last-modified": date_str(now),
        },
        CachePolicyOptions {
            immutable_min_time_to_live: Duration::from_secs(0),
            ..Default::default()
        },
    );
    assert!(cache.stale(now));
    assert_eq!(cache.max_age().as_secs(), 0);
}

#[test]
fn pragma_no_cache() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "pragma": "no-cache",
            "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT",
        },
        Default::default(),
    );
    assert!(cache.stale(now));
}

#[test]
fn blank_cache_control_and_pragma_no_cache() {
    let opts = CachePolicyOptions::default();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "",
            "pragma": "no-cache",
            "last-modified": date_str(opts.response_time - Duration::from_secs(10)),
        },
        opts,
    );
    assert!(!cache.stale(opts.response_time), "{:#?}", cache);
}

#[test]
fn no_store() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "no-store, public, max-age=1", },
        Default::default(),
    );
    assert!(cache.stale(now));
    assert_eq!(0, cache.max_age().as_secs());
}

#[test]
fn observe_private_cache() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "private, max-age=1234",
        },
        Default::default(),
    );
    assert!(proxy_cache.stale(now));
    assert_eq!(0, proxy_cache.max_age().as_secs());

    let ua_cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "private, max-age=1234",
        },
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(!ua_cache.stale(now));
    assert_eq!(1234, ua_cache.max_age().as_secs());
}

#[test]
fn don_t_share_cookies() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new(
        &req(),
        &headers! {
            "set-cookie": "foo=bar",
            "cache-control": "max-age=99",
        },
        CachePolicyOptions {
            shared: true,
            ..Default::default()
        },
    );
    assert!(proxy_cache.stale(now));
    assert_eq!(0, proxy_cache.max_age().as_secs());

    let ua_cache = CachePolicy::new(
        &req(),
        &headers! {
            "set-cookie": "foo=bar",
            "cache-control": "max-age=99",
        },
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(!ua_cache.stale(now));
    assert_eq!(99, ua_cache.max_age().as_secs());
}

#[test]
fn do_share_cookies_if_immutable() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new(
        &req(),
        &headers! {
            "set-cookie": "foo=bar",
            "cache-control": "immutable, max-age=99",
        },
        CachePolicyOptions {
            shared: true,
            ..Default::default()
        },
    );
    assert!(!proxy_cache.stale(now));
    assert_eq!(99, proxy_cache.max_age().as_secs());
}

#[test]
fn cache_explicitly_public_cookie() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new(
        &req(),
        &headers! {
            "set-cookie": "foo=bar",
            "cache-control": "max-age=5, public",
        },
        CachePolicyOptions {
            shared: true,
            ..Default::default()
        },
    );
    assert!(!proxy_cache.stale(now));
    assert_eq!(5, proxy_cache.max_age().as_secs());
}

#[test]
fn miss_max_age_0() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! { "cache-control": "public, max-age=0",       },
        Default::default(),
    );
    assert!(cache.stale(now));
    assert_eq!(0, cache.max_age().as_secs());
}

#[test]
fn uncacheable_503() {
    let now = SystemTime::now();
    let mut res = headers! {               "cache-control": "public, max-age=1000" };
    *res.status_mut() = StatusCode::from_u16(503).unwrap();
    let cache = CachePolicy::new(&req(), &res, Default::default());
    assert!(cache.stale(now));
    assert_eq!(0, cache.max_age().as_secs());
}

#[test]
fn cacheable_301() {
    let now = SystemTime::now();
    let mut res = headers! { "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT", };
    *res.status_mut() = StatusCode::from_u16(301).unwrap();
    let cache = CachePolicy::new(&req(), &res, Default::default());
    assert!(!cache.stale(now));
}

#[test]
fn uncacheable_303() {
    let now = SystemTime::now();
    let mut res = headers! { "last-modified": "Mon, 07 Mar 2016 11:52:56 GMT", };
    *res.status_mut() = StatusCode::from_u16(303).unwrap();
    let cache = CachePolicy::new(&req(), &res, Default::default());
    assert!(cache.stale(now));
    assert_eq!(0, cache.max_age().as_secs());
}

#[test]
fn cacheable_303() {
    let now = SystemTime::now();
    let mut res = headers! { "cache-control": "max-age=1000", };
    *res.status_mut() = StatusCode::from_u16(303).unwrap();
    let cache = CachePolicy::new(&req(), &res, Default::default());
    assert!(!cache.stale(now));
}

#[test]
fn uncacheable_412() {
    let now = SystemTime::now();
    let mut res = headers! { "cache-control": "public, max-age=1000", };
    *res.status_mut() = StatusCode::from_u16(412).unwrap();
    let cache = CachePolicy::new(&req(), &res, Default::default());
    assert!(cache.stale(now));
    assert_eq!(0, cache.max_age().as_secs());
}

#[test]
fn expired_expires_cached_with_max_age() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public, max-age=9999",
            "expires": "Sat, 07 May 2016 15:35:18 GMT",
        },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert_eq!(9999, cache.max_age().as_secs());
}

#[test]
fn expired_expires_cached_with_s_maxage() {
    let now = SystemTime::now();
    let proxy_cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public, s-maxage=9999",
            "expires": "Sat, 07 May 2016 15:35:18 GMT",
        },
        Default::default(),
    );
    assert!(!proxy_cache.stale(now));
    assert_eq!(9999, proxy_cache.max_age().as_secs());

    let ua_cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public, s-maxage=9999",
            "expires": "Sat, 07 May 2016 15:35:18 GMT",
        },
        CachePolicyOptions {
            shared: false,
            ..Default::default()
        },
    );
    assert!(ua_cache.stale(now));
    assert_eq!(0, ua_cache.max_age().as_secs());
}

#[test]
fn max_age_wins_over_future_expires() {
    let now = SystemTime::now();
    let cache = CachePolicy::new(
        &req(),
        &headers! {
            "cache-control": "public, max-age=333",
            "expires": date_str(now + Duration::from_secs(3600)),
        },
        Default::default(),
    );
    assert!(!cache.stale(now));
    assert_eq!(333, cache.max_age().as_secs());
}

#[test]
fn remove_hop_headers() {
    let mut now = SystemTime::now();
    let res = &headers! {
        "te": "deflate",
        "date": "now",
        "custom": "header",
        "oompa": "lumpa",
        "connection": "close, oompa, header",
        "age": "10",
        "cache-control": "public, max-age=333",
    };
    let cache = CachePolicy::new(&req(), res, Default::default());

    now += Duration::from_millis(1005);
    let h = cache.response_headers(now);
    assert!(h.get("connection").is_none());
    assert!(h.get("te").is_none());
    assert!(h.get("oompa").is_none());
    assert_eq!(h["cache-control"].to_str().unwrap(), "public, max-age=333");
    assert_ne!(
        h["date"].to_str().unwrap(),
        "now",
        "updated age requires updated date"
    );
    assert_eq!(h["custom"].to_str().unwrap(), "header");
    assert_eq!(h["age"].to_str().unwrap(), "11");

    // let cache2 = TimeTravellingPolicy.fromObject(
    //     JSON.parse(JSON.stringify(cache.toObject()))
    // );
    // assert!(cache2 instanceof TimeTravellingPolicy);
    // let h2 = cache2.response_headers(now);
    // assert.deepEqual(h, h2);
}

fn date_str(now: SystemTime) -> String {
    let timestamp = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let date = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp as _, 0), Utc);
    date.to_rfc2822()
}
