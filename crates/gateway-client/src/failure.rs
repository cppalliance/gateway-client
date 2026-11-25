//! The failures the transport builds from errors of `reqwest` and `tokio`,
//! and thin names for the constructors of [`Error`] that the wire code calls.

use std::fmt::Display;

use crate::error::{Error, ErrorKind};

/// A send or read that failed: `Timeout` when `error` was a timeout and
/// `Transport` otherwise, with `error` appended to the message as its cause.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a `map_err` adapter takes the error by value"
)]
pub(crate) fn make_transport_failure_error(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        return make_timeout_error(&error);
    }
    Error::phrased(ErrorKind::Transport).with_cause(&error)
}

/// A receive deadline that elapsed: a `Timeout` with the elapsed error
/// appended to the message as its cause.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a `map_err` adapter takes the error by value"
)]
pub(crate) fn make_elapsed_error(error: tokio::time::error::Elapsed) -> Error {
    make_timeout_error(&error)
}

fn make_timeout_error(cause: &(dyn std::error::Error + 'static)) -> Error {
    Error::phrased(ErrorKind::Timeout).with_cause(cause)
}

/// The failure of a client built with `GatewayChat::disabled`: model access
/// is off, so no round is sent.
pub(crate) fn make_unavailable_error() -> Error {
    Error::unavailable()
}

/// A `MalformedResponse` failure: the kind's fixed phrase extended with `: `
/// and `specific`, which this crate's own code wrote. Server text never goes
/// in `specific`.
pub(crate) fn make_malformed_error(specific: impl Display) -> Error {
    Error::malformed(specific)
}

/// Appends server `text`, already bounded and control-escaped, to `error`'s
/// message after `: `. The summary stays as it was, so server text never
/// reaches the model.
pub(crate) fn with_detail(error: Error, text: &str) -> Error {
    error.with_detail(text)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn each_helper_uses_the_fixed_phrase_for_its_code() {
        let elapsed_error =
            tokio::time::timeout(Duration::from_millis(1), std::future::pending::<()>())
                .await
                .expect_err("a pending future elapses");
        let cause = elapsed_error.to_string();
        let timed_out = make_elapsed_error(elapsed_error);
        assert_eq!(timed_out.kind, ErrorKind::Timeout);
        assert_eq!(
            timed_out.summary,
            "the model backend did not answer in time"
        );
        assert!(
            timed_out.message.contains(&cause) && !timed_out.summary.contains(&cause),
            "the elapsed error is in the message only: {timed_out:?}"
        );

        let disabled = make_unavailable_error();
        assert_eq!(disabled.kind, ErrorKind::Unavailable);
        assert_eq!(
            disabled.to_string(),
            "model access is turned off or not configured"
        );

        let unreadable = make_malformed_error("model list body exceeds the 8-byte limit");
        assert_eq!(unreadable.kind, ErrorKind::MalformedResponse);
        assert_eq!(
            unreadable.to_string(),
            "the model backend sent a reply that could not be understood: \
             model list body exceeds the 8-byte limit"
        );
        assert_eq!(unreadable.message, unreadable.summary);
    }
}
