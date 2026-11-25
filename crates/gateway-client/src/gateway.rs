//! Where a Gateway is and how to log in: the [`Gateway`] type and the ways to
//! find one.
//!
//! A program that knows the address builds a [`Gateway`] with
//! [`Gateway::new`]. A program on the machine of a running Gateway finds it
//! with `Gateway::find`, which reads the file the Gateway writes. A program
//! that is configured through the environment opts in with
//! [`Gateway::find_env`] or [`Gateway::find_env_named`]. Nothing here reads
//! the environment unless the program asks.

#[cfg(feature = "discovery")]
use std::path::Path;

use crate::config::{
    GatewayEndpoint, SecretString, make_invalid_env_error, make_missing_env_error,
};
#[cfg(feature = "discovery")]
use crate::discovery::DiscoveryFile;
use crate::error::Error;
#[cfg(feature = "discovery")]
use crate::error::ErrorKind;
use crate::transport::GatewayChat;

/// The environment variable that names the Gateway's API root.
const DEFAULT_URL_VAR: &str = "RUNVIL_GATEWAY_URL";
/// The environment variable that holds the Gateway's shared bearer key.
const DEFAULT_KEY_VAR: &str = "RUNVIL_GATEWAY_API_KEY";

/// Where a Gateway is and how to log in: an API root and, when it needs one, a
/// bearer key.
///
/// The key is a [`SecretString`], so `Debug` output and logs never show it.
#[derive(Clone, Debug)]
pub struct Gateway {
    endpoint: GatewayEndpoint,
    key: Option<SecretString>,
}

impl Gateway {
    /// A Gateway at a known address, with no key.
    #[must_use]
    pub fn new(endpoint: GatewayEndpoint) -> Gateway {
        Gateway {
            endpoint,
            key: None,
        }
    }

    /// The same Gateway, logged in with `key`.
    #[must_use]
    pub fn with_key(mut self, key: SecretString) -> Gateway {
        self.key = Some(key);
        self
    }

    /// The Gateway's API root, its `/v1` URL.
    #[must_use]
    pub fn endpoint(&self) -> &GatewayEndpoint {
        &self.endpoint
    }

    /// The bearer key, when there is one.
    #[must_use]
    pub fn key(&self) -> Option<&SecretString> {
        self.key.as_ref()
    }

    /// A client for this Gateway. It sends the key when there is one.
    #[must_use]
    pub fn client(&self) -> GatewayChat {
        match &self.key {
            Some(key) => GatewayChat::new(self.endpoint.clone(), key.clone()),
            None => GatewayChat::keyless(self.endpoint.clone()),
        }
    }

    /// Reads the Gateway from `RUNVIL_GATEWAY_URL` and `RUNVIL_GATEWAY_API_KEY`.
    ///
    /// The program opts in by calling it. The URL is required. The key is
    /// optional when the URL's host is loopback (`127.0.0.1`, `::1`,
    /// `localhost`), because a Gateway on the same machine admits keyless
    /// loopback callers by default, and required for every other host. An
    /// empty key counts as missing.
    ///
    /// # Errors
    /// Returns an [`InvalidConfig`](crate::ErrorKind::InvalidConfig) error
    /// when the URL is missing or invalid, when either variable is set to a
    /// value that is not valid Unicode, or when the host is not loopback and
    /// the key is missing or empty.
    pub fn find_env() -> Result<Gateway, Error> {
        Gateway::find_env_named(DEFAULT_URL_VAR, DEFAULT_KEY_VAR)
    }

    /// The same read as [`Gateway::find_env`], with the variable names the
    /// program chooses.
    ///
    /// # Errors
    /// The errors of [`Gateway::find_env`], naming `url_var` and `key_var`.
    pub fn find_env_named(url_var: &str, key_var: &str) -> Result<Gateway, Error> {
        from_lookup(url_var, key_var, |name| match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            // A set-but-non-Unicode value is a real misconfiguration, surfaced
            // explicitly instead of being silently treated as "not set".
            Err(std::env::VarError::NotUnicode(_)) => Err(make_invalid_env_error(name)),
        })
    }

    /// Finds a Gateway on this machine through `<home>/.runvil/run/gateway.json`.
    ///
    /// The call blocks for up to about two seconds while it proves the Gateway
    /// is live. A program on an async runtime calls it from a blocking task.
    ///
    /// # Errors
    /// Returns an [`Unavailable`](crate::ErrorKind::Unavailable) error when
    /// the home directory cannot be found, when there is no file, when the
    /// file is invalid or names a Gateway that is not live, or when the
    /// Gateway is older than this client works with. The error says which.
    #[cfg(feature = "discovery")]
    pub fn find() -> Result<Gateway, Error> {
        let Some(run_dir) = crate::discovery::default_run_dir() else {
            return Err(Error::specific(
                ErrorKind::Unavailable,
                "the user's home directory could not be found",
            ));
        };
        Gateway::find_in(&run_dir)
    }

    /// [`Gateway::find`] with the run directory named by the caller.
    ///
    /// # Errors
    /// The errors of [`Gateway::find`].
    #[cfg(feature = "discovery")]
    pub fn find_in(run_dir: &Path) -> Result<Gateway, Error> {
        let Some(file) = crate::discovery::read_live(run_dir)? else {
            return Err(Error::specific(
                ErrorKind::Unavailable,
                format!(
                    "no gateway is running: there is no gateway.json in {}",
                    run_dir.display()
                ),
            ));
        };
        Gateway::from_file(file)
    }

    /// The Gateway that a live discovery file names, once its version passes.
    ///
    /// The file's loopback address gets the /v1 suffix, which is how a
    /// Gateway serves its API, and the file's key becomes the bearer key.
    #[cfg(feature = "discovery")]
    fn from_file(file: DiscoveryFile) -> Result<Gateway, Error> {
        crate::discovery::check_version(&file.version)?;
        let endpoint = GatewayEndpoint::new(&format!("{}/v1", file.local_addr()))?;
        let key = SecretString::new(file.api_key)?;
        Ok(Gateway::new(endpoint).with_key(key))
    }
}

/// The environment read behind [`Gateway::find_env_named`], with the lookup
/// injected so tests need not touch the process environment.
///
/// The key is optional exactly when the URL's host is loopback; an empty key
/// counts as unset ([`SecretString::new`] refuses only an empty secret, and
/// `Result::ok` folds that refusal into `None`).
fn from_lookup(
    url_var: &str,
    key_var: &str,
    lookup: impl Fn(&str) -> Result<Option<String>, Error>,
) -> Result<Gateway, Error> {
    let base_url = lookup(url_var)?.ok_or_else(|| make_missing_env_error(url_var))?;
    let endpoint = GatewayEndpoint::new(&base_url)?;
    let key = lookup(key_var)?.map(SecretString::new).and_then(Result::ok);
    match key {
        Some(key) => Ok(Gateway::new(endpoint).with_key(key)),
        None if endpoint.is_loopback() => Ok(Gateway::new(endpoint)),
        None => Err(make_missing_env_error(key_var)),
    }
}

#[cfg(test)]
#[path = "gateway-tests.rs"]
mod tests;
