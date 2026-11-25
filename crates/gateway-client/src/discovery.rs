//! Finding a Gateway on this machine through the `gateway.json` it writes.
//!
//! The Gateway writes `gateway.json` into `<home>/.runvil/run` after it binds.
//! The file holds the loopback port, the bearer key, the process id, the boot
//! epoch, the version, and the start time. This module only reads it. It
//! trusts the file after one proof: the process is alive, it runs the Gateway
//! program, and its health endpoint and a key-gated route answer on one
//! socket. Writing the file, launching a Gateway, and stopping one belong to
//! the Gateway and to the programs that launch it.
//!
//! The file is the contract between a Gateway and the readers of it. The
//! Gateway repository holds the fixture that fixes its fields, and this crate
//! keeps a copy of that fixture to read in its tests.

use std::path::{Path, PathBuf};

use crate::error::{Error, ErrorKind};

mod error;
mod file;
mod health;
mod liveness;
mod paths;
mod sys;

pub(crate) use file::DiscoveryFile;

/// The run directory a Gateway writes `gateway.json` into: `<home>/.runvil/run`.
pub(crate) fn default_run_dir() -> Option<PathBuf> {
    paths::default_run_dir()
}

/// Reads `gateway.json` in `run_dir` and proves it names a live Gateway.
///
/// `Ok(None)` means there is no file. A file that fails validation, or that
/// names a Gateway that is not live, is an error that says why. The file is
/// never deleted: only the process that launches a Gateway may remove it, and
/// this reader may be racing the Gateway's own write.
pub(crate) fn read_live(run_dir: &Path) -> Result<Option<DiscoveryFile>, Error> {
    let file = match DiscoveryFile::read(run_dir) {
        Ok(Some(file)) => file,
        Ok(None) => return Ok(None),
        Err(error) => return Err(Error::specific(ErrorKind::Unavailable, error)),
    };
    match liveness::prove_live(file) {
        Ok(file) => Ok(Some(file)),
        Err(reason) => Err(Error::specific(
            ErrorKind::Unavailable,
            format!("the gateway discovery file is stale: {reason}"),
        )),
    }
}

/// The oldest Gateway this client works with, as (major, minor, patch). It is
/// the first Gateway release, 0.3.0, whose `gateway.json` already has all six
/// fields.
const MIN_GATEWAY_VERSION: (u64, u64, u64) = (0, 3, 0);

/// Refuses a Gateway older than [`MIN_GATEWAY_VERSION`], and a version that is
/// not `major.minor.patch`.
pub(crate) fn check_version(version: &str) -> Result<(), Error> {
    let Some(found) = parse_version(version) else {
        return Err(Error::specific(
            ErrorKind::Unavailable,
            format!("the gateway reports version {version:?}, which is not major.minor.patch"),
        ));
    };
    if found < MIN_GATEWAY_VERSION {
        let (major, minor, patch) = MIN_GATEWAY_VERSION;
        return Err(Error::specific(
            ErrorKind::Unavailable,
            format!(
                "the gateway is too old: it is {version}, and this client needs \
                 {major}.{minor}.{patch} or newer"
            ),
        ));
    }
    Ok(())
}

/// Reads `major.minor.patch`, ignoring a pre-release or build suffix.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_reads_as_major_minor_patch() {
        assert_eq!(parse_version("0.3.1"), Some((0, 3, 1)));
        assert_eq!(parse_version("1.22.333"), Some((1, 22, 333)));
        assert_eq!(parse_version("0.3.1-rc.1"), Some((0, 3, 1)));
        assert_eq!(parse_version("0.3.1+build.7"), Some((0, 3, 1)));
    }

    #[test]
    fn text_that_is_not_a_version_does_not_read() {
        for text in ["", "0.3", "0.3.1.2", "a.b.c", "0.3.x", "v0.3.1"] {
            assert_eq!(parse_version(text), None, "{text:?}");
        }
    }

    #[test]
    fn the_minimum_version_and_newer_pass() {
        for version in ["0.3.0", "0.3.1", "0.4.0", "1.0.0", "0.3.0-rc.1"] {
            assert!(check_version(version).is_ok(), "{version}");
        }
    }

    #[test]
    fn an_older_gateway_is_refused_with_the_minimum_named() {
        let error = check_version("0.2.9").expect_err("0.2.9 is too old");
        assert_eq!(error.kind, ErrorKind::Unavailable);
        assert!(
            error
                .summary
                .contains("the gateway is too old: it is 0.2.9")
        );
        assert!(error.summary.ends_with("needs 0.3.0 or newer"));
    }

    #[test]
    fn a_version_that_is_not_three_numbers_is_refused() {
        let error = check_version("nightly").expect_err("not a version");
        assert_eq!(error.kind, ErrorKind::Unavailable);
        assert!(error.summary.contains("not major.minor.patch"));
    }
}
