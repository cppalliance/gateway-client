//! What the client returns for one round: plain data, with nothing from
//! the Runvil Engine.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::error::Error;

/// One tool call the model asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    id: String,
    name: String,
    arguments: Map<String, Value>,
}

impl ToolCall {
    /// Builds a call from decoded parts.
    ///
    /// # Errors
    /// `MalformedResponse` when `id` or `name` is blank or `arguments` is not
    /// a JSON object.
    pub(crate) fn from_parts(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: Value,
    ) -> Result<ToolCall, Error> {
        let id = id.into();
        let name = name.into();
        if id.trim().is_empty() {
            return Err(Error::malformed("tool call id was blank"));
        }
        if name.trim().is_empty() {
            return Err(Error::malformed("tool call name was blank"));
        }
        let Value::Object(arguments) = arguments else {
            return Err(Error::malformed(
                "tool call arguments were not a JSON object",
            ));
        };
        Ok(ToolCall {
            id,
            name,
            arguments,
        })
    }

    /// The id the model gave the call, which its result answers.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The name of the tool to run.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The call's arguments, a JSON object.
    #[must_use]
    pub fn arguments(&self) -> &Map<String, Value> {
        &self.arguments
    }
}

/// What the model produced: an answer or a request to call tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The model answered in text.
    Text(String),
    /// The model asked to call one or more tools.
    ToolCalls(Vec<ToolCall>),
}

/// Token counts as the server reported them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    /// Tokens in the prompt.
    pub prompt_tokens: u32,
    /// Tokens in the reply.
    pub completion_tokens: u32,
    /// The prompt tokens plus the reply tokens.
    pub total_tokens: u32,
    /// Prompt tokens served from a prefix cache, when the server says.
    pub cached_tokens: Option<u32>,
    /// Tokens spent on reasoning, when the server says.
    pub reasoning_tokens: Option<u32>,
}

impl Usage {
    /// The counts of the prompt, the reply, and their total, with neither
    /// detail reported.
    #[must_use]
    pub fn new(prompt_tokens: u32, completion_tokens: u32, total_tokens: u32) -> Usage {
        Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens: None,
            reasoning_tokens: None,
        }
    }

    /// Sets the prompt tokens served from a prefix cache.
    #[must_use]
    pub fn with_cached_tokens(mut self, cached_tokens: Option<u32>) -> Usage {
        self.cached_tokens = cached_tokens;
        self
    }

    /// Sets the tokens spent on reasoning.
    #[must_use]
    pub fn with_reasoning_tokens(mut self, reasoning_tokens: Option<u32>) -> Usage {
        self.reasoning_tokens = reasoning_tokens;
        self
    }
}

/// Timing the client measured on its own clock.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientTiming {
    /// Milliseconds from sending the request to the first streamed token,
    /// when the stream produced one.
    pub ttft_ms: Option<f64>,
    /// Mean inter-token latency in milliseconds, when at least two tokens
    /// streamed.
    pub mean_itl_ms: Option<f64>,
    /// Milliseconds from sending the request to the completed response.
    pub e2e_ms: f64,
}

impl ClientTiming {
    /// The timing of a call that took `e2e_ms` milliseconds, with no
    /// streamed-token timings.
    #[must_use]
    pub fn new(e2e_ms: f64) -> ClientTiming {
        ClientTiming {
            ttft_ms: None,
            mean_itl_ms: None,
            e2e_ms,
        }
    }

    /// Sets the milliseconds to the first streamed token.
    #[must_use]
    pub fn with_ttft_ms(mut self, ttft_ms: Option<f64>) -> ClientTiming {
        self.ttft_ms = ttft_ms;
        self
    }

    /// Sets the mean inter-token latency in milliseconds.
    #[must_use]
    pub fn with_mean_itl_ms(mut self, mean_itl_ms: Option<f64>) -> ClientTiming {
        self.mean_itl_ms = mean_itl_ms;
        self
    }
}

/// The finished reply of one round.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    outcome: Outcome,
    model: String,
    finish_reason: Option<String>,
    reasoning: Option<String>,
    usage: Option<Usage>,
    timing: Option<ClientTiming>,
    diagnostics: Vec<String>,
    request: Value,
    response: Value,
}

impl Reply {
    /// Builds a reply from a turn outcome and the model label.
    ///
    /// # Errors
    /// `EmptyReply` for an empty batch of tool calls, and `MalformedResponse`
    /// when two calls in the batch share an id.
    pub(crate) fn new(
        outcome: Outcome,
        model: impl Into<String>,
        request: Value,
        response: Value,
    ) -> Result<Reply, Error> {
        if let Outcome::ToolCalls(calls) = &outcome {
            if calls.is_empty() {
                return Err(Error::empty_reply(false));
            }
            let mut seen = HashSet::new();
            for call in calls {
                if !seen.insert(call.id()) {
                    return Err(Error::malformed(format!(
                        "duplicate tool call id {:?} within one turn",
                        call.id()
                    )));
                }
            }
        }
        Ok(Reply {
            outcome,
            model: model.into(),
            finish_reason: None,
            reasoning: None,
            usage: None,
            timing: None,
            diagnostics: Vec::new(),
            request,
            response,
        })
    }

    pub(crate) fn with_finish_reason(mut self, finish_reason: String) -> Reply {
        self.finish_reason = Some(finish_reason);
        self
    }

    pub(crate) fn with_reasoning(mut self, reasoning: String) -> Reply {
        self.reasoning = Some(reasoning);
        self
    }

    pub(crate) fn with_usage(mut self, usage: Option<Usage>) -> Reply {
        self.usage = usage;
        self
    }

    pub(crate) fn with_timing(mut self, timing: Option<ClientTiming>) -> Reply {
        self.timing = timing;
        self
    }

    pub(crate) fn with_diagnostics(mut self, diagnostics: Vec<String>) -> Reply {
        self.diagnostics = diagnostics;
        self
    }

    /// What the model produced.
    #[must_use]
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    /// The model the request named, or the one the response named when the
    /// request named none.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The `finish_reason` of the reply, when the server sent one.
    #[must_use]
    pub fn finish_reason(&self) -> Option<&str> {
        self.finish_reason.as_deref()
    }

    /// Reasoning text from the server. It is never promoted into the answer.
    #[must_use]
    pub fn reasoning(&self) -> Option<&str> {
        self.reasoning.as_deref()
    }

    /// Token counts, when the server reported usage.
    #[must_use]
    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }

    /// Timing the client measured on its own clock.
    #[must_use]
    pub fn timing(&self) -> Option<&ClientTiming> {
        self.timing.as_ref()
    }

    /// One line for each part of the response that was present but unusable,
    /// and for a response that named no model.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// The request body that was sent.
    #[must_use]
    pub fn request(&self) -> &Value {
        &self.request
    }

    /// The response, reassembled into the buffered chat-completion shape when
    /// it streamed.
    #[must_use]
    pub fn response(&self) -> &Value {
        &self.response
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::error::ErrorKind;

    fn call(id: &str) -> ToolCall {
        ToolCall::from_parts(id, "fetch", json!({})).expect("a whole call")
    }

    #[test]
    fn a_blank_id_is_a_malformed_reply_with_its_specific() {
        let error = ToolCall::from_parts(" ", "fetch", json!({})).expect_err("a blank id");
        assert_eq!(error.kind, ErrorKind::MalformedResponse);
        assert_eq!(
            error.to_string(),
            "the model backend sent a reply that could not be understood: tool call id was blank"
        );
    }

    #[test]
    fn a_blank_name_and_non_object_arguments_are_refused() {
        let blank = ToolCall::from_parts("a", "", json!({})).expect_err("a blank name");
        assert!(blank.summary.ends_with("tool call name was blank"));
        let array = ToolCall::from_parts("a", "fetch", json!([])).expect_err("not an object");
        assert!(
            array
                .summary
                .ends_with("tool call arguments were not a JSON object")
        );
    }

    #[test]
    fn two_calls_with_one_id_are_refused() {
        let error = Reply::new(
            Outcome::ToolCalls(vec![call("dup"), call("dup")]),
            "m",
            json!({}),
            json!({}),
        )
        .expect_err("a duplicate id");
        assert_eq!(error.kind, ErrorKind::MalformedResponse);
        assert!(
            error
                .summary
                .ends_with("duplicate tool call id \"dup\" within one turn")
        );
    }

    #[test]
    fn an_empty_batch_of_calls_is_an_empty_reply() {
        let error = Reply::new(Outcome::ToolCalls(Vec::new()), "m", json!({}), json!({}))
            .expect_err("an empty batch");
        assert_eq!(error.kind, ErrorKind::EmptyReply);
    }
}
