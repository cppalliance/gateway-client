//! The one failure type of the crate: an [`Error`] with an [`ErrorKind`].
//!
//! Every [`Error`] has two texts. `summary` is this crate's own wording and
//! never holds server text, so it is safe to show a model. `message` is the
//! full diagnostic for logs: the summary, then server text and causes.

use std::fmt;

/// What went wrong, in the terms a caller acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The request is larger than the model's context window.
    ContextOverflow,
    /// The server is limiting the request rate.
    RateLimited,
    /// The billing or usage quota is spent.
    QuotaExhausted,
    /// The server is temporarily at capacity.
    Overloaded,
    /// The server declined the content on policy grounds.
    Refused,
    /// The wait for a reply or for its next chunk timed out.
    Timeout,
    /// The connection failed or the stream broke.
    Transport,
    /// The server reported a fault of its own.
    ServerError,
    /// The server refused the request for any other reason.
    Rejected,
    /// The reply was malformed or over the size limit.
    MalformedResponse,
    /// The model's reply had no text and no tool calls.
    EmptyReply,
    /// Model access is turned off or not configured.
    Unavailable,
    /// The caller's setup is wrong: a bad URL, a missing variable, an empty key.
    InvalidConfig,
    /// The messages could not be written as chat messages.
    InvalidRequest,
}

impl ErrorKind {
    /// Every kind, in declaration order, for tests that must visit them all.
    /// The match in [`ErrorKind::phrase`] has no wildcard arm, so adding a
    /// kind fails to compile until it is handled there. This list is the one
    /// place the compiler cannot check.
    pub const ALL: [ErrorKind; 14] = [
        ErrorKind::ContextOverflow,
        ErrorKind::RateLimited,
        ErrorKind::QuotaExhausted,
        ErrorKind::Overloaded,
        ErrorKind::Refused,
        ErrorKind::Timeout,
        ErrorKind::Transport,
        ErrorKind::ServerError,
        ErrorKind::Rejected,
        ErrorKind::MalformedResponse,
        ErrorKind::EmptyReply,
        ErrorKind::Unavailable,
        ErrorKind::InvalidConfig,
        ErrorKind::InvalidRequest,
    ];

    /// The fixed wording of the kind. It never holds server text.
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        match self {
            ErrorKind::ContextOverflow => "the request is larger than the model's context window",
            ErrorKind::RateLimited => "the model backend is limiting the request rate",
            ErrorKind::QuotaExhausted => "the model backend says the usage quota is spent",
            ErrorKind::Overloaded => "the model backend is overloaded",
            ErrorKind::Refused => "the model backend refused the request on content policy grounds",
            ErrorKind::Timeout => "the model backend did not answer in time",
            ErrorKind::Transport => "the connection to the model backend failed",
            ErrorKind::ServerError => "the model backend reported a fault of its own",
            ErrorKind::Rejected => "the model backend rejected the request",
            ErrorKind::MalformedResponse => {
                "the model backend sent a reply that could not be understood"
            }
            ErrorKind::EmptyReply => "the model replied with no text and no tool calls",
            ErrorKind::Unavailable => "model access is turned off or not configured",
            ErrorKind::InvalidConfig => "the client is not set up correctly",
            ErrorKind::InvalidRequest => "the request messages could not be sent as chat messages",
        }
    }
}

/// A failed round, model list, or setup step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// What went wrong.
    pub kind: ErrorKind,
    /// The full diagnostic for logs: the summary, then server text and causes.
    pub message: String,
    /// Wording that is safe to show a model. It holds this crate's text and
    /// never server text.
    pub summary: String,
    /// The HTTP status of a failed response, when there was one.
    pub status: Option<u16>,
    /// The `finish_reason` of an empty reply.
    pub finish_reason: Option<String>,
    /// Tokens in the prompt, when a context overflow named them.
    pub prompt_tokens: Option<u32>,
    /// The model's context window, when a context overflow named it.
    pub context_window: Option<u32>,
    /// Which setup rule failed, for [`ErrorKind::InvalidConfig`].
    pub reason: Option<&'static str>,
    /// The environment variable a setup error names.
    pub name: Option<String>,
}

impl Error {
    /// `message` is both the message and the summary.
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        let message = message.into();
        Error {
            kind,
            summary: message.clone(),
            message,
            status: None,
            finish_reason: None,
            prompt_tokens: None,
            context_window: None,
            reason: None,
            name: None,
        }
    }

    /// The kind's fixed phrase alone.
    pub(crate) fn phrased(kind: ErrorKind) -> Error {
        Error::new(kind, kind.phrase())
    }

    /// The kind's phrase, then `: ` and `specific`, which this crate wrote.
    pub(crate) fn specific(kind: ErrorKind, specific: impl fmt::Display) -> Error {
        Error::new(kind, format!("{}: {specific}", kind.phrase()))
    }

    pub(crate) fn malformed(specific: impl fmt::Display) -> Error {
        Error::specific(ErrorKind::MalformedResponse, specific)
    }

    pub(crate) fn invalid_request(specific: impl fmt::Display) -> Error {
        Error::specific(ErrorKind::InvalidRequest, specific)
    }

    pub(crate) fn unavailable() -> Error {
        Error::phrased(ErrorKind::Unavailable)
    }

    /// An empty reply. The text adds a specific when the reader dropped a
    /// reasoning side channel that it could not use as answer text.
    pub(crate) fn empty_reply(reasoning_present: bool) -> Error {
        if reasoning_present {
            Error::specific(
                ErrorKind::EmptyReply,
                "reasoning content was present but ignored",
            )
        } else {
            Error::phrased(ErrorKind::EmptyReply)
        }
    }

    pub(crate) fn context_overflow(
        prompt_tokens: Option<u32>,
        context_window: Option<u32>,
        message: impl Into<String>,
    ) -> Error {
        let mut error = Error::new(ErrorKind::ContextOverflow, message);
        error.prompt_tokens = prompt_tokens;
        error.context_window = context_window;
        error
    }

    /// A setup failure. `reason` is one of `missing_env`, `invalid_env`,
    /// `invalid_config`, and `empty_secret`.
    pub(crate) fn setup(reason: &'static str, message: impl Into<String>) -> Error {
        let mut error = Error::new(ErrorKind::InvalidConfig, message);
        error.reason = Some(reason);
        error
    }

    pub(crate) fn with_summary(mut self, summary: impl Into<String>) -> Error {
        self.summary = summary.into();
        self
    }

    /// Appends server text to the message after `: `. The summary stays as it
    /// was.
    pub(crate) fn with_detail(mut self, text: &str) -> Error {
        self.message.push_str(": ");
        self.message.push_str(text);
        self
    }

    /// Appends `cause`, and each cause under it, to the message after `: `,
    /// skipping any text the message already holds. A cause whose text starts
    /// with the message's end and then `: ` adds only what follows. The
    /// summary stays as it was.
    pub(crate) fn with_cause(mut self, cause: &(dyn std::error::Error + 'static)) -> Error {
        let mut next = Some(cause);
        while let Some(current) = next {
            let text = current.to_string();
            if !text.is_empty() && !self.message.contains(&text) {
                let repeated = text
                    .rmatch_indices(": ")
                    .map(|(at, _)| at)
                    .find(|&at| at > 0 && self.message.ends_with(&text[..at]));
                if let Some(at) = repeated {
                    self.message.push_str(&text[at..]);
                } else {
                    self.message.push_str(": ");
                    self.message.push_str(&text);
                }
            }
            next = current.source();
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary)
    }
}

impl std::error::Error for Error {}
