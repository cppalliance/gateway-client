//! Liveness: proving that a `gateway.json` names a live, authorized Gateway.
//!
//! A file is live when one OS process boot with a `runvil-gateway` image is
//! unchanged across a same-socket health and bearer proof, and the file
//! includes a boot identity. Anything else is stale: the process that wrote
//! the file may have died, and its pid may now belong to another program.
//! The proof is a point in time. It does not say the Gateway stays up after
//! it returns. Nothing here deletes the file.

use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, Instant};

use super::file::DiscoveryFile;
use super::health::{self, ConnectionProbe};
use super::sys::{ProcessIdentity, process_identity};

/// The image file name a live Gateway process must have.
#[cfg(windows)]
const GATEWAY_IMAGE_NAME: &str = "runvil-gateway.exe";
/// The image file name a live Gateway process must have.
#[cfg(not(windows))]
const GATEWAY_IMAGE_NAME: &str = "runvil-gateway";

/// The bearer-gated route used to prove the presented key is accepted.
const KEY_PROBE_PATH: &str = "/v1/models";

/// Budget for proving health without rejecting one transient failure.
const LIVENESS_BUDGET: Duration = Duration::from_secs(2);

/// Why a gateway discovery file was judged stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StaleReason {
    /// The file failed the static checks on its fields.
    #[error("the gateway discovery file is invalid")]
    Invalid,
    /// The pid is dead.
    #[error("the recorded gateway process is dead")]
    ProcessDead,
    /// The pid is alive but its image is not a `runvil-gateway`
    /// binary (a reused pid).
    #[error("the recorded pid belongs to another process image")]
    ImageMismatch,
    /// The gateway discovery file has no usable boot identity.
    #[error("the gateway discovery file has no usable boot identity")]
    BootIdentityInvalid,
    /// The pid changed process boot while validation was in progress.
    #[error("the recorded process identity changed during validation")]
    ProcessChanged,
    /// The health endpoint did not answer 200.
    #[error("the recorded gateway does not answer its health probe")]
    HealthFailed,
    /// The bearer key was rejected.
    #[error("the gateway discovery file bearer was rejected")]
    KeyRejected,
}

/// Proves that `file` names a live, authorized Gateway, and gives the file
/// back.
///
/// The proof checks the process image, the boot identity, the health
/// endpoint, and bearer acceptance. It observes the OS process boot
/// immediately before and after the one TCP connection that makes both
/// network checks. That closes the gap between the health answer and the
/// bearer answer, and it rejects pid reuse during that interval.
///
/// # Errors
/// Returns the first `StaleReason` that prevents the file from proving a
/// live, authorized Gateway boot.
pub(crate) fn prove_live(file: DiscoveryFile) -> Result<DiscoveryFile, StaleReason> {
    prove_live_named(file, GATEWAY_IMAGE_NAME)
}

/// [`prove_live`] against a caller-named process image, so tests can run the
/// full proof from a test binary, which is never named `runvil-gateway`.
pub(crate) fn prove_live_named(
    file: DiscoveryFile,
    image_name: &str,
) -> Result<DiscoveryFile, StaleReason> {
    prove_live_before(file, image_name, Instant::now() + LIVENESS_BUDGET)
}

/// The proof, with the network step bounded by an absolute `deadline`.
fn prove_live_before(
    file: DiscoveryFile,
    image_name: &str,
    deadline: Instant,
) -> Result<DiscoveryFile, StaleReason> {
    prove_before_with(
        file,
        image_name,
        deadline,
        process_identity,
        |address, bearer, deadline| {
            health::probe_connection_until(address, KEY_PROBE_PATH, bearer, deadline)
        },
    )
}

/// Performs one proof with the default budget, and lets unit tests observe
/// the process and the network deterministically.
#[cfg(test)]
fn prove_with(
    file: DiscoveryFile,
    image_name: &str,
    observe_process: impl FnMut(u32) -> Option<ProcessIdentity>,
    prove_connection: impl FnOnce(&str, &str, Instant) -> ConnectionProbe,
) -> Result<DiscoveryFile, StaleReason> {
    prove_before_with(
        file,
        image_name,
        Instant::now() + LIVENESS_BUDGET,
        observe_process,
        prove_connection,
    )
}

/// Production passes only the platform process observer and the shared
/// connection probe.
fn prove_before_with(
    file: DiscoveryFile,
    image_name: &str,
    deadline: Instant,
    mut observe_process: impl FnMut(u32) -> Option<ProcessIdentity>,
    prove_connection: impl FnOnce(&str, &str, Instant) -> ConnectionProbe,
) -> Result<DiscoveryFile, StaleReason> {
    if Instant::now() >= deadline {
        return Err(StaleReason::HealthFailed);
    }
    if file.validation_error().is_some() {
        return Err(StaleReason::Invalid);
    }
    let Some(before) = observe_process(file.pid) else {
        return Err(StaleReason::ProcessDead);
    };
    if !image_name_matches(&before.image, image_name) {
        return Err(StaleReason::ImageMismatch);
    }
    if !file.has_boot_identity() {
        return Err(StaleReason::BootIdentityInvalid);
    }
    if Instant::now() >= deadline {
        return Err(StaleReason::HealthFailed);
    }
    let address = format!("127.0.0.1:{}", file.port);
    match prove_connection(&address, &file.api_key, deadline) {
        ConnectionProbe::HealthFailed => return Err(StaleReason::HealthFailed),
        ConnectionProbe::KeyRejected => return Err(StaleReason::KeyRejected),
        ConnectionProbe::Accepted => {}
    }
    let Some(after) = observe_process(file.pid) else {
        return Err(StaleReason::ProcessChanged);
    };
    if before != after {
        return Err(StaleReason::ProcessChanged);
    }
    Ok(file)
}

fn image_name_matches(image: &Path, expected: &str) -> bool {
    let Some(name) = image.file_name() else {
        return false;
    };
    image_file_name_matches(name, expected)
}

#[cfg(windows)]
fn image_file_name_matches(name: &OsStr, expected: &str) -> bool {
    name.to_string_lossy().eq_ignore_ascii_case(expected)
}

#[cfg(not(windows))]
fn image_file_name_matches(name: &OsStr, expected: &str) -> bool {
    name == OsStr::new(expected)
}

#[cfg(test)]
#[path = "liveness-tests.rs"]
mod tests;
