# Security policy

## Read this first

Hardroxide's networking and cryptography (`hardroxide-net`, `hardroxide-crypto`, `hardroxide-tls`) are **home-made and have not been audited or reviewed by anyone outside this project.** They are written for
education and for use on a trusted network, and they are tested against OpenSSL, curl and a browser, which shows that they interoperate, not that they are secure against an attacker. In particular:

- the Zynq-7000 has no hardware random number generator, and the random bytes the example programs feed to TLS are **weak**;
- the "constant time" code is constant time by construction and has not been measured;
- a TLS handshake costs about 80 ms of the board's only core, so the server is easy to overload.

Do not use these crates to protect anything valuable. The [TLS reference](https://jwkunz.github.io/Hardroxide/book/reference/tls.html#security-statement) has the full statement.

## Reporting a vulnerability

Please report security problems **privately**, by email to **numerius.engineering@gmail.com** with "Hardroxide security" in the subject, and not in a public issue. Include what you found, how to reproduce it, and the version
(`hardroxide --version` or the crate version). You will get an acknowledgement; this is a small project, so please allow a few days.

What is in scope: flaws in the code of this repository (memory safety, protocol or cryptographic mistakes, parsing bugs that a network peer can trigger, a certificate or key handling error, `make-cert` producing a weak key).
What is known and documented rather than a finding: the weak random bytes of the examples, the lack of an audit, and the cost of a handshake.

## Supported versions

Only the latest release receives fixes. A version with a security problem is yanked from crates.io and the fix is released as a new version.
