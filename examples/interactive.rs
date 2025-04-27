//! A command-line interactive HTTP cache demo
//!
//! All of the `http_cache_semantics` logic is contained entirely within `fn make_a_request()`

use std::{collections::HashMap, sync::{LazyLock, Mutex}, time::{Duration, SystemTime}};

use dialoguer::{console::style, theme::ColorfulTheme, Input};
use http::{Response, Request, Uri};
use http_cache_semantics::{CacheOptions, CachePolicy};

const START: SystemTime = SystemTime::UNIX_EPOCH;
static CURRENT_TIME: Mutex<SystemTime> = Mutex::new(START);
static THEME: LazyLock<ColorfulTheme> = LazyLock::new(ColorfulTheme::default);

type Req = Request<()>;
type Body = String;
type Resp = Response<Body>;
type Cache = HashMap<Uri, (CachePolicy, Body)>;

fn main() {
    // handle cli args
    let mut args = std::env::args();
    let _exe = args.next().unwrap();
    let has_private_flag = match args.next().as_deref() {
        None => {
            println!(
                "running as a {}. pass {} to run as a private cache",
                bold("shared cache").magenta(),
                style("`-- --private-cache`").dim().italic(),
            );
            false
        }
        Some("-p" | "--private-cache") => {
            println!("running as a {}", bold("private cache").blue());
            true
        }
        _ => {
            eprintln!("usage: cargo run --example=interactive -- [-p|--private-cache]");
            std::process::exit(1);
        }
    };

    let cache_options = CacheOptions {
        shared: !has_private_flag,
        ..Default::default()
    };
    let mut cache = Cache::new();
    let items = ["make a request", "advance time", "list cache entries", "quit"];
    loop {
        println!("{} {}", bold("current time:"), style(current_m_ss()).green());
        let selection = select_prompt()
            .with_prompt("pick an action")
            .items(&items)
            .interact()
            .unwrap();
        match selection {
            0 => make_a_request(&mut cache, cache_options),
            1 => advance_time(),
            2 => list_cache_entries(&cache),
            3 => break,
            _ => unreachable!(),
        }
    }

    println!("\n...and a peek at the cache to finish things off. goodbye!");
    list_cache_entries(&cache);
}

fn make_a_request(cache: &mut Cache, cache_options: CacheOptions) {
    use std::collections::hash_map::Entry;

    use http_cache_semantics::{AfterResponse, BeforeRequest};

    let req = setup_req();
    let resp = match cache.entry(req.uri().to_owned()) {
        Entry::Occupied(mut occupied) => {
            let (policy, body) = occupied.get();
            match policy.before_request(&req, current_time()) {
                BeforeRequest::Fresh(resp) => {
                    println!("{} retrieving cached response", bold("fresh cache entry!").green());
                    Resp::from_parts(resp, body.to_owned())
                },
                BeforeRequest::Stale { request, .. } => {
                    println!("{}", bold("stale entry!").red());
                    let new_req = Req::from_parts(request, ());
                    let mut resp = server::get(new_req.clone());
                    let after_resp = policy.after_response(&new_req, &resp, current_time());
                    let (not_modified, new_policy, new_resp) = match after_resp {
                        AfterResponse::NotModified(p, r) => (true, p, r),
                        AfterResponse::Modified(p, r) => (false, p, r),
                    };
                    // NOTE: if the policy isn't storable then you MUST NOT store the entry
                    if new_policy.is_storable() {
                        if not_modified {
                            println!("{} only updating metadata", bold("not modified!").blue());
                            let entry = occupied.get_mut();
                            entry.0 = new_policy;
                            // and reconstruct the response from our cached bits
                            resp = Resp::from_parts(new_resp, entry.1.clone());
                        } else {
                            println!("{} updating full entry", bold("modified!").magenta());
                            occupied.insert((new_policy, resp.body().to_owned()));
                        }
                    } else {
                        println!(
                            "{} entry was not considered storable",
                            bold("skipping cache!").red(),
                        );
                    }
                    resp
                }
            }
        }
        Entry::Vacant(vacant) => {
            let resp = server::get(req.clone());
            let policy = CachePolicy::new_options(&req, &resp, current_time(), cache_options);
            // NOTE: if the policy isn't storable then you MUST NOT store the entry
            if policy.is_storable() {
                println!("{} inserting entry", bold("cached!").green());
                let body = resp.body().to_owned();
                vacant.insert((policy, body));
            } else {
                println!("{} entry was not considered storable", bold("skipping cache!").red());
            }
            resp
        }
    };

    println!("\n{} {} {}", bold("response from -"), bold("GET").green(), style(req.uri()).green());
    println!("{}", bold("headers -").blue());
    for (name, value) in resp.headers() {
        println!("{}: {}", bold(name.as_str()).blue(), style(value.to_str().unwrap()).italic());
    }
    println!("{} {}\n", bold("body -").blue(), style(resp.body()).blue());
}

fn advance_time() {
    let seconds: u64 = Input::with_theme(&*THEME)
        .with_prompt("seconds to advance")
        .interact()
        .unwrap();
    *CURRENT_TIME.lock().unwrap() += Duration::from_secs(seconds);
    println!("{} {}", bold("advanced to:"), style(current_m_ss()).green());
}

fn list_cache_entries(cache: &Cache) {
    println!();
    for (uri, (policy, body)) in cache {
        let (stale_msg, ttl) = if policy.is_stale(current_time()) {
            (bold("stale").magenta(), style("ttl - expired".to_owned()).italic())
        } else {
            let ttl = format!("ttl - {:>7?}", policy.time_to_live(current_time()));
            (bold("fresh").blue(), bold(ttl))
        };
        let get = bold("GET").green();
        let uri = style(uri.to_string()).green();
        let body = style(body).blue();
        println!("{stale_msg} {ttl} {get} {uri:23} | {body}");
    }
    println!();
}

use helpers::{bold, current_duration, current_time, current_m_ss, select_prompt, setup_req};
mod helpers {
    use std::time::{Duration, SystemTime};

    use super::{CURRENT_TIME, START, THEME, Req};

    use dialoguer::{console::{style, StyledObject}, Select, };

    pub fn select_prompt() -> Select<'static> {
        Select::with_theme(&*THEME)
            .default(0)
    }

    pub fn bold<D>(d: D) -> StyledObject<D> {
        style(d).bold()
    }

    pub fn current_time() -> SystemTime {
        *CURRENT_TIME.lock().unwrap()
    }

    pub fn current_duration() -> Duration {
        current_time().duration_since(START).unwrap()
    }

    pub fn current_m_ss() -> String {
        let elapsed = current_duration();
        let mins = elapsed.as_secs() / 60;
        let secs = elapsed.as_secs() % 60;
        format!("{mins}m{secs:02}s")
    }

    pub fn setup_req() -> Req {
        let path_to_cache_desc = [
            ("/current-time",           "no-store"),
            ("/cached-current-time",    "max-age: 10s"),
            ("/friends-online",         "private, max-age: 30s"),
            ("/user/123/profile-pic",   "e-tag w/ max-age: 30s"),
            ("/cache-busted-123B-8E2A", "immutable"),
        ];
        let styled: Vec<_> = path_to_cache_desc
            .iter()
            .map(|(path, cache_desc)| {
                format!(
                    "{} {:23} {}",
                    bold("GET").green(),
                    style(path).green(),
                    style(format!("server-side - {cache_desc}")).dim().italic(),
                )
            }).collect();
        let selection = select_prompt()
            .with_prompt("make a request")
            .items(&styled)
            .interact()
            .unwrap();
        let path = path_to_cache_desc[selection].0;
        Req::get(path).body(()).unwrap()
    }
}

mod server {
    use super::{CURRENT_TIME, START, Resp, Req, bold, current_duration};

    use dialoguer::console::style;
    use http::{header, Response, HeaderValue};

    pub fn get(req: Req) -> Resp {
        println!("{}ing a response for {}", bold("GET").green(), style(req.uri()).green());
        let elapsed = CURRENT_TIME.lock().unwrap().duration_since(START).unwrap();
        match req.uri().path() {
            "/current-time" => Response::builder()
                .header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))
                .body(format!("current elapsed time {elapsed:?}")),
            "/cached-current-time" => Response::builder()
                .header(header::CACHE_CONTROL, HeaderValue::from_static("max-age=10"))
                .body(format!("cached current elapsed time {elapsed:?}")),
            "/friends-online" => {
                let randomish_num = (current_duration().as_secs() / 10 + 1) * 1997 % 15;
                Response::builder()
                    .header(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=30"))
                    .body(format!("{randomish_num} friends online"))
            }
            "/user/123/profile-pic" => {
                // picture that changes every 5 minutes
                let maybe_client_e_tag = req.headers().get(header::IF_NONE_MATCH);
                let (pic, e_tag) = match current_duration().as_secs() / 300 % 3 {
                    0 => ("(cat looking at stars.jpg)", "1234-abcd"),
                    1 => ("(mountainside.png)", "aaaa-ffff"),
                    2 => ("(beach sunset.jpeg)", "9c31-be74"),
                    _ => unreachable!(),
                };
                if maybe_client_e_tag.is_some_and(|client_e_tag| client_e_tag == e_tag) {
                    // handle ETag revalidation
                    Response::builder()
                        .header(header::ETAG, HeaderValue::from_str(e_tag).unwrap())
                        .status(http::StatusCode::NOT_MODIFIED)
                        .body("".into())
                } else {
                    // no valid ETag. send the full response
                    Response::builder()
                        .header(header::CACHE_CONTROL, "max-age=30")
                        .header(header::ETAG, HeaderValue::from_str(e_tag).unwrap())
                        .body(pic.to_owned())
                }
            }
            "/cache-busted-123B-8E2A" => Response::builder()
                .header(header::CACHE_CONTROL, HeaderValue::from_static("immutable"))
                .body("(pretend like this is some very large asset ~('-')~)".to_owned()),
            _ => unreachable!(),
        }
        .unwrap()
    }
}
