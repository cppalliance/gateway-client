//! Catalog transport: fetching and decoding gateway `GET /v1/models`.

use serde::Deserialize;

use crate::config::SecretString;
use crate::error::Error;
use crate::failure::{make_malformed_error, make_transport_failure_error};
use crate::wire::classify::make_classify_http_failure_error;

/// How a model handles thinking tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Thinking {
    /// The backend always runs with thinking off.
    Never,
    /// The backend always emits thinking tokens.
    Always,
    /// The client may turn thinking on or off per request.
    Switchable,
}

/// One entry of `GET /v1/models`.
///
/// The Gateway fills every field. A stock server fills `id`, and the other
/// fields are `None` or empty. The list mixes inference models with the
/// Gateway's speech-to-text models, which have no context window because they
/// answer no completion request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    /// The model's name, as a request names it.
    pub id: String,
    /// A sentence about the model, or empty.
    pub description: String,
    /// The model's context window in tokens, when the server names one.
    pub context: Option<u32>,
    /// How the model handles thinking, when the server says.
    pub thinking: Option<Thinking>,
    /// The model's provider id, when the server names one.
    pub provider: Option<String>,
}

/// Wire shape of one entry from gateway `GET /v1/models`.
///
/// The inference fields are optional at the wire: a stock server sends only
/// `id`, and the Gateway's speech-to-text models have no context window.
#[derive(Debug, Deserialize)]
struct ModelsListEntry {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    context: Option<u32>,
    #[serde(default)]
    thinking: Option<Thinking>,
    /// The model's provider id, when the gateway names one.
    #[serde(default)]
    provider: Option<String>,
}

/// Wire shape of gateway `GET /v1/models`.
#[derive(Debug, Deserialize)]
struct ModelsListResponse {
    data: Vec<ModelsListEntry>,
}

/// The largest gateway error body kept for a catalog-fetch diagnostic, in bytes.
const MAX_CATALOG_ERROR_BODY: usize = 2000;

/// The largest success-path model-catalog body accepted before decoding, in
/// bytes. A gateway that returns more than this is refused rather than buffered
/// unbounded, mirroring the bound the error path already applies. Sized well
/// above any realistic model list (16 MiB) so legitimate catalogs are unaffected.
const MAX_CATALOG_BODY: u64 = 16 * 1024 * 1024;

/// Reads a success-path response body, refusing it once it would exceed `cap`
/// bytes so a decode cannot buffer an unbounded body first.
///
/// The advertised `Content-Length` short-circuits an oversize body, and the
/// streamed chunks are bounded so a gateway that omits or lies about the length
/// still cannot force an unbounded allocation.
async fn read_catalog_body_capped(
    mut response: reqwest::Response,
    cap: u64,
) -> std::result::Result<Vec<u8>, Error> {
    if let Some(len) = response.content_length()
        && len > cap
    {
        return Err(make_malformed_error(format!(
            "model list body of {len} bytes exceeds the {cap}-byte limit"
        )));
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(make_transport_failure_error)?
    {
        if body.len() as u64 + chunk.len() as u64 > cap {
            return Err(make_malformed_error(format!(
                "model list body exceeds the {cap}-byte limit"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Reads at most `limit` bytes of a non-success response body, stopping early so
/// an oversized error body cannot exhaust memory.
///
/// A read failure is returned as the concrete [`reqwest::Error`] (MODEL-010) so
/// the caller can append it and its cause chain to the failure's message,
/// rather than being flattened into display text that severs the cause.
async fn read_error_body_bounded(
    mut response: reqwest::Response,
    limit: usize,
) -> std::result::Result<String, reqwest::Error> {
    let mut buffer: Vec<u8> = Vec::new();
    while buffer.len() < limit {
        match response.chunk().await? {
            Some(chunk) => {
                let take = (limit - buffer.len()).min(chunk.len());
                buffer.extend_from_slice(&chunk[..take]);
                if take < chunk.len() {
                    break;
                }
            }
            None => break,
        }
    }
    if buffer.is_empty() {
        return Ok("(empty body)".to_owned());
    }
    // F5: escape control characters so a hostile catalog error body cannot forge
    // log lines or smuggle terminal control sequences into a diagnostic.
    let lossy = String::from_utf8_lossy(&buffer);
    let mut escaped = String::with_capacity(lossy.len());
    for ch in lossy.chars() {
        if ch.is_control() {
            escaped.extend(ch.escape_default());
        } else {
            escaped.push(ch);
        }
    }
    Ok(escaped)
}

/// Returns the process-wide catalog HTTP client, building it once on first use.
///
/// A single reusable client (MODEL-018) lets catalog fetches share one
/// connection pool and transport configuration rather than each constructing a
/// throwaway client with its own pool. The returned handle is a cheap clone of
/// the shared client (its state is reference-counted internally).
fn catalog_client() -> reqwest::Client {
    static CATALOG_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CATALOG_CLIENT.get_or_init(reqwest::Client::new).clone()
}

/// Sends a bearer-authed GET through the shared client (MODEL-018) and
/// returns the success response, classifying every failure the same way for
/// each gateway endpoint: `Transport` (or `Timeout`) when the send fails, the
/// classified code with a bounded, control-escaped body appended to its
/// message on a non-success status (MODEL-010: no unbounded buffering), and
/// `Transport` (or `Timeout`) when that error body cannot be read, appending
/// the [`reqwest::Error`] to the message under the same timeout marking as a
/// send failure.
async fn get_authed(url: String, token: &str) -> std::result::Result<reqwest::Response, Error> {
    let response = catalog_client()
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(make_transport_failure_error)?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = match read_error_body_bounded(response, MAX_CATALOG_ERROR_BODY).await {
        Ok(body) => body,
        Err(source) => return Err(make_transport_failure_error(source)),
    };
    Err(make_classify_http_failure_error(status.as_u16(), &body))
}

/// Fetches the model list from the server's `/models` endpoint.
///
/// `base_url` is the root of the server's OpenAI-compatible API, for example
/// `http://127.0.0.1:8081/v1`. The request sends `key` as a bearer token.
/// Every entry the server lists comes back, including one with no context
/// window, which a stock server never has.
///
/// # Errors
/// Returns an [`Error`]. Its [`kind`](Error::kind) is:
///
/// - `Transport` or `Timeout` when the HTTP request or a response read fails;
/// - the kind the classifier picks for the response when the server returns a
///   status outside the 2xx range;
/// - `MalformedResponse` when the body is not a valid model list.
pub async fn fetch_models(
    base_url: &str,
    key: &SecretString,
) -> std::result::Result<Vec<Model>, Error> {
    let base = base_url.trim_end_matches('/');
    let response = get_authed(format!("{base}/models"), key.expose_secret()).await?;
    // Bound the success body BEFORE decoding so an oversized (or unbounded)
    // model list cannot exhaust memory, matching the bound the error path applies.
    let body = read_catalog_body_capped(response, MAX_CATALOG_BODY).await?;
    // A body that does not decode as a model list is a malformed response, not a
    // transport failure - matching this function's documented error contract.
    let list: ModelsListResponse = serde_json::from_slice(&body).map_err(|error| {
        // MODEL-009: keep the decode error in the message and out of the
        // summary, while the classification stays `MalformedResponse`.
        make_malformed_error("model list response was not valid JSON").with_cause(&error)
    })?;
    Ok(list
        .data
        .into_iter()
        .map(|entry| Model {
            id: entry.id,
            description: entry.description,
            context: entry.context,
            thinking: entry.thinking,
            provider: entry.provider,
        })
        .collect())
}

#[cfg(test)]
#[path = "models-tests.rs"]
mod tests;
