//! Client configuration: the redacted bearer secret, the validated gateway
//! endpoint, and the setup errors a bad setup reports.
//!
//! A setup error happens before the client sends its first request. It is
//! an [`InvalidConfig`](crate::ErrorKind::InvalidConfig) whose message names the
//! environment variable or the rule that failed, never a key, and whose
//! `reason` is `missing_env`, `invalid_env`, `invalid_config`, or
//! `empty_secret`.

use std::fmt;

use crate::error::Error;

/// The setup error for a required environment variable that is not set.
pub(crate) fn make_missing_env_error(name: &str) -> Error {
    let mut error = Error::setup(
        "missing_env",
        format!("missing environment variable: {name}"),
    );
    error.name = Some(name.into());
    error
}

/// The setup error for an environment variable whose value is not valid
/// Unicode.
pub(crate) fn make_invalid_env_error(name: &str) -> Error {
    let mut error = Error::setup(
        "invalid_env",
        format!("environment variable is set but not valid Unicode: {name}"),
    );
    error.name = Some(name.into());
    error
}

/// A bearer credential whose contents never appear in `Debug` or `Display`
/// output or in logs.
///
/// Wrap a secret, such as the gateway bearer key, in a `SecretString` as soon
/// as you read it, so an accidental `{:?}` or log line cannot leak it. The
/// key is read in exactly two places: the `Authorization` header of a
/// request, and the entry a caller writes for a program that presents the key
/// to the gateway itself, through [`SecretString::expose_secret`].
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    /// Wraps a secret so it is redacted everywhere it is formatted.
    ///
    /// # Errors
    /// Returns an [`InvalidConfig`](crate::ErrorKind::InvalidConfig) error with the
    /// reason `empty_secret` when `secret` is empty, so a client can never be
    /// built to authenticate with a blank bearer credential.
    pub fn new(secret: impl Into<String>) -> std::result::Result<SecretString, Error> {
        let secret = secret.into();
        if secret.is_empty() {
            return Err(Error::setup("empty_secret", "secret must not be empty")
                .with_summary("The secret is empty. Supply a non-empty secret."));
        }
        Ok(SecretString(secret))
    }

    /// Borrows the raw secret. The key is read in exactly two places: the
    /// `Authorization` header of a request, and the entry a caller writes for
    /// a program that presents the key to the gateway itself.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// A validated base URL for the gateway's OpenAI-compatible API (its `/v1`
/// root).
///
/// Construction requires an `http` or `https` scheme and a host, so a
/// client always points at a usable endpoint. Any trailing slash is removed
/// so request paths join cleanly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayEndpoint {
    pub(crate) url: String,
    /// Whether the host names the local machine: `localhost`, or an IP whose
    /// `is_loopback()` holds. Decided at construction from the parsed host.
    loopback: bool,
}

impl GatewayEndpoint {
    /// Validates and normalizes a gateway base URL.
    ///
    /// The URL is checked with a strict URL parser.
    ///
    /// # Errors
    /// Returns an `invalid_config` setup error ([`ErrorKind::InvalidConfig`](crate::ErrorKind::InvalidConfig))
    /// when `url` fails to parse as
    /// an absolute URL, uses a scheme other than `http` or `https`, omits the
    /// host, embeds credentials (a `user:pass@` component), or has a query or
    /// fragment.
    /// A query or fragment is rejected because an API root is a bare path.
    /// No error includes `url`, because a URL can contain a credential.
    pub fn new(url: &str) -> std::result::Result<GatewayEndpoint, Error> {
        let reject = |message: String| Error::setup("invalid_config", message);
        let trimmed = url.trim();
        // The parse failure's own text names the rule, never the input.
        let parsed = url::Url::parse(trimmed)
            .map_err(|error| reject(format!("gateway URL is not a valid URL: {error}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(reject(
                "gateway URL must use the http or https scheme".to_owned(),
            ));
        }
        let loopback = match parsed.host() {
            None | Some(url::Host::Domain("")) => {
                return Err(reject("gateway URL names no host".to_owned()));
            }
            // The URL parser lowercases the host of an http(s) URL, so the
            // literal comparison covers `LOCALHOST` too.
            Some(url::Host::Domain(domain)) => domain == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        };
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(reject(
                "gateway URL must not embed credentials (user:pass@)".to_owned(),
            ));
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(reject(
                "gateway URL must not include a query or fragment".to_owned(),
            ));
        }
        Ok(GatewayEndpoint {
            // Normalized by the URL parser; trim the trailing slash so request
            // paths (`{base}/chat/completions`) join cleanly.
            url: parsed.as_str().trim_end_matches('/').to_string(),
            loopback,
        })
    }

    /// Returns the normalized base URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns whether the endpoint's host is the local machine.
    ///
    /// It returns `true` for `localhost`, for `127.0.0.1` and the rest of
    /// `127.0.0.0/8`, and for `::1`. It returns `false` for every other name
    /// or address. By default, a gateway on the local machine accepts callers
    /// that omit the key, so
    /// [`Gateway::find_env`](crate::Gateway::find_env) makes the bearer key
    /// optional exactly when this returns `true`.
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        self.loopback
    }
}

impl TryFrom<&str> for GatewayEndpoint {
    type Error = Error;

    fn try_from(url: &str) -> std::result::Result<GatewayEndpoint, Error> {
        GatewayEndpoint::new(url)
    }
}
