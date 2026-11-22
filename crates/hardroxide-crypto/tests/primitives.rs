//! Every primitive against the standard vectors and against OpenSSL/Python on random inputs (`vectors.rs`, made by `gen_vectors.py`).

use hardroxide_crypto::*;

mod vectors;
use vectors::*;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}
fn arr<const N: usize>(s: &str) -> [u8; N] {
    unhex(s).try_into().unwrap()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn sha256_and_sha512_match_the_standard_examples() {
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(
        hex(&sha512(b"abc")),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
    // A million 'a's, fed in uneven pieces.
    let mut h = Sha256::new();
    let mut left = 1_000_000;
    let chunk = [b'a'; 997];
    while left > 0 {
        let n = left.min(chunk.len());
        h.update(&chunk[..n]);
        left -= n;
    }
    assert_eq!(
        hex(&h.finish()),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn hashes_match_openssl_on_random_and_edge_lengths() {
    for (m, s256, s512) in HASHES {
        let m = unhex(m);
        assert_eq!(hex(&sha256(&m)), *s256, "sha256 of {} bytes", m.len());
        assert_eq!(hex(&sha512(&m)), *s512, "sha512 of {} bytes", m.len());
        // Incremental hashing in odd pieces gives the same digest.
        let (mut a, mut b) = (Sha256::new(), Sha512::new());
        for piece in m.chunks(7) {
            a.update(piece);
            b.update(piece);
        }
        assert_eq!(hex(&a.finish()), *s256);
        assert_eq!(hex(&b.finish()), *s512);
    }
}

#[test]
fn hmac_matches_python() {
    // RFC 4231 test case 1.
    assert_eq!(
        hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    for (k, d, mac) in HMACS {
        let (k, d) = (unhex(k), unhex(d));
        assert_eq!(
            hex(&hmac_sha256(&k, &d)),
            *mac,
            "key {} data {}",
            k.len(),
            d.len()
        );
        let mut h = HmacSha256::new(&k);
        for piece in d.chunks(5) {
            h.update(piece);
        }
        assert_eq!(hex(&h.finish()), *mac);
    }
}

#[test]
fn hkdf_matches_openssl() {
    // RFC 5869 test case 1.
    let prk = hkdf_extract(&unhex("000102030405060708090a0b0c"), &[0x0b; 22]);
    assert_eq!(
        hex(&prk),
        "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
    );
    let mut okm = [0u8; 42];
    hkdf_expand(&prk, &unhex("f0f1f2f3f4f5f6f7f8f9"), &mut okm);
    assert_eq!(
        hex(&okm),
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
    );
    for (salt, ikm, info, n, want) in HKDFS {
        let prk = hkdf_extract(&unhex(salt), &unhex(ikm));
        let mut okm = vec![0u8; *n];
        hkdf_expand(&prk, &unhex(info), &mut okm);
        assert_eq!(hex(&okm), *want, "length {n}");
    }
}

#[test]
fn chacha20_poly1305_matches_openssl_and_rejects_tampering() {
    for (k, n, aad, pt, want) in AEADS {
        let (k, n, aad, pt, want) = (
            arr::<32>(k),
            arr::<12>(n),
            unhex(aad),
            unhex(pt),
            unhex(want),
        );
        let mut buf = pt.clone();
        let tag = seal(&k, &n, &aad, &mut buf);
        let mut got = buf.clone();
        got.extend_from_slice(&tag);
        assert_eq!(
            got,
            want,
            "seal: {} bytes of data, {} of aad",
            pt.len(),
            aad.len()
        );
        // And back.
        let mut back = buf.clone();
        assert!(open(&k, &n, &aad, &mut back, &tag));
        assert_eq!(back, pt);
        // Any change to the tag, the ciphertext or the aad is refused, and the data is left alone.
        let mut bad_tag = tag;
        bad_tag[(pt.len() + aad.len()) % 16] ^= 1;
        let mut x = buf.clone();
        assert!(!open(&k, &n, &aad, &mut x, &bad_tag));
        assert_eq!(x, buf);
        if !buf.is_empty() {
            let mut y = buf.clone();
            y[0] ^= 0x80;
            assert!(!open(&k, &n, &aad, &mut y, &tag));
        }
        let mut other_aad = aad.clone();
        other_aad.push(0);
        let mut z = buf.clone();
        assert!(!open(&k, &n, &other_aad, &mut z, &tag));
    }
}

#[test]
fn rfc_8439_poly1305_and_chacha_vectors() {
    // RFC 8439 2.5.2: the Poly1305 example.
    let key = arr::<32>("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b");
    assert_eq!(
        hex(&poly1305::poly1305(
            &key,
            b"Cryptographic Forum Research Group"
        )),
        "a8061dc1305136c6c22b8baf0c0127a9"
    );
    // RFC 8439 2.1.1: a quarter round is part of the block function; 2.3.2: the block function test vector.
    let key = arr::<32>("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
    let block = chacha20::chacha20_block(&key, 1, &arr::<12>("000000090000004a00000000"));
    // The serialised block of RFC 8439 2.3.2 (the RFC prints the state as words, these are its bytes, little-endian).
    assert_eq!(
        hex(&block),
        "10f1e7e4d13b5915500fdd1fa32071c4c7d1f4c733c068030422aa9ac3d46c4ed2826446079faa0914c2d705d98b02a2b5129cd1de164eb9cbd083e8a2503c4e"
    );
}

#[test]
fn x25519_matches_openssl_and_the_rfc() {
    // RFC 7748 section 5.2, the first vector.
    let r = x25519(
        &arr("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4"),
        &arr("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c"),
    );
    assert_eq!(
        hex(&r),
        "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552"
    );
    // RFC 7748 section 6.1: Alice and Bob.
    let alice = arr::<32>("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
    let bob = arr::<32>("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
    assert_eq!(
        hex(&x25519_public(&alice)),
        "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"
    );
    assert_eq!(
        hex(&x25519_public(&bob)),
        "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f"
    );
    let shared = x25519(&alice, &x25519_public(&bob));
    assert_eq!(
        hex(&shared),
        "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742"
    );
    assert_eq!(shared, x25519(&bob, &x25519_public(&alice)));
    for (a, u, want) in X25519S {
        assert_eq!(hex(&x25519(&arr(a), &arr(u))), *want);
    }
    for (a, want) in X25519PUBS {
        assert_eq!(hex(&x25519_public(&arr(a))), *want);
    }
    // A point of low order gives the all-zero secret, which the caller must refuse.
    assert!(x25519::is_all_zero(&x25519(&arr(X25519S[0].0), &[0u8; 32])));
}

#[test]
fn ed25519_matches_openssl_and_the_rfc() {
    // RFC 8032 section 7.1, test 1 (the empty message).
    let seed = arr::<32>("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
    let public = ed25519_public(&seed);
    assert_eq!(
        hex(&public),
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    );
    let sig = ed25519_sign(&seed, b"");
    assert_eq!(
        hex(&sig),
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    );
    assert!(ed25519_verify(&public, b"", &sig));
    for (seed, public, msg, sig) in ED25519S {
        let (seed, public, msg, sig) = (
            arr::<32>(seed),
            arr::<32>(public),
            unhex(msg),
            arr::<64>(sig),
        );
        assert_eq!(ed25519_public(&seed), public);
        assert_eq!(
            ed25519_sign(&seed, &msg),
            sig,
            "signature of {} bytes",
            msg.len()
        );
        assert!(ed25519_verify(&public, &msg, &sig));
        // Tampering is caught: the message, either half of the signature, the key.
        let mut m2 = msg.clone();
        m2.push(1);
        assert!(!ed25519_verify(&public, &m2, &sig));
        for at in [0usize, 31, 32, 63] {
            let mut s2 = sig;
            s2[at] ^= 1;
            assert!(!ed25519_verify(&public, &msg, &s2), "signature byte {at}");
        }
        let mut p2 = public;
        p2[5] ^= 1;
        assert!(!ed25519_verify(&p2, &msg, &sig));
    }
}

#[test]
fn ed25519_refuses_a_non_canonical_s() {
    let seed = [7u8; 32];
    let public = ed25519_public(&seed);
    let sig = ed25519_sign(&seed, b"msg");
    assert!(ed25519_verify(&public, b"msg", &sig));
    // S + L is the same point equation but not a canonical S: RFC 8032 says to refuse it.
    let l = unhex("edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010");
    let mut s = [0u8; 33];
    let mut carry = 0u16;
    for i in 0..32 {
        let v = u16::from(sig[32 + i]) + u16::from(l[i]) + carry;
        s[i] = v as u8;
        carry = v >> 8;
    }
    s[32] = carry as u8;
    if s[32] == 0 {
        let mut forged = sig;
        forged[32..].copy_from_slice(&s[..32]);
        assert!(!ed25519_verify(&public, b"msg", &forged));
    }
}

#[test]
fn the_generator_is_deterministic_for_a_seed_and_differs_with_entropy_and_calls() {
    let mut a = Drbg::new(b"seed");
    let mut b = Drbg::new(b"seed");
    let (x, y) = (a.bytes64(), b.bytes64());
    assert_eq!(x, y, "same seed, same stream");
    assert_ne!(x, a.bytes64(), "successive outputs differ");
    let mut c = Drbg::new(b"seed2");
    assert_ne!(x, c.bytes64());
    a.add_entropy(b"more");
    b.add_entropy(b"other");
    assert_ne!(a.bytes64(), b.bytes64());
    // Output is not obviously biased: over 64 KiB each byte value appears about 256 times and bits are balanced.
    let mut g = Drbg::new(b"balance");
    let mut counts = [0u32; 256];
    let mut buf = [0u8; 1024];
    for _ in 0..64 {
        g.fill(&mut buf);
        for v in buf {
            counts[v as usize] += 1;
        }
    }
    assert!(
        counts.iter().all(|&n| (150..380).contains(&n)),
        "{counts:?}"
    );
    let ones: u32 = counts
        .iter()
        .enumerate()
        .map(|(v, n)| (v as u32).count_ones() * n)
        .sum();
    assert!(
        (ones as i64 - 262_144).abs() < 4000,
        "{ones} one bits of 524288"
    );
}
