//! Tests for the model catalog fetch against an axum mock gateway: the body
//! bounds, the failure causes kept in the message, the entry filter, and
//! failure prose that carries no Gateway-supplied text.

use crate::error::ErrorKind;

use super::*;

/// The text the fetch appended to `err`'s message after its prose: the
/// provider detail or the cause, which the prose never holds.
fn appended(err: &Error) -> &str {
    err.message
        .strip_prefix(err.summary.as_str())
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or_else(|| panic!("the message extends the prose: {err:?}"))
}

async fn spawn_models(app: axum::Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn fetch_model_catalog_bounds_and_reports_non_success_body() {
    use axum::Router;
    use axum::routing::get;

    async fn models() -> (axum::http::StatusCode, String) {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "e".repeat(MAX_CATALOG_ERROR_BODY * 4),
        )
    }
    let app = Router::new().route("/models", get(models));
    let addr = spawn_models(app).await;

    let err = fetch_models(
        &format!("http://{addr}"),
        &SecretString::new("tok").expect("a key"),
    )
    .await
    .expect_err("a 500 response must surface as an error");
    assert_eq!(err.kind, ErrorKind::ServerError);
    assert_eq!(
        err.summary,
        "the model backend reported a fault of its own (status 500)"
    );
    let detail = appended(&err);
    assert!(
        detail.len() < MAX_CATALOG_ERROR_BODY + 128,
        "the error-path body must be bounded, got {} bytes",
        detail.len()
    );
}

#[tokio::test]
async fn fetch_model_catalog_bounds_an_oversized_success_body() {
    use axum::Router;
    use axum::routing::get;

    // A 200 response whose body exceeds the success cap must be refused
    // BEFORE decoding, not buffered unbounded. The body is deliberately not
    // valid JSON: the bound must trip first, regardless of contents.
    async fn models() -> (axum::http::StatusCode, String) {
        let oversized = usize::try_from(MAX_CATALOG_BODY).unwrap() + 1;
        (axum::http::StatusCode::OK, "e".repeat(oversized))
    }
    let app = Router::new().route("/models", get(models));
    let addr = spawn_models(app).await;

    let err = fetch_models(
        &format!("http://{addr}"),
        &SecretString::new("tok").expect("a key"),
    )
    .await
    .expect_err("an oversized success body must be refused");
    assert_eq!(err.kind, ErrorKind::MalformedResponse);
    assert!(
        err.to_string().contains("exceeds"),
        "the bound must report the size limit, got {err:?}"
    );
    assert_eq!(
        err.message, err.summary,
        "our own wording is not provider text"
    );
}

#[tokio::test]
async fn fetch_model_catalog_keeps_the_json_decode_error_in_the_message() {
    use axum::Router;
    use axum::routing::get;

    // MODEL-009: a 200 body that is not a valid model list is classified as
    // MalformedResponse, and the underlying `serde_json::Error` is appended
    // to the message and kept out of the prose.
    async fn models() -> (axum::http::StatusCode, String) {
        (axum::http::StatusCode::OK, "{ this is not json".to_owned())
    }
    let app = Router::new().route("/models", get(models));
    let addr = spawn_models(app).await;

    let err = fetch_models(
        &format!("http://{addr}"),
        &SecretString::new("tok").expect("a key"),
    )
    .await
    .expect_err("an undecodable body must surface as an error");
    assert_eq!(err.kind, ErrorKind::MalformedResponse);
    let decode = serde_json::from_slice::<ModelsListResponse>(b"{ this is not json")
        .expect_err("the body is not JSON")
        .to_string();
    assert_eq!(appended(&err), decode);
    assert!(!err.summary.contains(&decode), "got {err:?}");
}

#[tokio::test]
async fn fetch_model_catalog_reads_a_provider_only_when_the_entry_names_one() {
    use axum::Router;
    use axum::routing::get;

    async fn models() -> axum::Json<serde_json::Value> {
        axum::Json(serde_json::json!({
            "object": "list",
            "data": [
                { "id": "grok", "object": "model", "kind": "chat", "description": "d",
                  "context": 4096, "thinking": "never", "provider": "xai" },
                { "id": "local", "object": "model", "kind": "chat", "description": "d",
                  "context": 4096, "thinking": "never" }
            ]
        }))
    }
    let app = Router::new().route("/models", get(models));
    let addr = spawn_models(app).await;

    let catalog = fetch_models(
        &format!("http://{addr}"),
        &SecretString::new("tok").expect("a key"),
    )
    .await
    .expect("an optional provider decodes");
    let provider = |name: &str| {
        catalog
            .iter()
            .find(|model| model.id == name)
            .expect("the model is in the catalog")
            .provider
            .clone()
    };
    assert_eq!(provider("grok").as_deref(), Some("xai"));
    assert_eq!(provider("local"), None);
}

#[tokio::test]
async fn fetch_model_catalog_keeps_a_body_read_failure_in_the_message() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // MODEL-010: a non-success response whose body cannot be fully read
    // (the server promises a large body then drops the connection) must
    // surface as a typed transport failure whose message, not its prose,
    // holds the `reqwest::Error`.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let header = "HTTP/1.1 500 Internal Server Error\r\n\
                 Content-Length: 1000000\r\n\r\n";
            let _ = sock.write_all(header.as_bytes()).await;
            let _ = sock.write_all(b"abc").await;
            // Socket drops here: the promised body never completes.
        }
    });

    let err = fetch_models(
        &format!("http://{addr}"),
        &SecretString::new("tok").expect("a key"),
    )
    .await
    .expect_err("a truncated error body must surface as an error");
    assert_eq!(err.kind, ErrorKind::Transport);
    assert_eq!(err.summary, "the connection to the model backend failed");
    assert!(
        !appended(&err).is_empty(),
        "the message holds the reqwest read error, got {err:?}"
    );
}
