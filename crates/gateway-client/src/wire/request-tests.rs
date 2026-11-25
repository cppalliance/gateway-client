//! Tests for the request body: the OpenAI function-call shape of a replayed
//! tool turn, message order, tool wrapping, and the option fields.
//!
//! A replayed tool turn is a message list that a caller built from an earlier
//! round: a user message, an assistant message that made one tool call, and
//! the tool's result. [`tool_round`] writes it as JSON values, which the
//! request builder accepts as any other message type. The adapter in the
//! Runvil Harness has the same tests against messages that a real Engine run
//! produced.

use std::num::NonZeroU32;

use serde_json::{Map, json};

use super::*;
use crate::error::ErrorKind;
use crate::message::{Message, Role};

/// What a second round sends, after one tool round.
struct Round {
    messages: Vec<Value>,
    tools: Vec<Tool>,
    options: Options,
}

/// One tool a round offers: its name, description, and parameters schema.
type Offered = (&'static str, &'static str, Value);

/// The tool object of a schema written as a JSON object.
fn parameters(schema: &Value) -> Map<String, Value> {
    schema
        .as_object()
        .expect("a tool schema is a JSON object")
        .clone()
}

/// Builds the second round of one tool round: the messages that replay the
/// first, the offered tools as the model sees them (`example_test_<name>`),
/// and the options.
///
/// The first round asked the first tool for `call_1` with `{"value":"one"}`,
/// the tool answered `echoed: one`, and the model is asked again.
fn tool_round(offered: &[Offered]) -> Round {
    let first_wire_name = format!("example_test_{}", offered[0].0);
    let messages = vec![
        json!({ "role": "user", "content": "echo once" }),
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": { "name": first_wire_name, "arguments": "{\"value\":\"one\"}" },
            }],
        }),
        json!({ "role": "tool", "content": "echoed: one", "tool_call_id": "call_1" }),
    ];
    let tools = offered
        .iter()
        .map(|(name, description, schema)| {
            Tool::new(
                format!("example_test_{name}"),
                *description,
                parameters(schema),
            )
        })
        .collect();
    Round {
        messages,
        tools,
        options: Options::new("m"),
    }
}

/// The `echo` tool with a one-string-field schema.
fn echo() -> Offered {
    (
        "echo",
        "Echo a value.",
        json!({
            "type": "object",
            "properties": { "value": { "type": "string" } },
            "required": ["value"],
        }),
    )
}

/// Checks every `messages[].tool_calls[]` entry against the OpenAI
/// function-call schema a strict endpoint enforces, returning the
/// offending path for the first violation.
fn check_openai_tool_calls(body: &Value) -> Result<(), String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "request body had no messages array".to_owned())?;
    for (message_index, message) in messages.iter().enumerate() {
        let Some(calls) = message.get("tool_calls").filter(|calls| !calls.is_null()) else {
            continue;
        };
        let calls = calls.as_array().ok_or_else(|| {
            format!("messages[{message_index}].tool_calls was present but not an array")
        })?;
        for (call_index, call) in calls.iter().enumerate() {
            let path = format!("messages[{message_index}].tool_calls[{call_index}]");
            if !call.is_object() {
                return Err(format!("{path} was not an object"));
            }
            if call.get("type") != Some(&Value::String("function".to_owned())) {
                return Err(format!("{path}.type must be the string \"function\""));
            }
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{path} had no string id"))?;
            if id.trim().is_empty() {
                return Err(format!("{path}.id was blank"));
            }
            let function = call
                .get("function")
                .filter(|function| function.is_object())
                .ok_or_else(|| format!("{path}.function was not an object"))?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{path}.function had no string name"))?;
            if name.trim().is_empty() {
                return Err(format!("{path}.function.name was blank"));
            }
            let raw = function
                .get("arguments")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("{path}.function.arguments was not a JSON-encoded string")
                })?;
            let decoded = serde_json::from_str::<Value>(raw).map_err(|error| {
                format!("{path}.function.arguments was not valid JSON: {error}")
            })?;
            if !decoded.is_object() {
                return Err(format!(
                    "{path}.function.arguments did not decode to a JSON object"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn a_replayed_tool_call_turn_serializes_in_the_openai_function_shape() {
    let round = tool_round(&[echo()]);
    let body = build_request_body(&round.messages, None, &round.options).expect("the body builds");
    assert_eq!(check_openai_tool_calls(&body), Ok(()), "{body}");
    let call = &body["messages"][1]["tool_calls"][0];
    assert_eq!(call["id"], "call_1");
    assert_eq!(call["type"], "function");
    assert_eq!(call["function"]["name"], "example_test_echo");
    assert_eq!(call["function"]["arguments"], "{\"value\":\"one\"}");
    assert_eq!(body["messages"][2]["role"], "tool");
    assert_eq!(body["messages"][2]["tool_call_id"], "call_1");
    assert_eq!(body["messages"][2]["content"], "echoed: one");
}

#[test]
fn the_openai_check_names_each_way_a_replayed_call_breaks_the_schema() {
    let round = tool_round(&[echo()]);
    let replayed =
        build_request_body(&round.messages, None, &round.options).expect("the body builds");
    let cases = [
        (
            json!({ "id": "call_1", "function": { "name": "echo", "arguments": "{}" } }),
            ".type must be the string \"function\"",
        ),
        (
            json!({ "id": " ", "type": "function", "function": { "name": "echo", "arguments": "{}" } }),
            ".id was blank",
        ),
        (
            json!({ "type": "function", "function": { "name": "echo", "arguments": "{}" } }),
            " had no string id",
        ),
        (
            json!({ "id": "call_1", "type": "function", "name": "echo", "arguments": "{}" }),
            ".function was not an object",
        ),
        (
            json!({ "id": "call_1", "type": "function", "function": { "name": "", "arguments": "{}" } }),
            ".function.name was blank",
        ),
        (
            json!({ "id": "call_1", "type": "function", "function": { "name": "echo", "arguments": { "value": "one" } } }),
            ".function.arguments was not a JSON-encoded string",
        ),
        (
            json!({ "id": "call_1", "type": "function", "function": { "name": "echo", "arguments": "{oops" } }),
            ".function.arguments was not valid JSON",
        ),
        (
            json!({ "id": "call_1", "type": "function", "function": { "name": "echo", "arguments": "[1]" } }),
            ".function.arguments did not decode to a JSON object",
        ),
    ];
    for (call, expected) in cases {
        let mut body = replayed.clone();
        body["messages"][1]["tool_calls"][0] = call;
        let violation = check_openai_tool_calls(&body).expect_err("the call breaks the schema");
        assert!(
            violation.starts_with("messages[1].tool_calls[0]") && violation.contains(expected),
            "expected {expected:?}, got {violation:?}"
        );
    }
}

#[test]
fn messages_go_out_verbatim_in_order() {
    let messages = [
        Message::text(Role::User, "draft text"),
        Message::text(Role::Assistant, "done"),
        Message::tool("call_1", "echoed: one"),
    ];
    let body = build_request_body(&messages, None, &Options::new("m")).expect("the body builds");
    assert_eq!(
        body["messages"],
        json!([
            { "role": "user", "content": "draft text" },
            { "role": "assistant", "content": "done" },
            { "role": "tool", "content": "echoed: one", "tool_call_id": "call_1" },
        ])
    );
}

#[test]
fn each_tool_is_wrapped_as_a_function_with_its_schema_in_order() {
    let (_, _, schema) = echo();
    let round = tool_round(&[
        echo(),
        ("grab", "Grab a value", json!({ "type": "object" })),
    ]);
    let body = build_request_body(
        &[Message::text(Role::User, "hi")],
        Some(&round.tools),
        &Options::new("m"),
    )
    .expect("the body builds");
    assert_eq!(
        body["tools"],
        json!([
            {
                "type": "function",
                "function": {
                    "name": "example_test_echo",
                    "description": "Echo a value.",
                    "parameters": schema,
                },
            },
            {
                "type": "function",
                "function": {
                    "name": "example_test_grab",
                    "description": "Grab a value",
                    "parameters": { "type": "object" },
                },
            },
        ])
    );
}

#[test]
fn unset_options_send_no_option_fields() {
    let body = build_request_body(
        &[Message::text(Role::User, "hi")],
        None,
        &Options::new("analyst"),
    )
    .expect("the body builds");
    assert_eq!(body["model"], "analyst");
    for field in ["temperature", "max_tokens", "chat_template_kwargs"] {
        assert!(body.get(field).is_none(), "{field} is absent: {body}");
    }
}

#[test]
fn the_body_always_streams_and_asks_for_usage() {
    let body = build_request_body(
        &[Message::text(Role::User, "hi")],
        None,
        &Options::new("analyst"),
    )
    .expect("the body builds");
    assert_eq!(body["model"], "analyst");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert!(body.get("tools").is_none(), "no tools field without tools");
    assert!(body.get("temperature").is_none());
}

#[test]
fn options_and_tools_reach_the_body() {
    let options = Options::new("analyst")
        .with_temperature(0.0)
        .with_max_tokens(NonZeroU32::new(128).expect("128 is non-zero"))
        .with_thinking(false);
    let round = tool_round(&[("echo", "Echo.", json!({ "type": "object" }))]);
    let body = build_request_body(
        &[Message::text(Role::User, "hi")],
        Some(&round.tools),
        &options,
    )
    .expect("the body builds");
    assert_eq!(body["temperature"], 0.0);
    assert_eq!(body["max_tokens"], 128);
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "example_test_echo");
}

#[test]
fn an_empty_tool_list_sends_no_tools_field() {
    let body = build_request_body(
        &[Message::text(Role::User, "hi")],
        Some(&[]),
        &Options::new("m"),
    )
    .expect("the body builds");
    assert!(body.get("tools").is_none());
    assert!(body.get("tool_choice").is_none());
}

#[test]
fn messages_that_do_not_write_as_chat_messages_are_an_invalid_request() {
    let options = Options::new("m");
    let not_an_array = build_request_body(&[json!("hello")], None, &options);
    assert_eq!(
        not_an_array.expect_err("a bare string has no role").kind,
        ErrorKind::InvalidRequest
    );
    let no_role = build_request_body(&[json!({ "content": "hi" })], None, &options);
    let error = no_role.expect_err("a message needs a role");
    assert_eq!(error.kind, ErrorKind::InvalidRequest);
    assert!(
        error.summary.ends_with("message 0 has no string `role`"),
        "{}",
        error.summary
    );
}
