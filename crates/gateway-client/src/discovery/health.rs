//! The health and key probe: raw loopback HTTP/1.x over
//! `std::net::TcpStream`, enough to read a status line, with no HTTP
//! client dependency.
//!
//! The `Host` header is the bound loopback address, matching the Gateway's
//! loopback `Host` allowlist.

use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

mod connection;

#[cfg(test)]
use connection::probe_connection;
pub(crate) use connection::probe_connection_until;

/// Delay between probes while the server comes up.
const RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// Per-attempt connect and read timeout, so one hung attempt cannot eat
/// the whole budget.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum accepted response head for the two-request validation proof.
const RESPONSE_HEAD_LIMIT: usize = 16 * 1024;
/// Maximum body drained between the health and bearer responses.
const RESPONSE_BODY_LIMIT: usize = 64 * 1024;

/// One probe attempt failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ProbeError {
    /// The connect, write, or read failed.
    #[error("{operation}")]
    Io {
        /// The failed operation.
        operation: &'static str,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The status line was not a 200.
    #[error("unexpected health response: {status_line}")]
    UnexpectedStatus {
        /// The response's first line.
        status_line: String,
    },
}

/// The combined readiness and authority proof for one connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionProbe {
    /// Health answered and the bearer was accepted.
    Accepted,
    /// Health failed, including a bearer probe that became unreachable.
    HealthFailed,
    /// Health answered but the bearer was rejected.
    KeyRejected,
}

/// Connects within the one absolute attempt deadline.
fn connect_until(address: &SocketAddr, deadline: Instant) -> Result<TcpStream, ProbeError> {
    let timeout = remaining(deadline)?;
    TcpStream::connect_timeout(address, timeout).map_err(|source| ProbeError::Io {
        operation: "connect",
        source,
    })
}

/// Applies the remaining absolute attempt budget to reads and writes.
fn configure_stream(stream: &TcpStream, deadline: Instant) -> Result<(), ProbeError> {
    let timeout = remaining(deadline)?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|source| ProbeError::Io {
            operation: "configure the read timeout",
            source,
        })?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|source| ProbeError::Io {
            operation: "configure the write timeout",
            source,
        })
}

fn remaining(deadline: Instant) -> Result<Duration, ProbeError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| ProbeError::Io {
            operation: "validation deadline elapsed",
            source: std::io::Error::from(std::io::ErrorKind::TimedOut),
        })
}

#[cfg(test)]
mod tests;
