use http::{header, Method, Request, Response};

use crate::private_opts;
use crate::Harness;
use crate::req_cache_control;
use crate::request_parts;
use crate::response_parts;

fn public_cacheable_response() -> http::response::Parts {
    response_parts(Response::builder().header(header::CACHE_CONTROL, "public, max-age=222"))
}

fn cacheable_response() -> http::response::Parts {
    response_parts(Response::builder().header(header::CACHE_CONTROL, "max-age=111"))
}

#[test]
fn no_store_kills_cache() {
    Harness::default()
        .no_store()
        .request(req_cache_control("no-store"))
        .test_with_response(public_cacheable_response());
}

#[test]
fn post_not_cacheable_by_default() {
    Harness::default()
        .no_store()
        .request(request_parts(Request::builder().method(Method::POST)))
        .test_with_cache_control("public");
}

#[test]
fn post_cacheable_explicitly() {
    Harness::default()
        .request(request_parts(Request::builder().method(Method::POST)))
        .test_with_response(public_cacheable_response());
}

#[test]
fn public_cacheable_auth_is_ok() {
    Harness::default()
        .request(
        request_parts(
            Request::builder()
                .header(header::AUTHORIZATION, "test"),
            )
        )
        .test_with_response(public_cacheable_response());
}

#[test]
fn private_auth_is_ok() {
    Harness::default()
        .options(private_opts())
        .request(request_parts(
            Request::builder()
                .header(header::AUTHORIZATION, "test"),
        ))
        .test_with_response(cacheable_response());
}

#[test]
fn revalidate_auth_is_ok() {
    Harness::default()
        .request(request_parts(
            Request::builder()
                .header(header::AUTHORIZATION, "test"),
        ))
        .test_with_cache_control("max-age=80, must-revalidate");
}

#[test]
fn auth_prevents_caching_by_default() {
    Harness::default()
        .no_store()
        .request(request_parts(
            Request::builder()
                .header(header::AUTHORIZATION, "test"),
        ))
        .test_with_response(cacheable_response());
}
