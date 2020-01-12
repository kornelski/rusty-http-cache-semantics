/*
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
*/
