//! Unit tests for proving that a gateway discovery file names a live Gateway.

use super::*;

use std::io::{Read, Write as _};
use std::net::TcpListener;

/// The test process's own image name, so the pid and image checks
/// pass and the test reaches the probe under test.
fn own_image_name() -> String {
    std::env::current_exe()
        .expect("current exe")
        .file_name()
        .expect("the exe has a file name")
        .to_string_lossy()
        .into_owned()
}

fn connection(port: u16, api_key: &str) -> DiscoveryFile {
    DiscoveryFile {
        port,
        api_key: api_key.to_owned(),
        pid: std::process::id(),
        epoch: 1_778_000_000,
        version: "0.2.0".to_owned(),
        started_at: "2026-05-05T12:00:00Z".to_owned(),
    }
}

/// A gateway discovery file pointing at the test process itself.
fn live_file(port: u16, api_key: &str) -> DiscoveryFile {
    DiscoveryFile {
        port,
        api_key: api_key.to_owned(),
        pid: std::process::id(),
        epoch: 1_757_000_000,
        version: "0.2.0".to_owned(),
        started_at: "2026-09-03T12:00:00Z".to_owned(),
    }
}

/// A pid guaranteed dead: a short-lived child, reaped and dropped so
/// no handle keeps the process object alive.
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new(std::env::current_exe().expect("current exe"))
        .arg("--list")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn a short-lived child");
    let pid = child.id();
    child.wait().expect("the child exits");
    drop(child);
    pid
}

/// A fixture gateway: answers `GET /health` with 200 and the key
/// probe with 200 only when the bearer matches `expected_key`.
fn fixture_gateway(expected_key: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let port = listener.local_addr().expect("fixture address").port();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            for _ in 0..2 {
                let mut buffer = [0_u8; 1024];
                let Ok(read) = stream.read(&mut buffer) else {
                    break;
                };
                let request = String::from_utf8_lossy(&buffer[..read]);
                let accepted = request.starts_with("GET /health ")
                    || request.contains(&format!("Authorization: Bearer {expected_key}\r\n"));
                let response = if accepted {
                    &b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"[..]
                } else {
                    &b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n"[..]
                };
                if stream.write_all(response).is_err() {
                    break;
                }
            }
        }
    });
    port
}

#[test]
fn a_wrong_process_image_does_not_prove_live() {
    let file = connection(1, "key");

    assert_eq!(
        prove_live_named(file, "not-the-test-binary"),
        Err(StaleReason::ImageMismatch)
    );
}

#[test]
fn a_dead_process_does_not_prove_live() {
    let file = DiscoveryFile {
        pid: dead_pid(),
        ..connection(1, "key")
    };

    assert_eq!(
        prove_live_named(file, &own_image_name()),
        Err(StaleReason::ProcessDead)
    );
}

#[test]
fn a_stale_boot_identity_does_not_prove_live() {
    let missing_epoch = DiscoveryFile {
        epoch: 0,
        ..connection(1, "key")
    };
    assert_eq!(
        prove_live_named(missing_epoch, &own_image_name()),
        Err(StaleReason::BootIdentityInvalid)
    );

    let missing_start = DiscoveryFile {
        started_at: String::new(),
        ..connection(1, "key")
    };
    assert_eq!(
        prove_live_named(missing_start, &own_image_name()),
        Err(StaleReason::BootIdentityInvalid)
    );
}

#[test]
fn an_invalid_raw_connection_does_not_prove_live() {
    let file = DiscoveryFile {
        port: 0,
        ..connection(1, "key")
    };

    assert_eq!(
        prove_live_named(file, &own_image_name()),
        Err(StaleReason::Invalid)
    );
}

#[test]
fn failed_health_does_not_prove_live() {
    let file = connection(1, "key");

    assert_eq!(
        prove_live_named(file, &own_image_name()),
        Err(StaleReason::HealthFailed)
    );
}

#[test]
fn an_absolute_validation_deadline_bounds_a_slow_owner() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind slow owner");
    let port = listener.local_addr().expect("slow owner address").port();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept validation");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        std::thread::sleep(Duration::from_secs(1));
    });
    let timeout = Duration::from_millis(100);
    let started = Instant::now();

    let result = prove_live_before(
        connection(port, "key"),
        &own_image_name(),
        started + timeout,
    );

    assert_eq!(result, Err(StaleReason::HealthFailed));
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "the caller's deadline caps the real network proof"
    );
}

#[test]
fn a_rejected_bearer_does_not_prove_live() {
    let port = fixture_gateway("accepted");
    let file = connection(port, "rejected");

    assert_eq!(
        prove_live_named(file, &own_image_name()),
        Err(StaleReason::KeyRejected)
    );
}

#[test]
fn debug_output_never_contains_the_bearer() {
    let secret = "capability-secret";
    let port = fixture_gateway(secret);
    let mut raw = connection(port, secret);
    raw.version = format!("version-{secret}\r\n");
    raw.started_at = format!("started-{secret}\t");
    let proven = prove_live_named(raw, &own_image_name()).expect("the connection validates");

    let debug = format!("{proven:?}");
    assert!(!debug.contains(secret), "debug output redacts the bearer");
    assert!(
        !debug.contains(['\r', '\n', '\t']),
        "untrusted metadata cannot inject debug output"
    );
    assert!(debug.contains(&port.to_string()), "the endpoint is visible");
}

#[test]
fn a_process_boot_change_during_the_network_proof_is_rejected() {
    let image = std::path::PathBuf::from(own_image_name());
    let before = ProcessIdentity::for_test(image.clone(), 41);
    let after = ProcessIdentity::for_test(image, 42);
    let mut observations = [Some(before), Some(after)].into_iter();

    assert_eq!(
        prove_with(
            connection(8081, "key"),
            &own_image_name(),
            |_| observations.next().flatten(),
            |_, _, _| ConnectionProbe::Accepted,
        ),
        Err(StaleReason::ProcessChanged),
        "a reused pid cannot complete a mixed proof"
    );
}

#[test]
fn validation_errors_never_contain_the_bearer_or_metadata() {
    let secret = "capability-secret";
    let mut raw = connection(8081, secret);
    raw.version = format!("version-{secret}\r\n");
    raw.started_at = format!("started-{secret}\t");
    let image = std::path::PathBuf::from(own_image_name());
    let identity = ProcessIdentity::for_test(image, 41);
    let error = prove_with(
        raw,
        &own_image_name(),
        |_| Some(identity.clone()),
        |_, _, _| ConnectionProbe::KeyRejected,
    )
    .expect_err("the bearer is rejected");

    let debug = format!("{error:?}");
    let display = format!("{error}");
    for rendered in [debug, display] {
        assert!(!rendered.contains(secret), "errors redact the bearer");
        assert!(
            !rendered.contains(['\r', '\n', '\t']),
            "untrusted metadata cannot inject errors"
        );
    }
}

#[test]
fn an_alive_pid_with_a_foreign_image_is_stale() {
    // The test binary is never named runvil-gateway, so the real
    // image name must reject the test process's own pid.
    assert_eq!(
        prove_live(live_file(1, "key")),
        Err(StaleReason::ImageMismatch)
    );
}

#[test]
fn a_transiently_silent_health_endpoint_is_not_stale() {
    // The first probe connection is hung up without a response; only
    // a retried probe learns the gateway is live.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let port = listener.local_addr().expect("fixture address").port();
    std::thread::spawn(move || {
        let mut first = true;
        while let Ok((mut stream, _)) = listener.accept() {
            if std::mem::take(&mut first) {
                drop(stream);
                continue;
            }
            for _ in 0..2 {
                let mut buffer = [0u8; 1024];
                if stream.read(&mut buffer).is_err() {
                    break;
                }
                if stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                    .is_err()
                {
                    break;
                }
            }
        }
    });
    let file = live_file(port, "key");

    assert_eq!(
        prove_live_named(file.clone(), &own_image_name()),
        Ok(file),
        "one dropped probe must not reject a live gateway"
    );
}

#[test]
fn a_fully_live_file_proves_live() {
    let port = fixture_gateway("right");
    let file = live_file(port, "right");

    assert_eq!(prove_live_named(file.clone(), &own_image_name()), Ok(file));
}
