//! Self-contained liveness probe, so the runtime image needs no shell or HTTP client.
//!
//! `pairdrop-rs --healthcheck` asks the server already listening in this container for `/config`
//! and exits 0 only on a 200.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

pub fn run(port: u16) -> std::process::ExitCode {
    match probe(port) {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(err) => {
            eprintln!("healthcheck: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn probe(port: u16) -> std::io::Result<bool> {
    let address = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect(&address)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;

    stream.write_all(b"GET /config HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    stream.flush()?;

    // Deliberately no half-close: hyper treats a write shutdown before the response as an
    // aborted connection and sends nothing back.
    //
    // The status line is all we need, and it arrives in the first packet.
    let mut head = [0u8; 64];
    let read = stream.read(&mut head)?;

    Ok(head[..read].starts_with(b"HTTP/1.1 200"))
}
