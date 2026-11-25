//! The tools and the request settings of one round.

use std::num::NonZeroU32;

use serde_json::{Map, Value};

/// A tool offered to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    name: String,
    description: String,
    parameters: Map<String, Value>,
}

impl Tool {
    /// `parameters` is the JSON Schema object that describes the tool's
    /// arguments, for example `{"type": "object", "properties": {...},
    /// "required": [...]}`. The server defines what else it may hold. The
    /// type says it is an object, which is the one rule every server shares,
    /// and leaves the schema's keywords open.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Map<String, Value>,
    ) -> Tool {
        Tool {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }

    /// The name the model calls the tool by.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The sentence that tells the model what the tool does.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The JSON Schema object for the tool's arguments.
    #[must_use]
    pub fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }
}

/// The request settings of one round.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    model: String,
    temperature: Option<f64>,
    max_tokens: Option<NonZeroU32>,
    thinking: Option<bool>,
}

impl Options {
    /// Settings for `model`, with the temperature, the token cap, and the
    /// thinking switch left to the server.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Options {
        Options {
            model: model.into(),
            temperature: None,
            max_tokens: None,
            thinking: None,
        }
    }

    /// Sets the sampling temperature. The server decides what range it
    /// accepts.
    #[must_use]
    pub fn with_temperature(mut self, temperature: f64) -> Options {
        self.temperature = Some(temperature);
        self
    }

    /// Sets the cap on the tokens of the reply.
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: NonZeroU32) -> Options {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Turns the model's thinking on or off, for a server that has the
    /// switch.
    #[must_use]
    pub fn with_thinking(mut self, thinking: bool) -> Options {
        self.thinking = Some(thinking);
        self
    }

    /// The model name the request sends.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The sampling temperature, when set.
    #[must_use]
    pub fn temperature(&self) -> Option<f64> {
        self.temperature
    }

    /// The cap on the tokens of the reply, when set.
    #[must_use]
    pub fn max_tokens(&self) -> Option<NonZeroU32> {
        self.max_tokens
    }

    /// The thinking switch, when set.
    #[must_use]
    pub fn thinking(&self) -> Option<bool> {
        self.thinking
    }
}
