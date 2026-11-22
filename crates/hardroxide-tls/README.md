# hardroxide-tls

A TLS 1.3 server (RFC 8446) written for [Hardroxide](https://github.com/jwkunz/Hardroxide): the one cipher suite `TLS_CHACHA20_POLY1305_SHA256`, X25519 key exchange, Ed25519 server certificates, no client
authentication, no resumption. `no_std`, no allocation. It runs over a byte stream (a TCP socket of `hardroxide-net`) and serves HTTP/1.1 through the same server as plain HTTP.

**Home-made cryptography and protocol code for education and a trusted network. Not audited.** See the reference page for what was tested and what was not.
Licensed under either of Apache-2.0 or MIT at your option.
