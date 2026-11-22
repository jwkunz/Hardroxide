# Applications 6: HTTPS with a self-signed certificate

**Design:** `fixtures/board/pynq_base_overlay.xsa` · **Needs:** [Applications 5](05-network.md) (the cable, the address), a host with `curl` or a browser; for your own certificate, the `hardroxide` binary

> **Status: run on a PYNQ-Z2 (2026-11-21)** with the board cabled to a PC: HTTPS with TLS 1.3 to `curl`, `openssl s_client` and **Microsoft Edge** (a Chromium browser, headless), about 11 handshakes a second, and an RTOS variant serving 489 TLS requests in 40 seconds
> while the 10 ms control loop kept its deadline to 2 microseconds. **Not run:** Firefox and Safari, an interactive browser session (Edge was driven with `--dump-dom`), a certificate you made yourself on the board (the throwaway test one was used), hours of traffic.
> **Everything here is home-made and unaudited; read the [security statement](../../reference/tls.md#security-statement) before trusting it with anything.** The first run found that Chromium offers no Ed25519 signatures (section 6).

TLS 1.3 ([`hardroxide-tls`](../../reference/tls.md)) sits between the TCP sockets of the [network stack](05-network.md) and the same HTTP server, so a page is served over HTTPS by changing the server's type. The cryptography
([`hardroxide-crypto`](../../reference/crypto.md): SHA-256, HMAC, HKDF, ChaCha20-Poly1305, X25519, ECDSA P-256 and Ed25519) was written for this project and checked against OpenSSL; so was the certificate tool.

## 1. A certificate

A browser needs a certificate that names the address it connects to. `hardroxide make-cert` makes a self-signed one with the key type browsers accept (ECDSA P-256):

```console
$ hardroxide make-cert --name pynq.local --dns pynq.local --ip 192.168.2.135 --out work/cert
wrote work/cert/cert.der, work/cert/cert.pem and work/cert/key.bin (the private key: keep it secret)
SHA-256 fingerprint: 96:5F:86:5F:35:86:1A:64:2C:FE:BF:0A:DE:F2:73:E8:DF:F8:C7:5B:4E:EB:92:D1:ED:DC:C7:17:9A:9B:32:09
```

`cert.der` is the certificate, `key.bin` the 32-byte private key; the example embeds both (`include_bytes!`). To try the examples first, `fixtures/tls/` holds a **throwaway test identity** whose private key is public, so it protects nothing.
A self-signed certificate is not trusted by anything: a browser warns until you accept it (compare the fingerprint it shows with the one printed here), and `curl` needs `-k` or `--cacert work/cert/cert.pem`.

## 2. An HTTPS server

```console
$ hardroxide generate fixtures/board/pynq_base_overlay.xsa --platform baremetal -o work/base --hardroxide-path . --name design_1
$ cd work/base
$ cargo add --dev --path ../../crates/hardroxide-net --path ../../crates/hardroxide-crypto --path ../../crates/hardroxide-tls --path ../../crates/hardroxide-fat
$ sed -i 's/"panic-handler"\]/"panic-handler", "net"]/' Cargo.toml
$ cp ../../apps/base/baremetal/net_https.rs ../../fixtures/tls/test-cert.der ../../fixtures/tls/test-key.bin examples/
$ cargo build --release --example net_https
$ hardroxide run target/armv7a-none-eabihf/release/examples/net_https --xsa ../../fixtures/board/pynq_base_overlay.xsa --watch 100
```

```console
== net_https ==
random generator: seeded from the clock and the seed file on the card
P-256 public key 21 ms, X25519 26 ms, ECDSA signature 22 ms (a handshake needs two X25519 and one signature)
NET HTTPS READY: https://192.168.2.135/ (from DHCP)
```

From the PC:

```console
$ curl -k https://192.168.2.135/api/status
{"ip":"192.168.2.135","uptime_ms":11863,"leds":0,"switches":1,"buttons":0,"https_requests":0,"rx_frames":10,"tx_frames":9}
$ echo | openssl s_client -connect 192.168.2.135:443 -tls1_3 -brief
verify error:num=18:self-signed certificate
CONNECTION ESTABLISHED
Protocol version: TLSv1.3
Ciphersuite: TLS_CHACHA20_POLY1305_SHA256
Peer certificate: CN=hardroxide-test
Hash used: SHA256
Signature type: ecdsa_secp256r1_sha256
Verification error: self-signed certificate
Peer Temp Key: X25519, 253 bits
```

and in a Chromium browser (here Microsoft Edge, headless, which accepted the certificate when told to ignore the warning):

```console
$ microsoft-edge --headless=new --ignore-certificate-errors --dump-dom https://192.168.2.135/
<!DOCTYPE html>
<html><head><title>Hardroxide HTTPS</title></head><body><h1>Hardroxide on the PYNQ-Z2</h1><p>This page came over TLS 1.3 from bare-metal Rust with our own TCP/IP and TLS.</p>...
```

The program is the web server of the previous tutorial with one type changed: `HttpsServer<4, 1024, 768>` instead of `Server<5, 1024, 768>`, plus a certificate and the random-number generator:

```rust,ignore
let tls_config = TlsConfig::p256(CERT, *KEY);            // the DER certificate and the 32-byte private key
let mut random = || { drbg.add_entropy(&now_ns().to_le_bytes()); drbg.bytes64() };   // 64 fresh bytes per connection
let mut server: HttpsServer<4, 1024, 768> = HttpsServer::new(443, 0, &tls_config, &mut random);
loop {
    net.poll(now_ms());
    server.poll(&mut net, &mut random, |req, res| { /* the same handler as for plain HTTP */ });
}
```

**Measured** from a PC over the direct cable, `curl -k`, every request a new connection with a full handshake:

| | |
|---|---|
| one client, one request after another | **10.9 requests/s** (83 ms each, of which about 75 are the handshake's public-key operations), 40 of 40 succeeded |
| four clients at once | **12.5 requests/s**, 60 of 60 succeeded |

The processor is the limit: two X25519 and one ECDSA signature per handshake, in software, on one 650 MHz core.

## 3. The random numbers (the weak point)

A TLS handshake needs bytes nobody can predict, and the Zynq-7000 has no random number generator. The example feeds a generator (`Drbg`) with the clock, the timing of every connection and a seed file on the SD card that it replaces at each start (`random generator: seeded
from the clock and the seed file on the card` above; the first run after formatting says `from the clock only (no seed file yet: weak)`). **That is not good randomness.** For a better first seed, put 32 bytes from a real source in `/RNGSEED` on
the card: `head -c 32 /dev/urandom > /media/$USER/CARD/RNGSEED`. The [reference](../../reference/tls.md#where-the-random-bytes-come-from) says what each input is worth.

## 4. The RTOS variant: TLS beside a 10 ms control loop

`apps/base/rtos/rt_https.rs` is `rt_net` with TLS: a priority-5 `control` task every 10 ms, a priority-3 `network` task (the stack, the TLS connections, the server) and a priority-2 `report` task.

```console
$ for i in 1 2 3; do (while true; do curl -sk https://192.168.2.135/ >/dev/null; done) & done
...
control: 6412 releases, worst deviation from 10 ms 2 us; web: https://192.168.2.135/ 489 requests
```

Three clients made **489 handshakes with requests in 40 seconds (12.2 per second), 0 errors**, while the control loop's worst deviation from its 10 ms period was **2 microseconds** over about 6,900 releases (1 microsecond with no load). Each handshake
is about 80 ms of processor time in the network task, which runs only when the control task sleeps.

## 5. Without the board: QEMU and the host

`scripts/test-qemu.sh` runs this stack on QEMU's emulated MAC and talks to it with `curl` and `openssl s_client`: a page, a 404, a 100,000-byte download compared byte for byte, a POST echoed, 30 handshakes in a row, 6 at once over 4 sockets, a client that
offers only AES-GCM refused with an alert, garbage and an idle connection that do not stop the server, and plain HTTP beside it. `cargo test -p hardroxide-tls` serves real `openssl s_client` and `curl` connections from the TLS engine on the host with Ed25519 and P-256
certificates, and `cargo test -p hardroxide-cli --test make_cert` has `curl` verify, with no `-k`, a certificate from `make-cert` by name and by address.

## 6. What the first run taught us

1. **Chromium does not do Ed25519.** The first server used an Ed25519 certificate (the plan in the ADR). `curl` and OpenSSL were happy; Edge refused with `ERR_SSL_VERSION_OR_CIPHER_MISMATCH`. Capturing its ClientHello
   with a few lines of Python showed why: it offers ChaCha20-Poly1305 and X25519 (plus a 1,216-byte post-quantum key share, which the parser handled) and the signature algorithms ECDSA P-256 (`0x0403`), ECDSA P-384, RSA-PSS and RSA, but not `0x0807`.
   The ADR's acceptance test says a browser must load the page, so ECDSA P-256 was added to the crypto crate: constant-time Montgomery arithmetic with the complete addition formulas, RFC 6979 nonces, and it matched OpenSSL byte for byte on the first
   run. It is also twice as fast as the Ed25519 signature on this core (22 ms against 44 ms).
2. **A test client had to be told not to hang up.** `openssl s_client` quits when its input ends, before reading the answer; `-ign_eof` keeps it reading until the server closes.
3. **A peer that never finishes closing must not hold a socket.** Found while adding a handshake that can fail halfway: TCP's `FIN-WAIT-2` now times out after 10 seconds.

## 7. What is not covered

Firefox and Safari (their signature algorithm lists were not captured; both are expected to accept ECDSA P-256), an interactive browser session, certificate chains and trust anchors (there is one self-signed certificate), session resumption,
HTTP keep-alive (a page needs a handshake each time, which is what the measurements show), the cost of a handshake while other traffic runs on the bare-metal flavor, and anything about the strength of the random numbers beyond "weak".
