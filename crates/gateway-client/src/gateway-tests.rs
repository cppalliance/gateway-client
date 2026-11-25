//! The environment read, the endpoint and key accessors, and the discovery
//! path of [`Gateway`].

use super::*;
use crate::error::ErrorKind;

/// A lookup that answers from a fixed list of variables, so no test touches
/// the process environment.
fn lookup_from<'a>(
    pairs: &'a [(&'a str, &'a str)],
) -> impl Fn(&str) -> Result<Option<String>, Error> + 'a {
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    move |name| {
        Ok(pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone()))
    }
}

/// Reads the default variables from a fixed list.
fn find_from(pairs: &[(&str, &str)]) -> Result<Gateway, Error> {
    from_lookup(DEFAULT_URL_VAR, DEFAULT_KEY_VAR, lookup_from(pairs))
}

/// Asserts `error` is a setup error for `reason`, naming `variable` when one
/// is given, whose prose is its message.
#[track_caller]
fn assert_setup_error(error: &Error, reason: &str, variable: Option<&str>) {
    assert_eq!(error.kind, ErrorKind::InvalidConfig, "{error:?}");
    assert_eq!(error.reason, Some(reason), "{error:?}");
    assert_eq!(error.name.as_deref(), variable, "{error:?}");
    assert_eq!(error.summary, error.message, "{error:?}");
}

#[test]
fn the_default_variable_names_are_the_runvil_ones() {
    assert_eq!(DEFAULT_URL_VAR, "RUNVIL_GATEWAY_URL");
    assert_eq!(DEFAULT_KEY_VAR, "RUNVIL_GATEWAY_API_KEY");
}

#[test]
fn from_env_surfaces_non_unicode_value_instead_of_dropping_it() {
    let err = from_lookup(DEFAULT_URL_VAR, DEFAULT_KEY_VAR, |name| {
        if name == "RUNVIL_GATEWAY_URL" {
            Err(crate::config::make_invalid_env_error(name))
        } else {
            Ok(Some("tok".to_owned()))
        }
    })
    .expect_err("a non-Unicode variable must be surfaced, not treated as missing");
    assert_setup_error(&err, "invalid_env", Some("RUNVIL_GATEWAY_URL"));
    assert_eq!(
        err.to_string(),
        "environment variable is set but not valid Unicode: RUNVIL_GATEWAY_URL"
    );
}

#[test]
fn from_env_missing_gateway_url() {
    let err = find_from(&[("RUNVIL_GATEWAY_API_KEY", "tok")]).expect_err("missing URL must fail");
    assert_eq!(
        err.to_string(),
        "missing environment variable: RUNVIL_GATEWAY_URL"
    );
    assert_setup_error(&err, "missing_env", Some("RUNVIL_GATEWAY_URL"));
}

#[test]
fn from_env_missing_gateway_key() {
    // A LAN gateway never trusts a keyless caller, so the key stays required
    // there; an empty value is the same as no value. Only the exact name
    // `localhost` is loopback: a name that merely contains it is not.
    for key_pairs in [
        vec![("RUNVIL_GATEWAY_URL", "http://192.168.1.20:8081/v1")],
        vec![
            ("RUNVIL_GATEWAY_URL", "http://192.168.1.20:8081/v1"),
            ("RUNVIL_GATEWAY_API_KEY", ""),
        ],
        vec![("RUNVIL_GATEWAY_URL", "https://gateway.example.com/v1")],
        vec![("RUNVIL_GATEWAY_URL", "http://localhost.evil.com:8081/v1")],
        vec![("RUNVIL_GATEWAY_URL", "http://notlocalhost:8081/v1")],
    ] {
        let err = find_from(&key_pairs)
            .expect_err("missing key against a non-loopback gateway must fail");
        assert_setup_error(&err, "missing_env", Some("RUNVIL_GATEWAY_API_KEY"));
    }
}

#[test]
fn from_env_missing_gateway_key_is_fine_for_a_loopback_gateway() {
    // A loopback gateway trusts keyless same-machine callers by default, so
    // the key is optional for every loopback spelling; the Gateway built has
    // no key, and the client it makes sends no bearer.
    for url in [
        "http://127.0.0.1:8081/v1",
        "http://127.0.0.2:8081/v1",
        "http://[::1]:8081/v1",
        "http://localhost:8081/v1",
        "http://LOCALHOST:8081/v1",
    ] {
        let gateway = find_from(&[("RUNVIL_GATEWAY_URL", url)])
            .unwrap_or_else(|err| panic!("a loopback URL needs no key, got {err:?} for {url}"));
        assert!(
            gateway.key().is_none(),
            "the Gateway built for {url} must leave the key unset"
        );
        assert!(!gateway.client().has_key());
        let empty_key = find_from(&[("RUNVIL_GATEWAY_URL", url), ("RUNVIL_GATEWAY_API_KEY", "")])
            .unwrap_or_else(|err| {
                panic!("an empty key on loopback is unset, got {err:?} for {url}")
            });
        assert!(empty_key.key().is_none());
    }
    let keyed = find_from(&[
        ("RUNVIL_GATEWAY_URL", "http://127.0.0.1:8081/v1"),
        ("RUNVIL_GATEWAY_API_KEY", "tok"),
    ])
    .expect("a loopback URL with a key builds");
    assert!(
        keyed.key().is_some(),
        "a key that is set is kept even on loopback"
    );
    assert!(keyed.client().has_key());
}

#[test]
fn the_names_the_caller_chooses_are_the_names_that_get_read() {
    let gateway = from_lookup(
        "MY_URL",
        "MY_KEY",
        lookup_from(&[
            ("MY_URL", "https://gateway.example.com/v1"),
            ("MY_KEY", "tok"),
            ("RUNVIL_GATEWAY_URL", "http://127.0.0.1:1/v1"),
        ]),
    )
    .expect("the named variables build a Gateway");
    assert_eq!(gateway.endpoint().url(), "https://gateway.example.com/v1");
    assert_eq!(gateway.key().map(SecretString::expose_secret), Some("tok"));

    let err = from_lookup(
        "MY_URL",
        "MY_KEY",
        lookup_from(&[("RUNVIL_GATEWAY_URL", "x")]),
    )
    .expect_err("the default names are not read");
    assert_setup_error(&err, "missing_env", Some("MY_URL"));
}

#[test]
fn find_env_named_reads_the_process_environment() {
    // No test sets a variable, so these names are unset in every run.
    let err = Gateway::find_env_named(
        "GATEWAY_CLIENT_TEST_UNSET_URL",
        "GATEWAY_CLIENT_TEST_UNSET_KEY",
    )
    .expect_err("an unset URL variable must fail");
    assert_setup_error(&err, "missing_env", Some("GATEWAY_CLIENT_TEST_UNSET_URL"));
}

#[test]
fn a_new_gateway_has_the_endpoint_and_no_key() {
    let endpoint = GatewayEndpoint::new("http://127.0.0.1:8081/v1").expect("valid endpoint");
    let gateway = Gateway::new(endpoint);
    assert_eq!(gateway.endpoint().url(), "http://127.0.0.1:8081/v1");
    assert!(gateway.key().is_none());
    assert!(!gateway.client().has_key());
}

#[test]
fn with_key_sets_the_key_and_the_client_sends_it() {
    let endpoint = GatewayEndpoint::new("http://127.0.0.1:8081/v1").expect("valid endpoint");
    let key = SecretString::new("tok").expect("non-empty key");
    let gateway = Gateway::new(endpoint).with_key(key);
    assert_eq!(gateway.key().map(SecretString::expose_secret), Some("tok"));
    assert!(gateway.client().has_key());
}

#[test]
fn the_debug_form_of_a_gateway_never_shows_its_key() {
    let endpoint = GatewayEndpoint::new("http://127.0.0.1:8081/v1").expect("valid endpoint");
    let key = SecretString::new("super-secret-token").expect("non-empty key");
    let rendered = format!("{:?}", Gateway::new(endpoint).with_key(key));
    assert!(!rendered.contains("super-secret-token"), "got: {rendered}");
    assert!(rendered.contains("<redacted>"), "got: {rendered}");
}

#[cfg(feature = "discovery")]
mod discovery {
    use super::*;

    /// The pinned copy of the Gateway's contract fixture.
    const FIXTURE: &str = include_str!("../tests/fixtures/gateway.json");

    /// The fixture as a discovery file that reports `version`.
    fn fixture_with_version(version: &str) -> DiscoveryFile {
        let mut file: DiscoveryFile = serde_json::from_str(FIXTURE).expect("the fixture parses");
        file.version = version.to_owned();
        file
    }

    /// A run directory that holds the fixture as `gateway.json`, with the pid
    /// of this test process.
    fn run_dir_with_fixture_of_this_process() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        let mut json: serde_json::Value =
            serde_json::from_str(FIXTURE).expect("the fixture is JSON");
        json["pid"] = serde_json::Value::from(std::process::id());
        std::fs::write(dir.path().join("gateway.json"), json.to_string()).expect("write the file");
        dir
    }

    #[test]
    fn a_live_file_names_the_gateway_at_its_v1_root_with_its_key() {
        let gateway = Gateway::from_file(fixture_with_version("0.3.1")).expect("a valid file");
        assert_eq!(gateway.endpoint().url(), "http://127.0.0.1:52814/v1");
        assert!(gateway.endpoint().is_loopback());
        assert_eq!(
            gateway.key().map(SecretString::expose_secret),
            Some("fixture-key")
        );
    }

    #[test]
    fn a_file_from_a_gateway_older_than_the_minimum_is_refused() {
        let error = Gateway::from_file(fixture_with_version("0.2.9")).expect_err("too old");
        assert_eq!(error.kind, ErrorKind::Unavailable);
        assert!(
            error.message.contains("the gateway is too old"),
            "{error:?}"
        );
    }

    #[test]
    fn a_file_with_a_version_that_is_not_three_numbers_is_refused() {
        let error = Gateway::from_file(fixture_with_version("nightly")).expect_err("no version");
        assert_eq!(error.kind, ErrorKind::Unavailable);
        assert!(error.message.contains("not major.minor.patch"), "{error:?}");
    }

    #[test]
    fn a_run_dir_with_no_file_says_no_gateway_is_running() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let error = Gateway::find_in(dir.path()).expect_err("no file means no gateway");
        assert_eq!(error.kind, ErrorKind::Unavailable);
        assert!(error.message.contains("no gateway is running"), "{error:?}");
        assert!(
            error.message.contains(&dir.path().display().to_string()),
            "the message names the directory: {error:?}"
        );
    }

    #[test]
    fn a_file_that_fails_validation_is_unavailable_and_names_the_file() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let mut json: serde_json::Value =
            serde_json::from_str(FIXTURE).expect("the fixture is JSON");
        json["port"] = serde_json::Value::from(0);
        std::fs::write(dir.path().join("gateway.json"), json.to_string()).expect("write the file");
        let error = Gateway::find_in(dir.path()).expect_err("port 0 is invalid");
        assert_eq!(error.kind, ErrorKind::Unavailable);
        assert!(error.message.contains("gateway.json"), "{error:?}");
        assert!(!error.message.contains("fixture-key"), "{error:?}");
    }

    #[test]
    fn a_file_for_a_process_that_is_not_a_gateway_is_stale_and_stays_on_disk() {
        // The pid is this test process, which is alive but is not a
        // `runvil-gateway`, so the liveness proof fails before any network
        // probe. The reader reports it and leaves the file alone.
        let dir = run_dir_with_fixture_of_this_process();
        let error = Gateway::find_in(dir.path()).expect_err("this process is not a gateway");
        assert_eq!(error.kind, ErrorKind::Unavailable);
        assert!(error.message.contains("is stale"), "{error:?}");
        assert!(!error.message.contains("fixture-key"), "{error:?}");
        assert!(
            dir.path().join("gateway.json").exists(),
            "the reader must never delete the discovery file"
        );
    }
}
