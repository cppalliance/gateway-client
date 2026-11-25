//! The `gateway.json` type: what the Gateway writes after a successful bind
//! and what this reader checks before it attaches.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde::Deserialize;

use super::error::ReadError;
use super::paths::gateway_discovery_file_path;

/// The on-disk gateway discovery file naming a running Gateway's loopback
/// port and bearer key.
///
/// Readers must tolerate unknown fields: a newer Gateway may write fields
/// an older reader does not know, and serde ignores them. The type only
/// deserializes, because this client never writes the file.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct DiscoveryFile {
    /// The Gateway's bound port; readers connect to `127.0.0.1:{port}`.
    pub(crate) port: u16,
    /// The bearer key the Gateway expects.
    pub(crate) api_key: String,
    /// The Gateway process id.
    pub(crate) pid: u32,
    /// The Gateway's boot instant in unix seconds.
    pub(crate) epoch: u64,
    /// The Gateway crate version.
    pub(crate) version: String,
    /// RFC 3339 rendering of the boot instant.
    pub(crate) started_at: String,
}

impl fmt::Debug for DiscoveryFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscoveryFile")
            .field("port", &self.port)
            .field("api_key", &"[REDACTED]")
            .field("pid", &self.pid)
            .field("epoch", &self.epoch)
            .field("version", &"[REDACTED]")
            .field("started_at", &"[REDACTED]")
            .finish()
    }
}

impl DiscoveryFile {
    /// Whether the file names one particular Gateway boot.
    pub(crate) fn has_boot_identity(&self) -> bool {
        self.epoch != 0 && !self.started_at.trim().is_empty()
    }

    /// The reason the file fails validation, or `None` when it is valid.
    ///
    /// Validation covers the fields that attaching depends on: a real port, a
    /// real key, a real pid. The remaining fields are informational.
    #[must_use]
    pub(crate) fn validation_error(&self) -> Option<String> {
        if self.port == 0 {
            return Some("port must not be 0".to_owned());
        }
        if self.api_key.is_empty() {
            return Some("api_key must not be empty".to_owned());
        }
        if self.api_key.contains(['\r', '\n']) {
            return Some("api_key must not contain line breaks".to_owned());
        }
        if self.pid == 0 {
            return Some("pid must not be 0".to_owned());
        }
        None
    }

    /// Reads and validates the gateway discovery file in `run_dir`,
    /// returning `None` when no file exists.
    ///
    /// # Errors
    /// Returns `ReadError::Read` when the file exists but cannot be read,
    /// `ReadError::Parse` when it is not valid JSON, and `ReadError::Invalid`
    /// when it fails validation.
    pub(crate) fn read(run_dir: &Path) -> Result<Option<DiscoveryFile>, ReadError> {
        let path = gateway_discovery_file_path(run_dir);
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ReadError::Read {
                    path,
                    source: error,
                });
            }
        };
        let file: DiscoveryFile =
            serde_json::from_str(&raw).map_err(|source| ReadError::Parse {
                path: path.clone(),
                source,
            })?;
        if let Some(reason) = file.validation_error() {
            return Err(ReadError::Invalid { path, reason });
        }
        Ok(Some(file))
    }

    /// Builds a string containing the local address the Gateway is running on,
    /// for example if the Gateway is running on port `12345` this would
    /// give `http://127.0.0.1:12345`
    #[must_use]
    pub(crate) fn local_addr(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_file() -> DiscoveryFile {
        DiscoveryFile {
            port: 8081,
            api_key: "key".to_owned(),
            pid: 4242,
            epoch: 1_757_000_000,
            version: "0.2.0".to_owned(),
            started_at: "2026-09-03T12:00:00Z".to_owned(),
        }
    }

    /// The JSON the Gateway would write for `file`. The type has no
    /// `Serialize`, because this client never writes the file.
    fn json_of(file: &DiscoveryFile) -> String {
        serde_json::json!({
            "port": file.port,
            "api_key": file.api_key,
            "pid": file.pid,
            "epoch": file.epoch,
            "version": file.version,
            "started_at": file.started_at,
        })
        .to_string()
    }

    #[test]
    fn debug_redacts_bearer_and_untrusted_metadata() {
        let secret = "capability-secret";
        let mut file = valid_file();
        file.api_key = secret.to_owned();
        file.version = format!("version-{secret}\r\n");
        file.started_at = format!("started-{secret}\t");

        let debug = format!("{file:?}");
        assert!(!debug.contains(secret), "debug output redacts the bearer");
        assert!(
            !debug.contains(['\r', '\n', '\t']),
            "untrusted metadata cannot inject debug output"
        );
        assert!(
            debug.contains(&file.port.to_string()),
            "the endpoint remains visible"
        );
    }

    #[test]
    fn unknown_fields_are_tolerated_for_forward_compatibility() {
        let json = r#"{"port":8081,"api_key":"k","pid":1,"epoch":0,"version":"0","started_at":"","future":true}"#;
        let file: DiscoveryFile = serde_json::from_str(json).expect("unknown fields ignored");
        assert_eq!(file.port, 8081);
    }

    #[test]
    fn validation_rejects_invalid_attach_fields() {
        let mut file = valid_file();
        assert_eq!(file.validation_error(), None);
        file.port = 0;
        assert_eq!(
            file.validation_error().as_deref(),
            Some("port must not be 0")
        );
        file = valid_file();
        file.api_key.clear();
        assert_eq!(
            file.validation_error().as_deref(),
            Some("api_key must not be empty")
        );
        file = valid_file();
        file.api_key = "key\r\ninjected: value".to_owned();
        assert_eq!(
            file.validation_error().as_deref(),
            Some("api_key must not contain line breaks")
        );
        file = valid_file();
        file.pid = 0;
        assert_eq!(
            file.validation_error().as_deref(),
            Some("pid must not be 0")
        );
    }

    #[test]
    fn read_returns_none_when_no_file_exists() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        assert_eq!(DiscoveryFile::read(dir.path()).expect("read"), None);
    }

    #[test]
    fn read_rejects_corrupt_json() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        fs::write(dir.path().join("gateway.json"), b"not json").expect("write fixture");
        let error = DiscoveryFile::read(dir.path()).expect_err("corrupt JSON must fail");
        assert!(
            matches!(error, ReadError::Parse { .. }),
            "a corrupt file is a parse error: {error}"
        );
    }

    #[test]
    fn read_rejects_an_invalid_file() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let mut file = valid_file();
        file.port = 0;
        let json = json_of(&file);
        fs::write(dir.path().join("gateway.json"), json).expect("write fixture");
        let error = DiscoveryFile::read(dir.path()).expect_err("an invalid file must fail");
        assert!(
            matches!(error, ReadError::Invalid { .. }),
            "a zero port is a validation error: {error}"
        );
    }

    #[test]
    fn the_reader_parses_the_gateway_fixture() {
        let fixture = include_str!("../../tests/fixtures/gateway.json");
        let file: DiscoveryFile = serde_json::from_str(fixture).expect("the fixture parses");
        assert_eq!(file.port, 52814);
        assert_eq!(file.api_key, "fixture-key");
        assert_eq!(file.pid, 4242);
        assert_eq!(file.epoch, 1_700_000_000);
        assert_eq!(file.version, "0.1.0");
        assert_eq!(file.started_at, "2023-11-14T22:13:20Z");
        assert_eq!(file.validation_error(), None);
        assert_eq!(file.local_addr(), "http://127.0.0.1:52814");
    }
}
