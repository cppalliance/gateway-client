//! Run-directory and gateway-discovery-file paths: the one place that knows
//! the `<home>/.runvil/run` layout.

use std::path::{Path, PathBuf};

/// The gateway discovery file's name inside the run directory.
pub(crate) const GATEWAY_DISCOVERY_FILE_NAME: &str = "gateway.json";

/// The run directory under the state dir: `<home>/.runvil/run`.
#[must_use]
pub(crate) fn run_dir(home: &Path) -> PathBuf {
    home.join(".runvil").join("run")
}

/// This process's default run directory: the user profile's
/// `.runvil/run`.
///
/// Returns `None` when the user profile directory cannot be located.
#[must_use]
pub(crate) fn default_run_dir() -> Option<PathBuf> {
    std::env::home_dir().map(|home| run_dir(&home))
}

/// The gateway discovery file inside `run_dir`.
#[must_use]
pub(crate) fn gateway_discovery_file_path(run_dir: &Path) -> PathBuf {
    run_dir.join(GATEWAY_DISCOVERY_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_dir_sits_under_the_profile_runvil_dir() {
        assert_eq!(
            run_dir(Path::new("home")),
            Path::new("home").join(".runvil").join("run")
        );
    }

    #[test]
    fn the_gateway_discovery_file_sits_in_the_run_dir() {
        let dir = Path::new("run");
        assert_eq!(
            gateway_discovery_file_path(dir),
            Path::new("run").join("gateway.json")
        );
    }
}
