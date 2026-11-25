//! Unit tests for the Host header the probe sends.

use super::*;

use std::io::{Read, Write as _};
use std::net::TcpListener;
use std::sync::mpsc;

#[path = "tests-connection.rs"]
mod connection;

#[test]
fn the_probe_sends_the_bound_address_as_host() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind recording server");
    let address = listener.local_addr().expect("recording server address");
    let (requests, received) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            for _ in 0..2 {
                let mut buffer = [0u8; 1024];
                let Ok(read) = stream.read(&mut buffer) else {
                    break;
                };
                let _ = requests.send(String::from_utf8_lossy(&buffer[..read]).into_owned());
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
            }
        }
    });
    assert_eq!(
        probe_connection(
            &address.to_string(),
            "/v1/models",
            "key",
            Duration::from_secs(5)
        ),
        ConnectionProbe::Accepted,
        "the stub answers 200 to both requests"
    );
    for _ in 0..2 {
        let request = received
            .recv_timeout(Duration::from_secs(5))
            .expect("the probe's request arrived");
        assert!(
            request.contains(&format!("Host: {address}\r\n")),
            "the Host header is the bound address, not localhost: {request:?}"
        );
    }
}
