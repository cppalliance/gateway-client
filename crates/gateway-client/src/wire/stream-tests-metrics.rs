//! Tests for the usage, the client timing, and the two bodies `finish` puts
//! in a reply: the usage section and the client clock reach the reply, the
//! extension sections stay in the response, and the request and reassembled
//! response bodies travel with the reply.

use serde_json::{Value, json};

use super::*;
use crate::reply::Usage;

/// Feeds every payload into a fresh accumulator, as the read loop does.
fn accumulate(payloads: &[Value]) -> StreamAccumulator {
    let mut accumulator = StreamAccumulator::new();
    for payload in payloads {
        accumulator
            .apply(&payload.to_string(), &|_| {})
            .expect("fixture payloads are well-formed");
    }
    accumulator
}

fn text_and_stop() -> [Value; 2] {
    [
        json!({
            "model": "qwen3-30b",
            "choices": [{ "index": 0, "delta": { "content": "Hello!" }, "finish_reason": null }]
        }),
        json!({
            "model": "qwen3-30b",
            "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }]
        }),
    ]
}

#[test]
fn finish_attaches_the_request_and_the_reassembled_response_as_the_raw_exchange() {
    let request = json!({ "model": "m", "messages": [{ "role": "user", "content": "hi" }] });
    let accumulator = accumulate(&text_and_stop());
    let response = accumulate(&text_and_stop()).into_body();
    let completion = accumulator
        .finish(request.clone(), None)
        .expect("a complete turn finishes");
    assert_eq!(completion.request(), &request);
    assert_eq!(completion.response(), &response);
    assert_eq!(
        completion.response().pointer("/choices/0/message/content"),
        Some(&json!("Hello!")),
        "the response is the buffered shape a non-streaming backend returns"
    );
}

fn client_timing() -> ClientTiming {
    ClientTiming::new(41.5).with_ttft_ms(Some(9.5))
}

#[test]
fn finish_merges_the_usage_section_and_the_client_clock_into_the_reply() {
    let [text, stop] = text_and_stop();
    let summary = json!({
        "model": "qwen3-30b",
        "choices": [],
        "usage": {
            "prompt_tokens": 20,
            "completion_tokens": 5,
            "total_tokens": 25,
            "prompt_tokens_details": { "cached_tokens": 16 },
            "completion_tokens_details": { "reasoning_tokens": 2 }
        },
        "timings": {
            "prompt_n": 20, "prompt_ms": 12.5, "prompt_per_second": 1600.0,
            "predicted_n": 5, "predicted_ms": 30.5, "predicted_per_second": 98.5
        },
        "metrics": { "time_to_first_token_ms": 8.5 }
    });
    let reply = accumulate(&[text, stop, summary])
        .finish(json!({ "model": "m" }), Some(client_timing()))
        .expect("a complete turn finishes");
    assert_eq!(
        reply.usage(),
        Some(
            &Usage::new(20, 5, 25)
                .with_cached_tokens(Some(16))
                .with_reasoning_tokens(Some(2))
        )
    );
    assert_eq!(reply.timing(), Some(&client_timing()));
    assert_eq!(
        reply.response()["timings"]["predicted_n"],
        5,
        "the llama.cpp section stays in the response, unread"
    );
    assert_eq!(
        reply.response()["metrics"]["time_to_first_token_ms"],
        8.5,
        "the vLLM section stays in the response, unread"
    );
}

#[test]
fn a_stream_with_no_usage_chunk_yields_no_usage_and_keeps_the_client_timing() {
    let reply = accumulate(&text_and_stop())
        .finish(json!({ "model": "m" }), Some(client_timing()))
        .expect("a complete turn finishes");
    assert_eq!(reply.usage(), None);
    assert_eq!(reply.timing(), Some(&client_timing()));
}

#[test]
fn a_turn_nothing_measured_has_neither_usage_nor_timing() {
    let reply = accumulate(&text_and_stop())
        .finish(json!({ "model": "m" }), None)
        .expect("a complete turn finishes");
    assert_eq!(reply.usage(), None);
    assert_eq!(reply.timing(), None);
}

#[test]
fn a_malformed_usage_section_degrades_with_a_diagnostic_and_the_turn_still_finishes() {
    let [text, stop] = text_and_stop();
    let summary = json!({ "choices": [], "usage": "broken" });
    let reply = accumulate(&[text, stop, summary])
        .finish(Value::Null, None)
        .expect("metadata never fails a usable turn");
    assert_eq!(reply.usage(), None, "a malformed usage degrades");
    assert_eq!(reply.diagnostics().len(), 1);
}
