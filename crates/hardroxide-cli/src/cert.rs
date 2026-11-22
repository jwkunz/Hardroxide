//! `hardroxide make-cert`: a self-signed X.509 certificate and its private key for the HTTPS examples, made with `hardroxide-crypto`
//! (so the tool and the TLS server share one implementation) and checked against OpenSSL in `tests/make_cert.rs`.

use anyhow::{Result, bail};
use hardroxide_crypto::{
    ecdsa_p256_sign, ed25519_public, ed25519_sign, p256::p256_private_is_valid, p256_public,
    sha256, sig_to_der,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    /// ECDSA with P-256 and SHA-256: what browsers accept.
    P256,
    /// Ed25519: accepted by `curl` and OpenSSL, not by Chromium-based browsers.
    Ed25519,
}

pub struct CertOptions {
    pub common_name: String,
    pub dns_names: Vec<String>,
    pub ips: Vec<[u8; 4]>,
    pub days: u32,
    pub algorithm: Algorithm,
    /// Seconds since 1970 (UTC).
    pub now_unix: u64,
}

pub struct Cert {
    pub der: Vec<u8>,
    /// The private key: the 32-byte P-256 scalar (big-endian) or the Ed25519 seed.
    pub key: [u8; 32],
    pub sha256_fingerprint: [u8; 32],
}

/// DER: tag, length, content.
fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let n = content.len();
    if n < 128 {
        out.push(n as u8);
    } else if n < 256 {
        out.extend_from_slice(&[0x81, n as u8]);
    } else {
        out.extend_from_slice(&[0x82, (n >> 8) as u8, n as u8]);
    }
    out.extend_from_slice(content);
    out
}

fn seq(parts: &[&[u8]]) -> Vec<u8> {
    tlv(0x30, &parts.concat())
}

/// Days since 1970-01-01 to (year, month, day) in the proleptic Gregorian calendar (Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A UTCTime (years 1950 to 2049) or GeneralizedTime.
fn time(unix: u64) -> Vec<u8> {
    let (days, secs) = ((unix / 86_400) as i64, (unix % 86_400) as u32);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (secs / 3600, secs / 60 % 60, secs % 60);
    if (1950..2050).contains(&y) {
        tlv(
            0x17,
            format!("{:02}{m:02}{d:02}{hh:02}{mm:02}{ss:02}Z", y % 100).as_bytes(),
        )
    } else {
        tlv(
            0x18,
            format!("{y:04}{m:02}{d:02}{hh:02}{mm:02}{ss:02}Z").as_bytes(),
        )
    }
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char)
            } else {
                s.push('=')
            }
        }
    }
    s
}

pub fn pem(der: &[u8], label: &str) -> String {
    let b = base64(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for line in b.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(line).unwrap());
        out.push('\n');
    }
    out.push_str(&format!("-----END {label}-----\n"));
    out
}

/// Make the certificate. `random` fills a buffer with unpredictable bytes (the key and the serial number come from it).
pub fn make_cert(o: &CertOptions, random: &mut dyn FnMut(&mut [u8]) -> Result<()>) -> Result<Cert> {
    // The key.
    let mut key = [0u8; 32];
    loop {
        random(&mut key)?;
        if o.algorithm == Algorithm::Ed25519 || p256_private_is_valid(&key) {
            break;
        }
    }
    let (spki, sig_alg) = match o.algorithm {
        Algorithm::P256 => {
            let public = p256_public(&key).expect("a valid key");
            // SubjectPublicKeyInfo: id-ecPublicKey with the prime256v1 curve, then the uncompressed point.
            let alg = seq(&[
                &tlv(0x06, &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01]),
                &tlv(0x06, &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07]),
            ]);
            let mut bits = vec![0u8];
            bits.extend_from_slice(&public);
            // ecdsa-with-SHA256
            (
                seq(&[&alg, &tlv(0x03, &bits)]),
                seq(&[&tlv(
                    0x06,
                    &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02],
                )]),
            )
        }
        Algorithm::Ed25519 => {
            let public = ed25519_public(&key);
            let alg = seq(&[&tlv(0x06, &[0x2b, 0x65, 0x70])]);
            let mut bits = vec![0u8];
            bits.extend_from_slice(&public);
            (
                seq(&[&alg, &tlv(0x03, &bits)]),
                seq(&[&tlv(0x06, &[0x2b, 0x65, 0x70])]),
            )
        }
    };

    // The subject (and issuer): CN only.
    if o.common_name.is_empty() || o.common_name.len() > 64 {
        bail!("the common name must be 1 to 64 characters");
    }
    let name = seq(&[&tlv(
        0x31,
        &seq(&[
            &tlv(0x06, &[0x55, 0x04, 0x03]),
            &tlv(0x0c, o.common_name.as_bytes()),
        ]),
    )]);
    let validity = seq(&[
        &time(o.now_unix.saturating_sub(86_400)),
        &time(o.now_unix + u64::from(o.days) * 86_400),
    ]);
    let mut serial = [0u8; 16];
    random(&mut serial)?;
    serial[0] = (serial[0] & 0x7f) | 0x01; // positive, and not a leading zero byte

    // Extensions: not a CA, a signing key, for servers, and the names (browsers look only at the subject alternative names).
    let mut san = Vec::new();
    let mut dns: Vec<&str> = o.dns_names.iter().map(String::as_str).collect();
    if dns.is_empty() && o.ips.is_empty() {
        dns.push(&o.common_name);
    }
    for d in dns {
        san.extend(tlv(0x82, d.as_bytes()));
    }
    for ip in &o.ips {
        san.extend(tlv(0x87, ip));
    }
    let ext = |oid: &[u8], critical: bool, value: &[u8]| {
        let mut parts = vec![tlv(0x06, oid)];
        if critical {
            parts.push(tlv(0x01, &[0xff]));
        }
        parts.push(tlv(0x04, value));
        tlv(0x30, &parts.concat())
    };
    let extensions = tlv(
        0xa3,
        &seq(&[
            &ext(&[0x55, 0x1d, 0x13], true, &seq(&[])), // basicConstraints: CA false
            &ext(&[0x55, 0x1d, 0x0f], true, &tlv(0x03, &[0x07, 0x80])), // keyUsage: digitalSignature
            &ext(
                &[0x55, 0x1d, 0x25],
                false,
                &seq(&[&tlv(
                    0x06,
                    &[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01],
                )]),
            ), // extKeyUsage: serverAuth
            &ext(&[0x55, 0x1d, 0x11], false, &tlv(0x30, &san)),         // subjectAltName
        ]),
    );
    let tbs = seq(&[
        &tlv(0xa0, &tlv(0x02, &[2])),
        &tlv(0x02, &serial),
        &sig_alg,
        &name,
        &validity,
        &name,
        &spki,
        &extensions,
    ]);

    // Sign the TBSCertificate.
    let sig: Vec<u8> = match o.algorithm {
        Algorithm::P256 => {
            let raw = ecdsa_p256_sign(&key, &sha256(&tbs))
                .ok_or_else(|| anyhow::anyhow!("signing failed"))?;
            let (der, n) = sig_to_der(&raw);
            der[..n].to_vec()
        }
        Algorithm::Ed25519 => ed25519_sign(&key, &tbs).to_vec(),
    };
    let mut sig_bits = vec![0u8];
    sig_bits.extend_from_slice(&sig);
    let der = seq(&[&tbs, &sig_alg, &tlv(0x03, &sig_bits)]);
    let sha256_fingerprint = sha256(&der);
    Ok(Cert {
        der,
        key,
        sha256_fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_base64_are_right() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(time(0), tlv(0x17, b"700101000000Z"));
        assert_eq!(time(2_524_608_000), tlv(0x18, b"20500101000000Z")); // 2050 needs GeneralizedTime
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn lengths_use_the_short_and_long_forms() {
        assert_eq!(tlv(4, &[0; 5])[..2], [4, 5]);
        assert_eq!(tlv(4, &[0; 200])[..3], [4, 0x81, 200]);
        assert_eq!(tlv(4, &[0; 300])[..4], [4, 0x82, 1, 44]);
    }
}
