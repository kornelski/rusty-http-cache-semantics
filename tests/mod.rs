//! Group all of the integration tests into a single module so that they can share utilities and
//! compile in a single binary

use std::time::SystemTime;

use http::{header, request, response, Request, Response};
use http_cache_semantics::{CacheOptions, CachePolicy, ResponseLike};

mod stub;

fn private_opts() -> CacheOptions {
    CacheOptions {
        shared: false,
        ..Default::default()
    }
}

fn request_parts(builder: request::Builder) -> request::Parts {
    builder.body(()).unwrap().into_parts().0
}

fn response_parts(builder: http::response::Builder) -> http::response::Parts {
    builder.body(()).unwrap().into_parts().0
}

#[derive(Default)]
struct Harness {
    // assertion toggles
    no_store: bool,
    stale_and_store: bool,
    assert_age: Option<u64>,
    assert_time_to_live: Option<u64>,
    // configuration
    time: Option<SystemTime>,
    request: Option<request::Parts>,
    options: CacheOptions,
}

impl Harness {
    fn no_store(mut self) -> Self {
        self.no_store = true;
        self
    }

    fn stale_and_store(mut self) -> Self {
        self.stale_and_store = true;
        self
    }

    fn request(mut self, req: impl Into<request::Parts>) -> Self {
        self.request = Some(req.into());
        self
    }

    fn assert_age(mut self, age: u64) -> Self {
        self.assert_time_to_live = Some(age);
        self
    }

    fn assert_time_to_live(mut self, ttl: u64) -> Self {
        self.assert_time_to_live = Some(ttl);
        self
    }

    fn options(mut self, opts: CacheOptions) -> Self {
        self.options = opts;
        self
    }

    fn time(mut self, time: SystemTime) -> Self {
        self.time = Some(time);
        self
    }

    #[track_caller]
    fn test_with_cache_control(self, c_c: &str) -> CachePolicy {
        let resp = Response::builder()
            .header(header::CACHE_CONTROL, c_c)
            .body(())
            .unwrap();
        self.test_with_response(resp)
    }

    #[track_caller]
    fn test_with_response(self, resp: impl ResponseLike) -> CachePolicy {
        let Self {
            no_store,
            stale_and_store,
            assert_age,
            assert_time_to_live,
            time,
            request,
            options,
        } = self;
        let time = time.unwrap_or_else(SystemTime::now);
        let request = request
            .unwrap_or_else(|| Request::builder().body(()).unwrap().into_parts().0);
        let policy = CachePolicy::new_options(&request, &resp, time, options);
        assert_eq!(no_store, !policy.is_storable(), "Policy didn't match expected storability");
        if no_store {
            assert!(policy.is_stale(time), "no-store always means stale");
        } else {
            assert_eq!(
                stale_and_store,
                policy.is_stale(time),
                "Policy didn't match expected freshness",
            );
        }
        if let Some(age) = assert_age {
            assert_eq!(age, policy.age(time).as_secs(), "Policy didn't have expected age");
        }
        if let Some(ttl) = assert_time_to_live {
            assert_eq!(ttl, policy.time_to_live(time).as_secs(), "Policy didn't have expected TTL");
        }
        if no_store || stale_and_store {
            assert_eq!(0, policy.time_to_live(time).as_secs(), "Stale entries should have no TTL");
        }
        policy
    }
}

fn req_cache_control(s: &str) -> request::Parts {
    Request::builder().header(header::CACHE_CONTROL, s).body(()).unwrap().into_parts().0
}

fn resp_cache_control(s: &str) -> response::Parts {
    Response::builder().header(header::CACHE_CONTROL, s).body(()).unwrap().into_parts().0
}
