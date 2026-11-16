use super::request_has_api_key;
use axum::http::HeaderMap;

#[tokio::test]
async fn bind_listener_accepts_hostname_and_ipv4_loopback() {
    for host in ["localhost", "127.0.0.1"] {
        let listener = super::server::bind_listener(host, 0).await.unwrap();
        assert!(listener.local_addr().unwrap().ip().is_loopback());
    }
}

#[tokio::test]
async fn bind_listener_accepts_unbracketed_ipv6_loopback() {
    let listener = super::server::bind_listener("::1", 0).await.unwrap();
    assert!(listener.local_addr().unwrap().ip().is_loopback());
}

#[test]
fn blank_api_key_is_refused_unless_unauthenticated_access_is_explicit() {
    for blank in ["", " ", "\t\n"] {
        for is_loopback in [true, false] {
            let error =
                super::build_api_auth(Some(blank.to_string()), false, is_loopback, "127.0.0.1")
                    .err()
                    .expect("a blank key must not start an unauthenticated server")
                    .to_string();
            assert!(error.contains("set but empty"), "{error}");
        }
        let auth = super::build_api_auth(Some(blank.to_string()), true, true, "127.0.0.1").unwrap();
        assert!(auth.is_none());
    }
}

#[test]
fn missing_api_key_keeps_the_loopback_and_public_bind_rules() {
    assert!(
        super::build_api_auth(None, false, true, "127.0.0.1")
            .unwrap()
            .is_none()
    );
    assert!(super::build_api_auth(None, false, false, "0.0.0.0").is_err());
    assert!(
        super::build_api_auth(Some(" key ".to_string()), false, false, "0.0.0.0")
            .unwrap()
            .is_some()
    );
}

#[test]
fn request_has_api_key_accepts_valid_and_rejects_invalid() {
    let expected = "s3cr3t-key";

    let mut bearer = HeaderMap::new();
    bearer.insert(
        axum::http::header::AUTHORIZATION,
        "Bearer s3cr3t-key".parse().unwrap(),
    );
    assert!(request_has_api_key(&bearer, expected));

    let mut api_key = HeaderMap::new();
    api_key.insert("x-api-key", "s3cr3t-key".parse().unwrap());
    assert!(request_has_api_key(&api_key, expected));

    let mut wrong = HeaderMap::new();
    wrong.insert("x-api-key", "wrong-key!!".parse().unwrap());
    assert!(!request_has_api_key(&wrong, expected));

    assert!(!request_has_api_key(&HeaderMap::new(), expected));
}
