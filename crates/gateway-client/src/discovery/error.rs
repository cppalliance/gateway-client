//! The error of reading `gateway.json`.
//!
//! `ReadError` is what `DiscoveryFile::read` returns. A file that is read
//! and valid but names a Gateway that is not live is a different failure,
//! `StaleReason` in the liveness module.

use std::path::PathBuf;

/// A failure to turn `gateway.json` into a valid `DiscoveryFile`.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    /// The file existed but could not be read.
    #[error("read {path}")]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The file was not valid JSON for the fields it needs.
    #[error("parse {path}")]
    Parse {
        /// The file that could not be parsed.
        path: PathBuf,
        /// The underlying JSON error.
        #[source]
        source: serde_json::Error,
    },

    /// The file failed the static checks on its fields.
    #[error("invalid gateway discovery file {path}: {reason}")]
    Invalid {
        /// The file that failed validation.
        path: PathBuf,
        /// The broken invariant.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::ReadError;
    use std::error::Error as _;
    use std::path::PathBuf;

    #[test]
    fn the_parse_variant_returns_the_json_error_as_its_source() {
        let Err(json) = serde_json::from_str::<u32>("nope") else {
            panic!("`nope` must not parse as a u32");
        };
        let error = ReadError::Parse {
            path: PathBuf::from("gateway.json"),
            source: json,
        };
        let Some(cause) = error.source() else {
            panic!("the parse variant returns its JSON cause from source()");
        };
        let Some(json) = cause.downcast_ref::<serde_json::Error>() else {
            panic!("the JSON cause is the serde_json error");
        };
        assert!(json.is_syntax());
    }
}
