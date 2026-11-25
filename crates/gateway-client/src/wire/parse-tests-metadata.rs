//! Tests for the lenient call-metadata parse and its degrade policy.

use super::*;

/// Parses the metadata and counts its diagnostics, so the tests can pin
/// both halves of the degrade policy: malformed sections report, and
/// well-formed or absent sections stay silent.
fn with_warn_count(body: &Value) -> (ResponseMetadata, usize) {
    let metadata = response_metadata(body);
    let warnings = metadata.diagnostics.len();
    (metadata, warnings)
}

/// One assistant text choice, shared by the metadata fixture bodies.
fn reply_choice() -> Value {
    serde_json::json!([{
        "index": 0,
        "message": { "role": "assistant", "content": "hi" },
        "finish_reason": "stop"
    }])
}

#[test]
fn frontier_body_parses_usage_detail_fields() {
    let body = serde_json::json!({
        "id": "chatcmpl-frontier",
        "object": "chat.completion",
        "model": "gpt-5.2",
        "choices": reply_choice(),
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 40,
            "total_tokens": 140,
            "prompt_tokens_details": { "cached_tokens": 64, "audio_tokens": 0 },
            "completion_tokens_details": {
                "reasoning_tokens": 25,
                "audio_tokens": 0,
                "accepted_prediction_tokens": 0,
                "rejected_prediction_tokens": 0
            }
        }
    });

    let (metadata, warnings) = with_warn_count(&body);
    assert_eq!(warnings, 0, "a well-formed body must not warn");
    assert_eq!(metadata.model, "gpt-5.2");
    assert_eq!(
        metadata.usage,
        Some(
            Usage::new(100, 40, 140)
                .with_cached_tokens(Some(64))
                .with_reasoning_tokens(Some(25))
        )
    );
}

#[test]
fn absent_metadata_sections_are_none_without_warning() {
    let bare = serde_json::json!({ "model": "m", "choices": reply_choice() });
    let with_nulls = serde_json::json!({
        "model": "m",
        "choices": reply_choice(),
        "usage": null,
        "timings": null,
        "metrics": null
    });

    for body in [bare, with_nulls] {
        let (metadata, warnings) = with_warn_count(&body);
        assert_eq!(warnings, 0, "absence is normal, never a warning: {body}");
        assert_eq!(metadata.model, "m");
        assert_eq!(metadata.usage, None);
    }
}

#[test]
fn malformed_metadata_degrades_to_none_with_a_warning() {
    // The deliberate degrade path: every section malformed at once, each
    // one warning and dropping to None, and the call still succeeds. The
    // `timings` and `metrics` extensions stay in the reassembled response
    // and are not read here, so only the model and the usage can warn.
    let body = serde_json::json!({
        "model": 7,
        "choices": reply_choice(),
        "usage": { "prompt_tokens": "seven" },
        "timings": { "prompt_n": 7 },
        "metrics": ["not", "an", "object"]
    });

    let (metadata, warnings) = with_warn_count(&body);
    assert_eq!(metadata.model, "", "a non-string model records as empty");
    assert_eq!(metadata.usage, None, "non-numeric token counts degrade");
    assert_eq!(warnings, 2, "each malformed section warns exactly once");
}

#[test]
fn missing_model_records_empty_and_warns() {
    let body = serde_json::json!({ "choices": reply_choice() });

    let (metadata, warnings) = with_warn_count(&body);
    assert_eq!(metadata.model, "");
    assert_eq!(warnings, 1, "an OpenAI-shaped body without a model warns");
}
