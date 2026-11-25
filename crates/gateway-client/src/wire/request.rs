//! The chat-completions request body: the one JSON shape every transport
//! sends for a round, built from the messages, the tools, and the options.

use serde::Serialize;
use serde_json::Value;

use crate::error::Error;
use crate::options::{Options, Tool};

/// Builds the JSON body of a streaming chat-completions request.
///
/// The body names the model given by `options.model()` and holds `messages`
/// as `serde` writes them. Every request streams, so `stream` is always true.
/// The body also sets `stream_options.include_usage`. That asks the backend
/// to send token usage in a final chunk with an empty `choices` list, so
/// token counts still arrive when the reply streams as server-sent events.
///
/// When `tools` is `Some` and holds at least one tool, the body sends each
/// tool in its `tools` array as an `OpenAI` function tool. Each entry is an
/// object with `type` set to `"function"` and a `function` object that holds
/// the tool's `name`, `description`, and `parameters`. The body then also
/// sets `tool_choice` to `"auto"`. When `tools` is `None` or an empty slice,
/// the body omits both fields and the request is a plain chat completion.
///
/// The optional settings in `options` extend the body only when set.
/// `temperature` and `max_tokens` become fields of the same name. `thinking`
/// becomes `chat_template_kwargs.enable_thinking`.
///
/// A transport must build its request body here. It sends the result as the
/// JSON body of its chat-completions request, and later hands the same value
/// to the stream reader. One shared builder keeps every transport sending the
/// same request.
///
/// # Errors
/// `InvalidRequest` when the messages do not write as JSON, or do not write
/// as an array of objects that each have a string `role`.
pub(crate) fn build_request_body<M: Serialize>(
    messages: &[M],
    tools: Option<&[Tool]>,
    options: &Options,
) -> Result<Value, Error> {
    let messages = serde_json::to_value(messages).map_err(|error| {
        Error::invalid_request("the messages could not be written as JSON").with_cause(&error)
    })?;
    check_messages(&messages)?;
    let mut body = serde_json::json!({
        "model": options.model(),
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    if let Some(tools) = tools.filter(|tools| !tools.is_empty()) {
        let wrapped: Vec<Value> = tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.parameters(),
                    },
                })
            })
            .collect();
        body["tools"] = Value::Array(wrapped);
        body["tool_choice"] = Value::String("auto".into());
    }
    if let Some(temperature) = options.temperature() {
        body["temperature"] = serde_json::json!(temperature);
    }
    if let Some(max_tokens) = options.max_tokens() {
        body["max_tokens"] = serde_json::json!(max_tokens.get());
    }
    if let Some(thinking) = options.thinking() {
        body["chat_template_kwargs"] = serde_json::json!({
            "enable_thinking": thinking,
        });
    }
    Ok(body)
}

/// The messages must write as a JSON array of objects that each have a string
/// `role`.
fn check_messages(messages: &Value) -> Result<(), Error> {
    let Value::Array(items) = messages else {
        return Err(Error::invalid_request(
            "the messages did not write as a JSON array",
        ));
    };
    for (index, item) in items.iter().enumerate() {
        if !matches!(item.get("role"), Some(Value::String(_))) {
            return Err(Error::invalid_request(format!(
                "message {index} has no string `role`"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "request-tests.rs"]
mod tests;
