//! The TLS server against OpenSSL's and curl's clients over a real TCP socket on the host.

use hardroxide_tls::{Config, TlsServer};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

/// A self-signed Ed25519 certificate made by OpenSSL: (DER certificate, 32-byte seed).
fn identity() -> &'static (Vec<u8>, [u8; 32]) {
    static ID: OnceLock<(Vec<u8>, [u8; 32])> = OnceLock::new();
    ID.get_or_init(|| {
        let dir: PathBuf =
            std::env::temp_dir().join(format!("hrx-tls-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (key, cert) = (dir.join("key.pem"), dir.join("cert.pem"));
        let st = Command::new("openssl")
            .args(["req", "-x509", "-newkey", "ed25519", "-nodes", "-keyout"])
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .args(["-subj", "/CN=hardroxide.local", "-days", "3650"])
            .output()
            .expect("openssl must be installed to run this test");
        assert!(
            st.status.success(),
            "{}",
            String::from_utf8_lossy(&st.stderr)
        );
        let der = Command::new("openssl")
            .args(["x509", "-outform", "DER", "-in"])
            .arg(&cert)
            .output()
            .unwrap()
            .stdout;
        let pkcs8 = Command::new("openssl")
            .args(["pkey", "-outform", "DER", "-in"])
            .arg(&key)
            .output()
            .unwrap()
            .stdout;
        // PKCS#8 of an Ed25519 key: a 16-byte header, then the 32-byte seed.
        assert_eq!(pkcs8.len(), 48);
        let seed: [u8; 32] = pkcs8[16..].try_into().unwrap();
        std::fs::remove_dir_all(&dir).ok();
        (der, seed)
    })
}

fn random64() -> [u8; 64] {
    let mut r = [0u8; 64];
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut r)
        .unwrap();
    r
}

/// Serve one connection with our TLS engine: read a request, answer with `body`, close.
fn serve_one(
    listener: &TcpListener,
    body: &'static [u8],
) -> std::thread::JoinHandle<Result<(), String>> {
    let listener = listener.try_clone().unwrap();
    std::thread::spawn(move || {
        let (cert, seed) = identity();
        let config = Config::new(cert, *seed);
        let (mut sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut tls = TlsServer::new(&config, random64());
        let mut request = Vec::new();
        let mut responded = false;
        let t0 = std::time::Instant::now();
        let mut buf = [0u8; 2048];
        while t0.elapsed() < Duration::from_secs(20) {
            match sock.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut off = 0;
                    while off < n {
                        let took = tls.feed(&buf[off..n]);
                        if took == 0 {
                            break;
                        }
                        off += took;
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
            if let Some(e) = tls.failure() {
                // Let the alert go out, then report.
                sock.write_all(tls.pending_output()).ok();
                return Err(format!("{e:?}"));
            }
            let n = tls.recv(&mut buf);
            request.extend_from_slice(&buf[..n]);
            if tls.is_connected() && !responded && request.windows(4).any(|w| w == b"\r\n\r\n") {
                responded = true;
                let head = format!(
                    "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n",
                    body.len()
                );
                let mut all = head.into_bytes();
                all.extend_from_slice(body);
                let mut off = 0;
                while off < all.len() {
                    let took = tls.send(&all[off..]);
                    off += took;
                    if took == 0 {
                        sock.write_all(tls.pending_output()).unwrap();
                        let n = tls.pending_output().len();
                        tls.consume_output(n);
                    }
                }
                tls.close();
            }
            let out = tls.pending_output().to_vec();
            if !out.is_empty() {
                sock.write_all(&out).unwrap();
                tls.consume_output(out.len());
            }
            if responded && tls.pending_output().is_empty() && tls.peer_closed() {
                break;
            }
            if responded && t0.elapsed() > Duration::from_secs(8) {
                break;
            }
        }
        Ok(())
    })
}

#[test]
fn openssl_s_client_completes_a_tls13_handshake_and_gets_a_page() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = serve_one(&listener, b"hello from hardroxide over TLS 1.3\n");
    let mut client = Command::new("openssl")
        .args([
            "s_client",
            "-connect",
            &format!("127.0.0.1:{port}"),
            "-tls1_3",
            "-ciphersuites",
            "TLS_CHACHA20_POLY1305_SHA256",
            "-brief",
            "-ign_eof",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    client
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"GET / HTTP/1.0\r\n\r\n")
        .unwrap();
    let out = client.wait_with_output().unwrap();
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let r = server.join().unwrap();
    assert!(
        r.is_ok(),
        "server: {r:?}\nclient stderr: {stderr}\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("hello from hardroxide over TLS 1.3"),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stderr.contains("TLSv1.3"), "{stderr}");
    assert!(stderr.contains("TLS_CHACHA20_POLY1305_SHA256"), "{stderr}");
    assert!(
        stderr.contains("ED25519") || stderr.contains("Ed25519") || stderr.contains("ed25519"),
        "{stderr}"
    );
}

#[test]
fn curl_fetches_a_page_over_https() {
    if Command::new("curl").arg("--version").output().is_err() {
        eprintln!("curl not installed; skipping");
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = serve_one(&listener, b"curl was here\n");
    let out = Command::new("curl")
        .args([
            "-s",
            "-k",
            "-m",
            "20",
            "--tlsv1.3",
            "-w",
            "\n%{http_code} %{ssl_verify_result}",
            &format!("https://127.0.0.1:{port}/"),
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let r = server.join().unwrap();
    assert!(
        r.is_ok(),
        "server: {r:?}; curl said {text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.starts_with("curl was here\n"),
        "{text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("200"), "{text}");
}

#[test]
fn a_client_without_chacha_or_ed25519_is_refused_with_an_alert() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = serve_one(&listener, b"x");
    // Only AES-GCM offered: the server must say handshake_failure.
    let mut client = Command::new("openssl")
        .args([
            "s_client",
            "-connect",
            &format!("127.0.0.1:{port}"),
            "-tls1_3",
            "-ciphersuites",
            "TLS_AES_128_GCM_SHA256",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(client.stdin.take());
    let out = client.wait_with_output().unwrap();
    let stderr =
        String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    let r = server.join().unwrap();
    let e = r.expect_err("the handshake must fail");
    assert!(e.contains("HandshakeFailure"), "{e}");
    assert!(
        stderr.contains("handshake failure") || stderr.contains("alert"),
        "{stderr}"
    );
}

#[test]
fn garbage_and_tampered_records_fail_cleanly() {
    let (cert, seed) = identity();
    let config = Config::new(cert, *seed);
    // Noise as a first record.
    let mut tls = TlsServer::new(&config, random64());
    tls.feed(&[0x17, 3, 3, 0, 5, 1, 2, 3, 4, 5]);
    assert!(tls.failure().is_some());
    // A record claiming to be enormous.
    let mut tls = TlsServer::new(&config, random64());
    tls.feed(&[0x16, 3, 1, 0xFF, 0xFF]);
    assert_eq!(
        tls.failure().unwrap().alert,
        hardroxide_tls::Alert::RecordOverflow
    );
    // 20,000 random-ish bytes never panic.
    let mut x = 0x1234_5678u32;
    for round in 0..200 {
        let mut tls = TlsServer::new(&config, random64());
        let mut data = vec![0u8; 300];
        for b in data.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
        if round % 2 == 0 {
            data[0] = 0x16;
            data[1] = 3;
            data[2] = 1;
            data[3] = 0;
            data[4] = 200;
        }
        let mut off = 0;
        while off < data.len() {
            let n = tls.feed(&data[off..]);
            if n == 0 {
                break;
            }
            off += n;
        }
    }
}
