//! A TLS 1.3 server: the handshake, the key schedule and the record layer, as a state machine from bytes to bytes ([`TlsServer`]), and
//! an HTTPS adapter for `hardroxide-net`'s HTTP server ([`https`]).
//!
//! One cipher suite (`TLS_CHACHA20_POLY1305_SHA256`), X25519, Ed25519 certificates, no client certificates, no session resumption, no
//! 0-RTT, no HelloRetryRequest. **Home-made and unaudited: for education and a trusted network.**

#![no_std]

pub mod https;
mod server;

pub use https::{HttpsServer, TlsStream};
pub use server::{Alert, Config, IN_CAP, OUT_CAP, TlsError, TlsServer};
