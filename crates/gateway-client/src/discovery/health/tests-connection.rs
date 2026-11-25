//! Unit tests for the one-connection health and bearer proof.

use super::*;

use super::super::connection::{ConnectionAttempt, probe_connection_once};

/// The 100 ms attempt budget that these tests give one probe attempt, so a
/// stalled attempt ends quickly.
const SHORT_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(100);

#[test]
fn connection_proof_cannot_mix_health_and_bearer_across_endpoints() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind replacement fixture");
    let address = listener
        .local_addr()
        .expect("replacement fixture address")
        .to_string();
    let (accepted, received) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut health, _) = listener.accept().expect("accept health probe");
        let mut buffer = [0_u8; 1024];
        let read = health.read(&mut buffer).expect("read health probe");
        assert!(
            String::from_utf8_lossy(&buffer[..read]).starts_with("GET /health "),
            "the first endpoint receives health"
        );
        health
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
            .expect("answer health");
        drop(health);

        listener
            .set_nonblocking(true)
            .expect("make replacement observation bounded");
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut connection_count = 1;
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut bearer, _)) => {
                    connection_count += 1;
                    let read = bearer.read(&mut buffer).expect("read bearer probe");
                    assert!(
                        String::from_utf8_lossy(&buffer[..read])
                            .contains("Authorization: Bearer accepted\r\n"),
                        "the replacement endpoint accepts the bearer"
                    );
                    bearer
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                        .expect("answer bearer");
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("observe replacement connection: {error}"),
            }
        }
        accepted
            .send(connection_count)
            .expect("report accepted connections");
    });

    assert_eq!(
        probe_connection(
            &address,
            "/v1/models",
            "accepted",
            Duration::from_millis(250)
        ),
        ConnectionProbe::HealthFailed,
        "one capability cannot combine health from one socket with authority from another"
    );
    assert_eq!(
        received
            .recv_timeout(Duration::from_secs(2))
            .expect("fixture reports its connection count"),
        1,
        "validation never reconnects after health succeeds"
    );
}

#[test]
fn a_real_connect_attempt_obeys_one_absolute_deadline() {
    let started = Instant::now();
    let result = probe_connection_once(
        "192.0.2.1:9",
        "/v1/models",
        "key",
        Instant::now() + Duration::from_millis(100),
    );

    assert_eq!(result, ConnectionAttempt::HealthFailed);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "connect_timeout bounds a stalled or unreachable route"
    );
}

#[test]
fn slow_drip_response_head_cannot_refresh_the_attempt_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind slow-head fixture");
    let address = listener
        .local_addr()
        .expect("slow-head address")
        .to_string();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept slow-head probe");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        for byte in b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}" {
            if stream.write_all(&[*byte]).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });

    let started = Instant::now();
    let result = probe_connection_once(
        &address,
        "/v1/models",
        "key",
        Instant::now() + SHORT_ATTEMPT_TIMEOUT,
    );

    assert_eq!(result, ConnectionAttempt::HealthFailed);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "one absolute deadline bounds a slow-drip response head"
    );
}

#[test]
fn slow_drip_response_body_cannot_refresh_the_attempt_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind slow-body fixture");
    let address = listener
        .local_addr()
        .expect("slow-body address")
        .to_string();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept slow-body probe");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        if stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 16\r\n\r\n")
            .is_err()
        {
            return;
        }
        for byte in b"0123456789abcdef" {
            if stream.write_all(&[*byte]).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });

    let started = Instant::now();
    let result = probe_connection_once(
        &address,
        "/v1/models",
        "key",
        Instant::now() + SHORT_ATTEMPT_TIMEOUT,
    );

    assert_eq!(result, ConnectionAttempt::HealthFailed);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "one absolute deadline bounds a slow-drip response body"
    );
}

#[test]
fn an_unbounded_declared_body_is_rejected_without_draining() {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
        RESPONSE_BODY_LIMIT + 1
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind oversized-body fixture");
    let address = listener
        .local_addr()
        .expect("oversized-body address")
        .to_string();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept oversized-body probe");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        let _ = stream.write_all(response.as_bytes());
        std::thread::sleep(Duration::from_secs(1));
    });

    let started = Instant::now();
    let result = probe_connection_once(
        &address,
        "/v1/models",
        "key",
        Instant::now() + SHORT_ATTEMPT_TIMEOUT,
    );

    assert_eq!(result, ConnectionAttempt::HealthFailed);
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "an oversized declared body is rejected before its bytes arrive"
    );
}
