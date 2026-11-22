//! The TLS 1.3 server state machine.

use hardroxide_crypto::{
    HmacSha256, Sha256, constant_time_eq, ecdsa_p256_sign, ed25519_public,
    ed25519_sign_with_public, hkdf_expand, hkdf_extract, open, p256_public, seal, sha256,
    sig_to_der, x25519, x25519_public,
};

/// The most record bytes (header and payload) the server takes in. A longer record is refused with `record_overflow`.
pub const IN_CAP: usize = 4096 + 5 + 256;
/// The output buffer: a handshake flight and a few application records.
pub const OUT_CAP: usize = 4096;
/// Decrypted application data waiting to be read.
const APP_CAP: usize = 4096;
/// The largest application plaintext put into one record (it fits a TCP segment with headers).
const RECORD_PLAINTEXT: usize = 1300;

const CIPHER_CHACHA20_POLY1305: u16 = 0x1303;
const GROUP_X25519: u16 = 0x001d;
const SIGALG_ED25519: u16 = 0x0807;
const SIGALG_ECDSA_P256_SHA256: u16 = 0x0403;
const VERSION_TLS13: u16 = 0x0304;

const CT_ALERT: u8 = 21;
const CT_HANDSHAKE: u8 = 22;
const CT_APPDATA: u8 = 23;
const CT_CCS: u8 = 20;
const HS_CLIENT_HELLO: u8 = 1;
const HS_SERVER_HELLO: u8 = 2;
const HS_ENCRYPTED_EXTENSIONS: u8 = 8;
const HS_CERTIFICATE: u8 = 11;
const HS_CERTIFICATE_VERIFY: u8 = 15;
const HS_FINISHED: u8 = 20;

/// A TLS alert description, sent when the connection fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Alert {
    CloseNotify = 0,
    UnexpectedMessage = 10,
    BadRecordMac = 20,
    RecordOverflow = 22,
    HandshakeFailure = 40,
    IllegalParameter = 47,
    DecodeError = 50,
    DecryptError = 51,
    ProtocolVersion = 70,
    InternalError = 80,
    MissingExtension = 109,
}

/// Why a connection failed: the alert sent, and a short reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlsError {
    pub alert: Alert,
    pub reason: &'static str,
}

fn err<T>(alert: Alert, reason: &'static str) -> Result<T, TlsError> {
    Err(TlsError { alert, reason })
}

/// The private key behind the server's certificate.
enum Key {
    Ed25519 { seed: [u8; 32], public: [u8; 32] },
    P256 { private: [u8; 32] },
}

/// The server's identity: a DER certificate and the private key it certifies. Chromium-based browsers offer only ECDSA P-256 (and RSA) signatures,
/// so a certificate for them must be [`Config::p256`]; `curl` and OpenSSL accept either.
pub struct Config<'a> {
    certificate: &'a [u8],
    key: Key,
}

impl<'a> Config<'a> {
    /// An Ed25519 certificate: `certificate` is the DER X.509 certificate whose key is the Ed25519 key of `seed`. Computes the public key once.
    pub fn ed25519(certificate: &'a [u8], seed: [u8; 32]) -> Self {
        Config {
            certificate,
            key: Key::Ed25519 {
                public: ed25519_public(&seed),
                seed,
            },
        }
    }

    /// An ECDSA P-256 certificate: `certificate` is the DER X.509 certificate whose key is the P-256 key with the 32-byte big-endian `private` scalar.
    /// Panics if `private` is not a valid key (1 to n-1).
    pub fn p256(certificate: &'a [u8], private: [u8; 32]) -> Self {
        assert!(
            p256_public(&private).is_some(),
            "not a valid P-256 private key"
        );
        Config {
            certificate,
            key: Key::P256 { private },
        }
    }

    /// The signature algorithm the certificate's key signs with (a TLS SignatureScheme code).
    pub fn signature_scheme(&self) -> u16 {
        match self.key {
            Key::Ed25519 { .. } => SIGALG_ED25519,
            Key::P256 { .. } => SIGALG_ECDSA_P256_SHA256,
        }
    }

    /// Sign the CertificateVerify content: returns the signature bytes and their count (at most 72).
    fn sign(&self, content: &[u8]) -> ([u8; 72], usize) {
        match &self.key {
            Key::Ed25519 { seed, public } => {
                let mut out = [0u8; 72];
                out[..64].copy_from_slice(&ed25519_sign_with_public(seed, public, content));
                (out, 64)
            }
            Key::P256 { private } => {
                let sig = ecdsa_p256_sign(private, &sha256(content)).expect("the key was checked");
                sig_to_der(&sig)
            }
        }
    }
}

fn wipe(b: &mut [u8]) {
    for x in b.iter_mut() {
        // SAFETY: a plain write to a valid byte; volatile so the compiler does not drop it as dead.
        unsafe { core::ptr::write_volatile(x, 0) };
    }
}

fn expand_label(secret: &[u8; 32], label: &[u8], context: &[u8], out: &mut [u8]) {
    let mut info = [0u8; 2 + 1 + 6 + 16 + 1 + 32];
    info[..2].copy_from_slice(&(out.len() as u16).to_be_bytes());
    info[2] = (6 + label.len()) as u8;
    info[3..9].copy_from_slice(b"tls13 ");
    info[9..9 + label.len()].copy_from_slice(label);
    let mut n = 9 + label.len();
    info[n] = context.len() as u8;
    info[n + 1..n + 1 + context.len()].copy_from_slice(context);
    n += 1 + context.len();
    hkdf_expand(secret, &info[..n], out);
}

fn derive_secret(secret: &[u8; 32], label: &[u8], transcript_hash: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    expand_label(secret, label, transcript_hash, &mut out);
    out
}

/// Record protection keys for one direction.
#[derive(Clone)]
struct Keys {
    key: [u8; 32],
    iv: [u8; 12],
    seq: u64,
}

impl Keys {
    fn from_secret(secret: &[u8; 32]) -> Keys {
        let (mut key, mut iv) = ([0u8; 32], [0u8; 12]);
        expand_label(secret, b"key", &[], &mut key);
        expand_label(secret, b"iv", &[], &mut iv);
        Keys { key, iv, seq: 0 }
    }
    fn nonce(&self) -> [u8; 12] {
        let mut n = self.iv;
        for (i, b) in self.seq.to_be_bytes().iter().enumerate() {
            n[4 + i] ^= b;
        }
        n
    }
    fn wipe(&mut self) {
        wipe(&mut self.key);
        wipe(&mut self.iv);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    WaitClientHello,
    WaitClientFinished,
    Connected,
    Failed,
}

/// A TLS 1.3 server connection. Feed it the bytes that arrive from the peer, take the bytes it wants sent, read and write application data.
pub struct TlsServer<'a> {
    config: &'a Config<'a>,
    state: State,
    random: [u8; 64], // 32 for the ServerHello random, 32 for the X25519 private key
    input: [u8; IN_CAP],
    in_len: usize,
    output: [u8; OUT_CAP],
    out_len: usize,
    app: [u8; APP_CAP],
    app_len: usize,
    transcript: Sha256,
    read_keys: Option<Keys>,
    write_keys: Option<Keys>,
    // Kept until the client's Finished arrives.
    client_hs_secret: [u8; 32],
    master_secret: [u8; 32],
    server_finished_transcript: [u8; 32],
    peer_closed: bool,
    sent_close: bool,
    error: Option<TlsError>,
}

impl<'a> TlsServer<'a> {
    /// A new connection. `random` must be 64 fresh unpredictable bytes (the ServerHello random and the ephemeral X25519 private key).
    pub fn new(config: &'a Config<'a>, random: [u8; 64]) -> Self {
        TlsServer {
            config,
            state: State::WaitClientHello,
            random,
            input: [0; IN_CAP],
            in_len: 0,
            output: [0; OUT_CAP],
            out_len: 0,
            app: [0; APP_CAP],
            app_len: 0,
            transcript: Sha256::new(),
            read_keys: None,
            write_keys: None,
            client_hs_secret: [0; 32],
            master_secret: [0; 32],
            server_finished_transcript: [0; 32],
            peer_closed: false,
            sent_close: false,
            error: None,
        }
    }

    /// Start over for a new connection (with fresh randomness).
    pub fn reset(&mut self, random: [u8; 64]) {
        self.wipe_secrets();
        let config = self.config;
        *self = TlsServer::new(config, random);
    }

    fn wipe_secrets(&mut self) {
        wipe(&mut self.random);
        wipe(&mut self.client_hs_secret);
        wipe(&mut self.master_secret);
        if let Some(k) = self.read_keys.as_mut() {
            k.wipe();
        }
        if let Some(k) = self.write_keys.as_mut() {
            k.wipe();
        }
        wipe(&mut self.app);
        wipe(&mut self.input);
    }

    // ---- What the application sees ----

    /// The handshake is complete and application data can flow.
    pub fn is_connected(&self) -> bool {
        self.state == State::Connected
    }
    /// The connection failed (an alert was or will be sent) or the peer closed.
    pub fn failure(&self) -> Option<TlsError> {
        self.error
    }
    /// We have queued our `close_notify`.
    pub fn close_sent(&self) -> bool {
        self.sent_close
    }
    /// The handshake has not finished and has not failed.
    pub fn is_handshaking(&self) -> bool {
        matches!(
            self.state,
            State::WaitClientHello | State::WaitClientFinished
        )
    }
    /// The peer sent `close_notify`: no more data will arrive.
    pub fn peer_closed(&self) -> bool {
        self.peer_closed
    }
    /// Bytes waiting to be sent to the peer.
    pub fn pending_output(&self) -> &[u8] {
        &self.output[..self.out_len]
    }
    /// The first `n` pending output bytes have been sent.
    pub fn consume_output(&mut self, n: usize) {
        let n = n.min(self.out_len);
        self.output.copy_within(n..self.out_len, 0);
        self.out_len -= n;
    }
    /// How many bytes of input [`TlsServer::feed`] would take now.
    pub fn input_space(&self) -> usize {
        if self.state == State::Failed {
            0
        } else {
            IN_CAP - self.in_len
        }
    }
    /// Application bytes waiting to be read.
    pub fn app_len(&self) -> usize {
        self.app_len
    }
    /// Room to queue application data (one record's overhead included).
    pub fn send_space(&self) -> usize {
        if self.state != State::Connected || self.sent_close {
            return 0;
        }
        (OUT_CAP - self.out_len)
            .saturating_sub(5 + 1 + 16)
            .min(RECORD_PLAINTEXT)
    }

    /// Give the server bytes from the network; returns how many it took (fewer than `data.len()` if its buffer is full:
    /// offer the rest again later). Processes every complete record.
    pub fn feed(&mut self, data: &[u8]) -> usize {
        if self.state == State::Failed {
            return 0;
        }
        let n = data.len().min(IN_CAP - self.in_len);
        self.input[self.in_len..self.in_len + n].copy_from_slice(&data[..n]);
        self.in_len += n;
        self.process();
        n
    }

    /// Process records already buffered (call after reading application data to make room).
    pub fn process(&mut self) {
        loop {
            if self.state == State::Failed || self.in_len < 5 {
                return;
            }
            let len = usize::from(u16::from_be_bytes([self.input[3], self.input[4]]));
            if 5 + len > IN_CAP {
                self.fail(TlsError {
                    alert: Alert::RecordOverflow,
                    reason: "record too large",
                });
                return;
            }
            if self.in_len < 5 + len {
                return;
            }
            // Application data waits until the application has read what it has.
            if self.state == State::Connected && self.input[0] == CT_APPDATA && self.app_len > 0 {
                return;
            }
            if let Err(e) = self.handle_record(len) {
                self.fail(e);
                return;
            }
            self.input.copy_within(5 + len..self.in_len, 0);
            self.in_len -= 5 + len;
        }
    }

    /// Read decrypted application bytes.
    pub fn recv(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.app_len);
        buf[..n].copy_from_slice(&self.app[..n]);
        self.app.copy_within(n..self.app_len, 0);
        self.app_len -= n;
        if n > 0 {
            self.process();
        }
        n
    }

    /// Queue application bytes for the peer; returns how many were taken (at most one record's worth).
    pub fn send(&mut self, data: &[u8]) -> usize {
        let n = data.len().min(self.send_space());
        if n == 0 {
            return 0;
        }
        self.queue_record(CT_APPDATA, &data[..n]);
        n
    }

    /// Send `close_notify`: no more data will be sent.
    pub fn close(&mut self) {
        if self.state == State::Connected && !self.sent_close {
            self.sent_close = true;
            self.queue_record(CT_ALERT, &[1, Alert::CloseNotify as u8]);
        }
    }

    // ---- Failing ----

    fn fail(&mut self, e: TlsError) {
        if self.state != State::Failed {
            // The alert goes out in the clear before the keys exist and protected after. (A failure while the queue is full drops it.)
            let body = [2u8, e.alert as u8];
            if self.write_keys.is_some() {
                self.queue_record(CT_ALERT, &body);
            } else {
                self.queue_plain(CT_ALERT, &body);
            }
            self.error = Some(e);
            self.state = State::Failed;
            self.wipe_secrets();
        }
    }

    // ---- Records out ----

    fn queue_plain(&mut self, content_type: u8, payload: &[u8]) {
        if self.out_len + 5 + payload.len() > OUT_CAP {
            return;
        }
        let o = &mut self.output[self.out_len..];
        o[0] = content_type;
        o[1..3].copy_from_slice(&0x0303u16.to_be_bytes());
        o[3..5].copy_from_slice(&(payload.len() as u16).to_be_bytes());
        o[5..5 + payload.len()].copy_from_slice(payload);
        self.out_len += 5 + payload.len();
    }

    /// Protect `payload` as an inner content type and queue the record. Needs `write_keys`.
    fn queue_record(&mut self, inner_type: u8, payload: &[u8]) {
        let total = 5 + payload.len() + 1 + 16;
        if self.out_len + total > OUT_CAP || self.write_keys.is_none() {
            return;
        }
        let keys = self.write_keys.as_mut().unwrap();
        let (nonce, key) = (keys.nonce(), keys.key);
        keys.seq += 1;
        let o = &mut self.output[self.out_len..self.out_len + total];
        let ct_len = payload.len() + 1 + 16;
        o[0] = CT_APPDATA;
        o[1..3].copy_from_slice(&0x0303u16.to_be_bytes());
        o[3..5].copy_from_slice(&(ct_len as u16).to_be_bytes());
        o[5..5 + payload.len()].copy_from_slice(payload);
        o[5 + payload.len()] = inner_type;
        let header: [u8; 5] = o[..5].try_into().unwrap();
        let tag = seal(&key, &nonce, &header, &mut o[5..5 + payload.len() + 1]);
        o[5 + payload.len() + 1..].copy_from_slice(&tag);
        self.out_len += total;
    }

    // ---- Records in ----

    fn handle_record(&mut self, len: usize) -> Result<(), TlsError> {
        let content_type = self.input[0];
        match self.state {
            State::WaitClientHello => {
                if content_type != CT_HANDSHAKE {
                    return err(Alert::UnexpectedMessage, "expected a ClientHello");
                }
                self.handle_client_hello(len)
            }
            State::WaitClientFinished | State::Connected => {
                if content_type == CT_CCS && self.state == State::WaitClientFinished {
                    // The compatibility change_cipher_spec: a single byte 1, ignored.
                    return if len == 1 && self.input[5] == 1 {
                        Ok(())
                    } else {
                        err(Alert::UnexpectedMessage, "bad change_cipher_spec")
                    };
                }
                if content_type == CT_ALERT {
                    return err(
                        Alert::UnexpectedMessage,
                        "plaintext alert after the handshake began",
                    );
                }
                if content_type != CT_APPDATA {
                    return err(Alert::UnexpectedMessage, "expected a protected record");
                }
                self.handle_protected(len)
            }
            State::Failed => Ok(()),
        }
    }

    fn handle_protected(&mut self, len: usize) -> Result<(), TlsError> {
        if len < 16 + 1 {
            return err(Alert::DecodeError, "short protected record");
        }
        let keys = self.read_keys.as_mut().ok_or(TlsError {
            alert: Alert::InternalError,
            reason: "no read keys",
        })?;
        let (nonce, key) = (keys.nonce(), keys.key);
        let header: [u8; 5] = self.input[..5].try_into().unwrap();
        let tag: [u8; 16] = self.input[5 + len - 16..5 + len].try_into().unwrap();
        if !open(
            &key,
            &nonce,
            &header,
            &mut self.input[5..5 + len - 16],
            &tag,
        ) {
            return err(Alert::BadRecordMac, "record authentication failed");
        }
        self.read_keys.as_mut().unwrap().seq += 1;
        // The inner type is the last non-zero byte (zeros after it are padding).
        let mut end = len - 16;
        while end > 0 && self.input[5 + end - 1] == 0 {
            end -= 1;
        }
        if end == 0 {
            return err(Alert::UnexpectedMessage, "empty inner plaintext");
        }
        let inner_type = self.input[5 + end - 1];
        let plain_len = end - 1;
        match (self.state, inner_type) {
            (State::WaitClientFinished, CT_HANDSHAKE) => self.handle_client_finished(5, plain_len),
            (State::Connected, CT_APPDATA) => {
                if plain_len > APP_CAP - self.app_len {
                    return err(Alert::InternalError, "application buffer full");
                }
                let (src, dst) = (5, self.app_len);
                self.app[dst..dst + plain_len].copy_from_slice(&self.input[src..src + plain_len]);
                self.app_len += plain_len;
                Ok(())
            }
            (State::Connected, CT_ALERT) => {
                if plain_len == 2 && self.input[6] == Alert::CloseNotify as u8 {
                    self.peer_closed = true;
                    Ok(())
                } else {
                    err(Alert::UnexpectedMessage, "the peer sent an alert")
                }
            }
            (State::Connected, CT_HANDSHAKE) => err(
                Alert::UnexpectedMessage,
                "post-handshake messages (KeyUpdate and the like) are not supported",
            ),
            _ => err(Alert::UnexpectedMessage, "unexpected protected record"),
        }
    }

    // ---- The handshake ----

    fn handle_client_hello(&mut self, record_len: usize) -> Result<(), TlsError> {
        let rec = &self.input[5..5 + record_len];
        if rec.len() < 4 || rec[0] != HS_CLIENT_HELLO {
            return err(Alert::UnexpectedMessage, "expected a ClientHello");
        }
        let body_len =
            (usize::from(rec[1]) << 16) | (usize::from(rec[2]) << 8) | usize::from(rec[3]);
        if 4 + body_len != rec.len() {
            return err(
                Alert::DecodeError,
                "a ClientHello split over records (or trailing bytes) is not supported",
            );
        }
        let hello = Hello::parse(&rec[4..])?;
        if !hello.cipher_ok {
            return err(
                Alert::HandshakeFailure,
                "the client does not offer TLS_CHACHA20_POLY1305_SHA256",
            );
        }
        if !hello.version_ok {
            return err(Alert::ProtocolVersion, "the client does not offer TLS 1.3");
        }
        let needed = if self.config.signature_scheme() == SIGALG_ED25519 {
            1
        } else {
            2
        };
        if hello.sigalgs & needed == 0 {
            return err(
                Alert::HandshakeFailure,
                "the client does not accept signatures of the certificate's key type",
            );
        }
        let Some(client_share) = hello.x25519_share else {
            return err(
                Alert::HandshakeFailure,
                "no X25519 key share (HelloRetryRequest is not supported)",
            );
        };
        let mut session_id = [0u8; 32];
        session_id[..hello.session_id.len()].copy_from_slice(hello.session_id);
        let session_id_len = hello.session_id.len();
        // The transcript starts with the whole ClientHello handshake message.
        self.transcript.update(rec);

        // Key exchange.
        let mut eph_private = [0u8; 32];
        eph_private.copy_from_slice(&self.random[32..]);
        let eph_public = x25519_public(&eph_private);
        let mut shared = x25519(&eph_private, &client_share);
        wipe(&mut eph_private);
        if hardroxide_crypto::x25519::is_all_zero(&shared) {
            return err(Alert::IllegalParameter, "a low-order X25519 point");
        }

        // ServerHello.
        let mut sh = [0u8; 4 + 2 + 32 + 1 + 32 + 2 + 1 + 2 + 6 + 40];
        let mut n = 4;
        sh[0] = HS_SERVER_HELLO;
        sh[n..n + 2].copy_from_slice(&0x0303u16.to_be_bytes());
        n += 2;
        sh[n..n + 32].copy_from_slice(&self.random[..32]);
        n += 32;
        sh[n] = session_id_len as u8;
        sh[n + 1..n + 1 + session_id_len].copy_from_slice(&session_id[..session_id_len]);
        n += 1 + session_id_len;
        sh[n..n + 2].copy_from_slice(&CIPHER_CHACHA20_POLY1305.to_be_bytes());
        sh[n + 2] = 0; // no compression
        n += 3;
        // Extensions: supported_versions (selected 1.3) and key_share (our X25519 public key).
        let ext_start = n;
        n += 2;
        sh[n..n + 6].copy_from_slice(&[0x00, 0x2b, 0x00, 0x02, 0x03, 0x04]);
        n += 6;
        sh[n..n + 4].copy_from_slice(&[0x00, 0x33, 0x00, 0x24]);
        sh[n + 4..n + 6].copy_from_slice(&GROUP_X25519.to_be_bytes());
        sh[n + 6..n + 8].copy_from_slice(&32u16.to_be_bytes());
        sh[n + 8..n + 40].copy_from_slice(&eph_public);
        n += 40;
        let ext_len = n - ext_start - 2;
        sh[ext_start..ext_start + 2].copy_from_slice(&(ext_len as u16).to_be_bytes());
        let body = n - 4;
        sh[1..4].copy_from_slice(&(body as u32).to_be_bytes()[1..]);
        let sh = &sh[..n];
        self.transcript.update(sh);
        self.queue_plain(CT_HANDSHAKE, sh);
        self.queue_plain(CT_CCS, &[1]); // for middleboxes; the client ignores it

        // The key schedule up to the handshake traffic keys.
        let zeros = [0u8; 32];
        let early = hkdf_extract(&[], &zeros);
        let empty_hash = sha256(b"");
        let derived = derive_secret(&early, b"derived", &empty_hash);
        let handshake_secret = hkdf_extract(&derived, &shared);
        wipe(&mut shared);
        let hello_hash = self.transcript.clone().finish();
        let client_hs = derive_secret(&handshake_secret, b"c hs traffic", &hello_hash);
        let server_hs = derive_secret(&handshake_secret, b"s hs traffic", &hello_hash);
        self.read_keys = Some(Keys::from_secret(&client_hs));
        self.write_keys = Some(Keys::from_secret(&server_hs));
        self.client_hs_secret = client_hs;
        let derived2 = derive_secret(&handshake_secret, b"derived", &empty_hash);
        self.master_secret = hkdf_extract(&derived2, &zeros);

        // EncryptedExtensions (none), Certificate, CertificateVerify, Finished: one protected record each.
        let ee = [HS_ENCRYPTED_EXTENSIONS, 0, 0, 2, 0, 0];
        self.transcript.update(&ee);
        self.queue_record(CT_HANDSHAKE, &ee);

        let cert = self.config.certificate;
        if cert.len() > 2000 {
            return err(Alert::InternalError, "certificate too large");
        }
        let mut msg = [0u8; 2048 + 16];
        let m = 4 + 1 + 3 + 3 + cert.len() + 2;
        msg[0] = HS_CERTIFICATE;
        msg[1..4].copy_from_slice(&((m - 4) as u32).to_be_bytes()[1..]);
        msg[4] = 0; // an empty certificate_request_context
        msg[5..8].copy_from_slice(&((3 + cert.len() + 2) as u32).to_be_bytes()[1..]);
        msg[8..11].copy_from_slice(&(cert.len() as u32).to_be_bytes()[1..]);
        msg[11..11 + cert.len()].copy_from_slice(cert);
        // the entry's extensions length (0) is already zero
        self.transcript.update(&msg[..m]);
        self.queue_record(CT_HANDSHAKE, &msg[..m]);

        // CertificateVerify: sign 64 spaces, a context string, a zero byte and the transcript hash so far.
        let mut to_sign = [0u8; 64 + 33 + 1 + 32];
        to_sign[..64].fill(0x20);
        to_sign[64..97].copy_from_slice(b"TLS 1.3, server CertificateVerify");
        to_sign[97] = 0;
        to_sign[98..].copy_from_slice(&self.transcript.clone().finish());
        let (sig, sig_len) = self.config.sign(&to_sign);
        let mut cv = [0u8; 4 + 2 + 2 + 72];
        cv[0] = HS_CERTIFICATE_VERIFY;
        cv[1..4].copy_from_slice(&((2 + 2 + sig_len) as u32).to_be_bytes()[1..]);
        cv[4..6].copy_from_slice(&self.config.signature_scheme().to_be_bytes());
        cv[6..8].copy_from_slice(&(sig_len as u16).to_be_bytes());
        cv[8..8 + sig_len].copy_from_slice(&sig[..sig_len]);
        let cv = &cv[..8 + sig_len];
        self.transcript.update(cv);
        self.queue_record(CT_HANDSHAKE, cv);

        // Finished: HMAC over the transcript with the server's finished key.
        let mut finished_key = [0u8; 32];
        expand_label(&server_hs, b"finished", &[], &mut finished_key);
        let mut hm = HmacSha256::new(&finished_key);
        hm.update(&self.transcript.clone().finish());
        wipe(&mut finished_key);
        let verify = hm.finish();
        let mut fin = [0u8; 4 + 32];
        fin[0] = HS_FINISHED;
        fin[1..4].copy_from_slice(&32u32.to_be_bytes()[1..]);
        fin[4..].copy_from_slice(&verify);
        self.transcript.update(&fin);
        self.queue_record(CT_HANDSHAKE, &fin);
        self.server_finished_transcript = self.transcript.clone().finish();

        // Our application traffic keys start now; the client's follow its Finished.
        let server_ap = derive_secret(
            &self.master_secret,
            b"s ap traffic",
            &self.server_finished_transcript,
        );
        // (The write keys switch after the Finished above was queued with the handshake keys.)
        self.write_keys = Some(Keys::from_secret(&server_ap));
        self.state = State::WaitClientFinished;
        Ok(())
    }

    fn handle_client_finished(&mut self, at: usize, len: usize) -> Result<(), TlsError> {
        let msg = &self.input[at..at + len];
        if len != 4 + 32 || msg[0] != HS_FINISHED || msg[1..4] != [0, 0, 32] {
            return err(Alert::UnexpectedMessage, "expected the client's Finished");
        }
        let mut finished_key = [0u8; 32];
        expand_label(&self.client_hs_secret, b"finished", &[], &mut finished_key);
        let mut hm = HmacSha256::new(&finished_key);
        hm.update(&self.server_finished_transcript);
        wipe(&mut finished_key);
        let expected = hm.finish();
        if !constant_time_eq(&expected, &msg[4..]) {
            return err(Alert::DecryptError, "the client's Finished does not verify");
        }
        let client_ap = derive_secret(
            &self.master_secret,
            b"c ap traffic",
            &self.server_finished_transcript,
        );
        self.read_keys = Some(Keys::from_secret(&client_ap));
        wipe(&mut self.client_hs_secret);
        wipe(&mut self.master_secret);
        wipe(&mut self.random);
        self.state = State::Connected;
        Ok(())
    }
}

/// What the server needs from a ClientHello.
struct Hello<'a> {
    session_id: &'a [u8],
    cipher_ok: bool,
    version_ok: bool,
    sigalgs: u8, // bit 0: Ed25519 offered, bit 1: ECDSA P-256 with SHA-256 offered
    x25519_share: Option<[u8; 32]>,
}

/// A cursor over bytes with checked reads.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], TlsError> {
        if self.0.len() < n {
            return err(Alert::DecodeError, "truncated message");
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, TlsError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, TlsError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    /// A block with a one-byte length prefix.
    fn vec8(&mut self) -> Result<&'a [u8], TlsError> {
        let n = usize::from(self.u8()?);
        self.take(n)
    }
    /// A block with a two-byte length prefix.
    fn vec16(&mut self) -> Result<&'a [u8], TlsError> {
        let n = usize::from(self.u16()?);
        self.take(n)
    }
}

impl<'a> Hello<'a> {
    fn parse(body: &'a [u8]) -> Result<Hello<'a>, TlsError> {
        let mut r = Reader(body);
        let _legacy_version = r.u16()?;
        r.take(32)?; // random
        let session_id = r.vec8()?;
        if session_id.len() > 32 {
            return err(Alert::IllegalParameter, "session id too long");
        }
        let suites = r.vec16()?;
        let cipher_ok = suites
            .as_chunks::<2>()
            .0
            .iter()
            .any(|c| u16::from_be_bytes(*c) == CIPHER_CHACHA20_POLY1305);
        let compression = r.vec8()?;
        if !compression.contains(&0) {
            return err(Alert::IllegalParameter, "no null compression offered");
        }
        let mut h = Hello {
            session_id,
            cipher_ok,
            version_ok: false,
            sigalgs: 0,
            x25519_share: None,
        };
        let mut exts = Reader(r.vec16()?);
        while !exts.0.is_empty() {
            let (ty, data) = (exts.u16()?, exts.vec16()?);
            let mut d = Reader(data);
            match ty {
                // supported_versions: a one-byte-length list of versions.
                0x002b => {
                    let list = d.vec8()?;
                    h.version_ok = list
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .any(|c| u16::from_be_bytes(*c) == VERSION_TLS13);
                }
                // signature_algorithms.
                0x000d => {
                    let list = d.vec16()?;
                    for c in list.as_chunks::<2>().0 {
                        match u16::from_be_bytes(*c) {
                            SIGALG_ED25519 => h.sigalgs |= 1,
                            SIGALG_ECDSA_P256_SHA256 => h.sigalgs |= 2,
                            _ => {}
                        }
                    }
                }
                // key_share: a list of (group, key).
                0x0033 => {
                    let mut shares = Reader(d.vec16()?);
                    while !shares.0.is_empty() {
                        let (group, key) = (shares.u16()?, shares.vec16()?);
                        if group == GROUP_X25519 && key.len() == 32 && h.x25519_share.is_none() {
                            h.x25519_share = Some(key.try_into().unwrap());
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(h)
    }
}
