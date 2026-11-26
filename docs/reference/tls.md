# hardroxide-tls

A TLS 1.3 server (RFC 8446) written for this project, over a byte stream. `no_std`, no allocation. It serves the HTTP server of [`hardroxide-net`](net.md) as HTTPS, using the primitives of
[`hardroxide-crypto`](crypto.md).

> **Home-made and unaudited: for education and a trusted network.** The [security statement](#security-statement) below says what was tested and what was not.

## The server

```rust,ignore
pub const IN_CAP: usize = 4096 + 5 + 256
pub const OUT_CAP: usize = 4096
pub enum Alert
pub struct TlsError
pub struct Config<'a>
pub fn ed25519(certificate: &'a [u8], seed: [u8; 32]) -> Self
pub fn p256(certificate: &'a [u8], private: [u8; 32]) -> Self
pub fn signature_scheme(&self) -> u16
pub struct TlsServer<'a>
pub fn new(config: &'a Config<'a>, random: [u8; 64]) -> Self
pub fn reset(&mut self, random: [u8; 64])
pub fn is_connected(&self) -> bool
pub fn failure(&self) -> Option<TlsError>
pub fn close_sent(&self) -> bool
pub fn is_handshaking(&self) -> bool
pub fn peer_closed(&self) -> bool
pub fn pending_output(&self) -> &[u8]
pub fn consume_output(&mut self, n: usize)
pub fn input_space(&self) -> usize
pub fn app_len(&self) -> usize
pub fn send_space(&self) -> usize
pub fn feed(&mut self, data: &[u8]) -> usize
pub fn process(&mut self)
pub fn recv(&mut self, buf: &mut [u8]) -> usize
pub fn send(&mut self, data: &[u8]) -> usize
pub fn close(&mut self)
```

A `Config` is the server's identity: the DER certificate and its private key, either `Config::p256` (the 32-byte private scalar; **the kind browsers accept**) or `Config::ed25519` (the 32-byte seed; `curl` and
OpenSSL accept it). Make a certificate with [`hardroxide make-cert`](cli.md#make-cert). A `TlsServer` is one connection and is a state machine from bytes to bytes, with no I/O of its own: give it the bytes that arrive
(`feed`; it returns how many it took, fewer if its input buffer is full), send the bytes it wants sent (`pending_output`, then `consume_output(n)` for the `n` bytes that were sent), and exchange application data with
`recv` and `send` (one record's worth at a time) once `is_connected`. `close` queues `close_notify`; `failure` says why a connection failed, and the alert was queued for sending. The 64 random bytes `new` takes (the ServerHello random
and the ephemeral X25519 private key) must be unpredictable and fresh for every connection (see below). `reset` recycles a connection object for the next client, wiping the secrets.

**Handshake.** ClientHello (a single record of up to `IN_CAP` bytes, which holds Chromium's 1.7 KB hello with its 1,216-byte post-quantum key share) -> ServerHello (X25519), a compatibility ChangeCipherSpec, and then,
encrypted, EncryptedExtensions (empty), Certificate, CertificateVerify and Finished -> the client's Finished -> application data. The key schedule is HKDF-SHA-256 as in RFC 8446 section 7.1. The only cipher
suite is `TLS_CHACHA20_POLY1305_SHA256`; the only group is X25519; the signature algorithms are `ecdsa_secp256r1_sha256` (0x0403) for a P-256 certificate and `ed25519` (0x0807) for an Ed25519 one. A client
that does not offer what the certificate needs, or the suite, or TLS 1.3, or an X25519 key share, gets a `handshake_failure`, `protocol_version` or `missing_extension` alert.

**Not supported** (the connection fails or the feature is ignored, as noted): other cipher suites (AES-GCM) and groups (so a client that has no X25519 key share fails: **no HelloRetryRequest**); TLS 1.2 and earlier; client
certificates; session resumption and tickets (a PSK offer is ignored and a full handshake is done); 0-RTT; ALPN (ignored: HTTP/1.1 is what is served, and browsers fall back to it); the server name (ignored: the one certificate
is always sent); KeyUpdate (a post-handshake message is `unexpected_message`); a ClientHello split over several records; certificate chains (one certificate is sent); OCSP; renegotiation (TLS 1.3 has none); records
over `IN_CAP` bytes (`record_overflow`). Duplicate extensions are not rejected.

## HTTPS

```rust,ignore
pub struct TlsStream<'s, 'c, const N: usize>
pub struct HttpsServer<'c, const CONNS: usize, const REQ: usize, const SCRATCH: usize>
pub fn new(
pub fn served(&self) -> u32
pub fn poll<D: Device, const T: usize, const U: usize, const BUF: usize>(
```

`HttpsServer::new(port, first_socket, &config, &mut random)` serves `port` on `CONNS` of the stack's TCP sockets (from `first_socket`), one `TlsServer` per socket (about 13 KB each: input, output and application
buffers); `poll(&mut stack, &mut random, handler)` is the HTTP server's `poll` (same handler, same responses, see [`hardroxide-net`](net.md#http)) with TLS in between. `random` is a closure that returns 64 fresh bytes (it is
called for every new connection). `TlsStream` is the adapter doing the work: it implements the HTTP server's `Stream` trait over a `TcpSocket` and a `TlsServer`, moving bytes between them on every call. A failed handshake
sends its alert and closes the socket; a connection that never finishes its handshake is cut by TCP's own timers (an idle half-open connection holds a socket until the peer gives up: there is no TLS handshake timeout).

## Where the random bytes come from

The server needs unpredictable bytes for every handshake, and **the Zynq-7000 has no hardware random number generator**. The examples use [`Drbg`](crypto.md#the-random-bit-generator), fed with:
the clock at start (a value an observer of the board's start-up could largely guess); the timing in nanoseconds of every new connection (a few bits each, partly observable by the peer); and, in `net_https`,
a 32-byte seed kept in `/RNGSEED` on the SD card (`rt_station` too). A program renews it from the generator at every start, so that entropy
accumulates over runs, only on the card named when it was built (`HRX_WRITE_SERIAL`); any other card is only read, and its seed stays the same
from start to start. `rt_https` has no card and only the clock and the connection timing. **That is weak.**
An attacker who can predict those inputs can predict the ephemeral keys and so decrypt recorded sessions. The honest options are a seed generated on a host with a real random source and written to the card (the
`/RNGSEED` file is just bytes: `head -c 32 /dev/urandom` onto a FAT32 card gives a strong first seed), a hardware source in the PL (a ring-oscillator or the XADC noise; planned, not done), or not
using the server outside a trusted network. The generator hashes everything it is given, so adding more entropy never hurts.

## Testing

`cargo test -p hardroxide-tls`: the server against **OpenSSL's `s_client` and `curl`** over a real TCP socket on the host, with an Ed25519 and a P-256 certificate (TLS 1.3, ChaCha20-Poly1305, the right signature
algorithm and X25519 reported by the client); a client that offers only AES-GCM, or only Ed25519 to a P-256 server, is refused with `handshake_failure`; garbage, enormous records and 200 random byte streams do not panic. `cargo test -p hardroxide-cli --test
make_cert`: `curl` fully verifies (no `-k`) a certificate made by `hardroxide make-cert` by DNS name and by IP address and refuses a wrong name. `scripts/test-qemu.sh`: the whole stack on QEMU's emulated MAC against `curl`
and `openssl s_client` (a page, a 404, a 100,000-byte download byte for byte, a POST echoed, 30 handshakes in a row, 6 at once over 4 sockets, a refused client, garbage and an idle connection that do not stop the server, plain
HTTP beside it). **On the board** a PC's `curl`, `openssl s_client` and **Microsoft Edge** (a Chromium browser with its own TLS stack, headless) fetch pages over TLS 1.3 (see the tutorial).

**Found by testing against a browser:** Edge's ClientHello offers ChaCha20-Poly1305, X25519 (with a post-quantum share alongside) and the signature algorithms ECDSA P-256, ECDSA P-384, RSA-PSS and RSA, and
**not Ed25519**: the first version of this server (Ed25519 only) was refused with `ERR_SSL_VERSION_OR_CIPHER_MISMATCH`. That is why P-256 ECDSA was added.

## Performance

On the PYNQ-Z2 (one 650 MHz core, no cryptographic hardware): a handshake costs two X25519 (26 ms each) and one signature (22 ms for P-256, 44 ms for Ed25519), about 80 ms with the rest. Measured from a PC over a direct cable with a
P-256 certificate, every request a new connection with a full handshake: **10.9 requests/s** from one client (83 ms each) and **12.5/s** from four. The public-key operations dominate; the bulk cipher was not benchmarked separately. The processor is the limit, and one handshake in progress delays the others.

## Security statement

This is an educational stack. Specifically:

- **Not audited and not reviewed by anyone else.** The protocol code, the key schedule and the cryptography were written for this project and tested against OpenSSL (also inside curl) and Edge, which shows that they agree with those implementations
  on the inputs tried, not that they are secure against an adversary.
- **The random bytes are weak** (above). This is the most serious weakness.
- **"Constant time" is by construction, not by measurement** (see [the crypto reference](crypto.md#what-is-not-verified)).
- **Denial of service is easy.** A handshake costs about 80 ms of the processor and the server has a handful of sockets: a few connections a second keep it busy, and an idle connection holds a socket until the peer gives up.
- **The certificate is self-signed.** A browser warns; the protection against an impersonator is only as good as the user's check of the fingerprint (`hardroxide make-cert` prints it). The repository's test key
  (`fixtures/tls/`) is public and must never be used for anything real.
- **No resumption, no client authentication, one cipher suite, one certificate, no chain.** Features that would broaden the attack surface were left out; features that a modern deployment expects were left out too.
- Recommended use: a lab or home network you control, for controlling a board from a browser without sending the traffic in clear. Not the open internet, not anything that guards something valuable.

Test vectors, differential tests against OpenSSL, parser fuzzing on random bytes and interoperability with independent clients (OpenSSL, which curl also uses, and Chromium's BoringSSL in Edge) are the minimum that was done before this shipped; an independent review would be the next step.
