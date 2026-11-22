//! `hardroxide make-cert` against OpenSSL and curl: the certificates parse, verify, and are trusted by a real client for the right names.

use hardroxide_tls::{Config, TlsServer};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn make(dir: &Path, extra: &[&str]) -> (Vec<u8>, [u8; 32]) {
    let out = Command::new(env!("CARGO_BIN_EXE_hardroxide"))
        .arg("make-cert")
        .args(extra)
        .arg("--out")
        .arg(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("SHA-256 fingerprint:"), "{text}");
    (
        std::fs::read(dir.join("cert.der")).unwrap(),
        std::fs::read(dir.join("key.bin"))
            .unwrap()
            .try_into()
            .unwrap(),
    )
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hrx-cert-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    d
}

fn openssl(args: &[&str]) -> (bool, String) {
    let o = Command::new("openssl")
        .args(args)
        .output()
        .expect("openssl must be installed to run this test");
    (
        o.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )
}

#[test]
fn certificates_parse_and_verify_in_openssl_for_both_key_types() {
    for (kind, algo_text, key_text) in [
        ("p256", "ecdsa-with-SHA256", "id-ecPublicKey"),
        ("ed25519", "ED25519", "ED25519"),
    ] {
        let dir = tmp(kind);
        make(
            &dir,
            &[
                "--name",
                "board.example",
                "--dns",
                "board.local",
                "--ip",
                "192.168.2.100",
                "--ip",
                "10.0.2.15",
                "--days",
                "30",
                "--key-type",
                kind,
            ],
        );
        let pem = dir.join("cert.pem");
        let (ok, text) = openssl(&["x509", "-in", pem.to_str().unwrap(), "-noout", "-text"]);
        assert!(ok, "{kind}: {text}");
        assert!(text.contains(algo_text), "{kind}: {text}");
        assert!(text.contains(key_text), "{kind}: {text}");
        assert!(
            text.contains("CN=board.example") || text.contains("CN = board.example"),
            "{text}"
        );
        assert!(
            text.contains("DNS:board.local")
                && text.contains("IP Address:192.168.2.100")
                && text.contains("IP Address:10.0.2.15"),
            "{text}"
        );
        assert!(
            text.contains("CA:FALSE")
                && text.contains("TLS Web Server Authentication")
                && text.contains("Digital Signature"),
            "{text}"
        );
        // The signature on the certificate is checked by OpenSSL against the certificate's own key.
        let (ok, text) = openssl(&[
            "verify",
            "-check_ss_sig",
            "-CAfile",
            pem.to_str().unwrap(),
            pem.to_str().unwrap(),
        ]);
        assert!(ok && text.contains("OK"), "{kind}: {text}");
        // The DER and the PEM are the same certificate.
        let (_, der_text) = openssl(&[
            "x509",
            "-inform",
            "DER",
            "-in",
            dir.join("cert.der").to_str().unwrap(),
            "-noout",
            "-fingerprint",
            "-sha256",
        ]);
        let (_, pem_text) = openssl(&[
            "x509",
            "-in",
            pem.to_str().unwrap(),
            "-noout",
            "-fingerprint",
            "-sha256",
        ]);
        assert_eq!(der_text, pem_text);
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Serve one connection with our TLS engine using the made certificate and key.
fn serve(
    listener: &TcpListener,
    cert: Vec<u8>,
    key: [u8; 32],
    ed25519: bool,
) -> std::thread::JoinHandle<Result<(), String>> {
    let listener = listener.try_clone().unwrap();
    std::thread::spawn(move || {
        let config = if ed25519 {
            Config::ed25519(&cert, key)
        } else {
            Config::p256(&cert, key)
        };
        let (mut sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut random = [0u8; 64];
        std::fs::File::open("/dev/urandom")
            .unwrap()
            .read_exact(&mut random)
            .unwrap();
        let mut tls = TlsServer::new(&config, random);
        let (mut request, mut responded) = (Vec::new(), false);
        let t0 = std::time::Instant::now();
        let mut buf = [0u8; 2048];
        while t0.elapsed() < Duration::from_secs(15) {
            if let Ok(n) = sock.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let mut off = 0;
                while off < n {
                    let took = tls.feed(&buf[off..n]);
                    if took == 0 {
                        break;
                    }
                    off += took;
                }
            }
            if let Some(e) = tls.failure() {
                sock.write_all(tls.pending_output()).ok();
                return Err(format!("{e:?}"));
            }
            let n = tls.recv(&mut buf);
            request.extend_from_slice(&buf[..n]);
            if tls.is_connected() && !responded && request.windows(4).any(|w| w == b"\r\n\r\n") {
                responded = true;
                tls.send(b"HTTP/1.0 200 OK\r\nContent-Length: 8\r\n\r\ntrusted\n");
                tls.close();
            }
            let out = tls.pending_output().to_vec();
            if !out.is_empty() {
                sock.write_all(&out).unwrap();
                tls.consume_output(out.len());
            }
            if responded && tls.peer_closed() {
                break;
            }
        }
        Ok(())
    })
}

#[test]
fn curl_trusts_a_made_certificate_by_name_and_by_address_and_rejects_the_wrong_name() {
    for kind in ["p256", "ed25519"] {
        let dir = tmp(&format!("trust-{kind}"));
        let (cert, key) = make(
            &dir,
            &[
                "--name",
                "hardroxide.test",
                "--dns",
                "board.local",
                "--ip",
                "127.0.0.1",
                "--key-type",
                kind,
            ],
        );
        let cacert = dir.join("cert.pem");
        let try_url = |url: &str, resolve: Option<String>| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = serve(&listener, cert.clone(), key, kind == "ed25519");
            let mut cmd = Command::new("curl");
            cmd.args([
                "-s",
                "-m",
                "15",
                "--tlsv1.3",
                "--cacert",
                cacert.to_str().unwrap(),
            ]);
            if let Some(r) = resolve {
                cmd.args(["--resolve", &format!("{r}:{port}:127.0.0.1")]);
            }
            cmd.arg(url.replace("PORT", &port.to_string()));
            let out = cmd.output().unwrap();
            let _ = server.join();
            (
                out.status.code().unwrap(),
                String::from_utf8_lossy(&out.stdout).to_string(),
            )
        };
        // By the DNS name in the certificate: fully verified (no -k).
        let (code, body) = try_url("https://board.local:PORT/", Some("board.local".into()));
        assert_eq!((code, body.as_str()), (0, "trusted\n"), "{kind}: by name");
        // By the IP address in the certificate.
        let (code, body) = try_url("https://127.0.0.1:PORT/", None);
        assert_eq!(
            (code, body.as_str()),
            (0, "trusted\n"),
            "{kind}: by address"
        );
        // A name the certificate does not carry is refused (curl exit 60: peer certificate cannot be authenticated).
        let (code, _) = try_url("https://other.example:PORT/", Some("other.example".into()));
        assert_ne!(code, 0, "{kind}: the wrong name must be refused");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn bad_arguments_are_reported() {
    let o = Command::new(env!("CARGO_BIN_EXE_hardroxide"))
        .args(["make-cert", "--name", "x", "--ip", "300.1.1.1", "--out"])
        .arg(tmp("bad"))
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("not an IPv4 address"));
}
