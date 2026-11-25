//! The bearer on the wire, and the credential and endpoint guards.

use super::*;
use crate::error::ErrorKind;
use crate::message::{Message, Role};

/// Asserts `error` is a setup error for `reason`, naming `variable` when one
/// is given, whose prose is its message.
#[track_caller]
fn assert_setup_error(error: &Error, reason: &str, variable: Option<&str>) {
    assert_eq!(error.kind, ErrorKind::InvalidConfig, "{error:?}");
    assert_eq!(error.reason, Some(reason), "{error:?}");
    assert_eq!(error.name.as_deref(), variable, "{error:?}");
    assert_eq!(error.summary, error.message, "{error:?}");
}

/// Spawns a gateway that records the `Authorization` header of each
/// completion request (as `Some(value)` or `None`) and answers a minimal
/// stop-finished stream, returning its `/v1` base and the capture slot.
async fn spawn_auth_capturing_gateway() -> (
    String,
    std::sync::Arc<std::sync::Mutex<Option<Option<String>>>>,
) {
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::http::HeaderMap;
    use axum::routing::post;

    let captured: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&captured);
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |headers: HeaderMap| {
            let slot = Arc::clone(&slot);
            async move {
                let auth = headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                *slot.lock().expect("capture lock") = Some(auth);
                (
                    [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                    ok_stream(),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/v1"), captured)
}

#[tokio::test]
async fn keyless_client_sends_no_authorization_header() {
    // The gateway's loopback trust admits only a request with NO
    // Authorization header at all - a presented-but-wrong bearer is still
    // 401 - so a keyless client must omit the header, not send an empty one.
    let (base, captured) = spawn_auth_capturing_gateway().await;
    let client = GatewayChat::keyless(GatewayEndpoint::new(&base).expect("valid endpoint"));
    client
        .complete(
            &[Message::text(Role::User, "hi")],
            None,
            &openai_options(),
            |_| {},
        )
        .await
        .expect("the keyless completion succeeds");
    let seen = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("the gateway saw the request");
    assert_eq!(
        seen, None,
        "a keyless client must send no Authorization header, got {seen:?}"
    );
}

#[tokio::test]
async fn keyed_client_still_sends_the_bearer_header() {
    let (base, captured) = spawn_auth_capturing_gateway().await;
    let client = keyed_client(&base);
    client
        .complete(
            &[Message::text(Role::User, "hi")],
            None,
            &openai_options(),
            |_| {},
        )
        .await
        .expect("the keyed completion succeeds");
    let seen = captured
        .lock()
        .expect("capture lock")
        .clone()
        .expect("the gateway saw the request");
    assert_eq!(seen.as_deref(), Some("Bearer tok"));
}

#[test]
fn keyless_client_debug_is_indistinguishable_from_a_keyed_one() {
    // No presence signal leaks through Debug either way.
    let keyless =
        GatewayChat::keyless(GatewayEndpoint::new("http://127.0.0.1:8081/v1").expect("valid"));
    let rendered = format!("{keyless:?}");
    assert!(rendered.contains("<redacted>"), "got: {rendered}");
    assert!(!rendered.contains("None"), "got: {rendered}");
}

#[test]
fn debug_redacts_the_bearer_key_and_never_leaks_it() {
    let client = GatewayChat::new(
        GatewayEndpoint::new("http://127.0.0.1:8081/v1").expect("valid test endpoint"),
        SecretString::new("super-secret-token").expect("non-empty test key"),
    );
    let rendered = format!("{client:?}");
    assert!(
        !rendered.contains("super-secret-token"),
        "the bearer key must never appear in Debug output, got: {rendered}"
    );
    assert!(
        rendered.contains("<redacted>"),
        "the key field must be redacted, got: {rendered}"
    );
    assert!(
        rendered.contains("http://127.0.0.1:8081/v1"),
        "the base URL is not a secret and should still appear, got: {rendered}"
    );
}

#[test]
fn secret_string_never_prints_its_contents() {
    let secret = SecretString::new("super-secret-token").expect("non-empty test key");
    assert_eq!(format!("{secret:?}"), "SecretString(<redacted>)");
    assert_eq!(format!("{secret}"), "<redacted>");
    assert_eq!(secret.expose_secret(), "super-secret-token");
}

#[test]
fn secret_string_construction_rejects_an_empty_credential() {
    // F12: an empty bearer credential is unrepresentable.
    let error = SecretString::new("").expect_err("blank key is rejected");
    assert_eq!(error.kind, ErrorKind::InvalidConfig);
    assert_eq!(error.reason, Some("empty_secret"));
    assert_eq!(error.message, "secret must not be empty");
    assert_eq!(
        error.summary,
        "The secret is empty. Supply a non-empty secret."
    );
    assert!(SecretString::new("tok").is_ok());
}

#[test]
fn gateway_endpoint_rejects_non_http_schemes_and_missing_host() {
    for url in ["ftp://example.com/v1", "not-a-url", "http://", ""] {
        let error = GatewayEndpoint::new(url).expect_err("invalid endpoint must be rejected");
        assert!(
            error.to_string().starts_with("gateway URL"),
            "the message names the rule that failed: {error}"
        );
        assert!(!error.to_string().contains("missing environment variable"));
    }
}

#[test]
fn gateway_endpoint_keeps_the_url_parse_cause_in_its_message() {
    let url_error = GatewayEndpoint::new("not a url").expect_err("malformed URL is rejected");
    assert_eq!(
        url_error.message,
        format!(
            "gateway URL is not a valid URL: {}",
            url::ParseError::RelativeUrlWithoutBase
        )
    );
    assert_setup_error(&url_error, "invalid_config", None);
}

#[test]
fn constructor_errors_do_not_leak_secrets() {
    for url in [
        "not a url hunter2",
        "ftp://alice:hunter2@host/v1",
        "http://alice:hunter2@host/v1",
        "http://host/v1?token=hunter2",
        "http://host/v1#hunter2",
    ] {
        let error = GatewayEndpoint::new(url).expect_err("invalid endpoint must be rejected");
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(
                !rendered.contains("hunter2") && !rendered.contains("alice"),
                "the rejected URL must not be echoed into diagnostics: {rendered}"
            );
        }
    }
}

#[test]
fn gateway_endpoint_rejects_credentials_query_and_fragment() {
    // F12: the strict URL parse rejects embedded credentials and the
    // query/fragment ambiguity a hand-rolled prefix scan let through.
    for url in [
        "http://user:pass@host/v1",
        "http://user@host/v1",
        "http://host/v1?token=leak",
        "http://host/v1#frag",
    ] {
        let error = GatewayEndpoint::new(url).expect_err("invalid endpoint must be rejected");
        assert!(
            error.to_string().starts_with("gateway URL must"),
            "the message names the rule that failed: {error}"
        );
        assert!(!error.to_string().contains("missing environment variable"));
    }
    // A clean http(s) API root is still accepted and normalized.
    assert_eq!(
        GatewayEndpoint::new("http://host:8080/v1/")
            .expect("clean URL")
            .url(),
        "http://host:8080/v1"
    );
}

#[test]
fn gateway_endpoint_trims_trailing_slash_and_keeps_valid_urls() {
    let endpoint = GatewayEndpoint::new("https://gateway.example.com/v1/")
        .expect("a well-formed https URL is accepted");
    assert_eq!(endpoint.url(), "https://gateway.example.com/v1");
}
