# hardroxide-tls

> **Warning: home-made and unaudited.** This TLS 1.3 server was written for education and for trusted networks. It matches test vectors and interoperates with OpenSSL, curl and a browser, but nobody outside this project has reviewed it,
> its timing behaviour has not been measured, and its random bytes come from the caller (the examples supply weak ones), and a handshake is easy to use to overload a small device. Do not use it to protect anything valuable. See the [security policy](https://github.com/jwkunz/Hardroxide/blob/main/SECURITY.md).

A TLS 1.3 server (RFC 8446) written for [Hardroxide](https://github.com/jwkunz/Hardroxide): the one cipher suite `TLS_CHACHA20_POLY1305_SHA256`, X25519 key exchange, Ed25519 server certificates, no client
authentication, no resumption. `no_std`, no allocation. It runs over a byte stream (a TCP socket of `hardroxide-net`) and serves HTTP/1.1 through the same server as plain HTTP.

**Home-made cryptography and protocol code for education and a trusted network. Not audited.** See the reference page for what was tested and what was not.
Licensed under either of Apache-2.0 or MIT at your option.
