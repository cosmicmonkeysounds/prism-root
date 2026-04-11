//! Crypto module — X25519 ECDH + XChaCha20-Poly1305 AEAD behind five
//! `crypto.*` commands.
//!
//! SPEC motivation: the Prism sync layer is end-to-end encrypted between
//! trusted peers (Mesh Trust) and from client to relay (Blind Ping). The
//! primitives the SPEC names are libsodium-equivalents: X25519 key
//! exchange plus XChaCha20-Poly1305 symmetric encryption. This module
//! exposes exactly that set to every host (desktop, mobile, browser)
//! through the same transport-agnostic command registry.
//!
//! We use pure-Rust RustCrypto crates rather than `libsodium-sys` so the
//! library compiles cleanly on every target we care about (wasm32, iOS,
//! Android) without pulling in a C dependency. The wire format is
//! identical — an X25519 public key is 32 bytes, a shared secret is 32
//! bytes, a ChaCha20-Poly1305 tag is 16 bytes, an XChaCha nonce is 24
//! bytes — so a libsodium-based host could interoperate with us if
//! anyone ever needed to.
//!
//! | Command               | Payload                                                | Result                                |
//! |-----------------------|--------------------------------------------------------|---------------------------------------|
//! | `crypto.keypair`      | `{}`                                                   | `{ secret_key, public_key }`          |
//! | `crypto.derive_public`| `{ secret_key }`                                       | `{ public_key }`                      |
//! | `crypto.shared_secret`| `{ secret_key, peer_public_key }`                      | `{ shared_secret }`                   |
//! | `crypto.encrypt`      | `{ key, plaintext, associated_data? }`                 | `{ ciphertext, nonce }`               |
//! | `crypto.decrypt`      | `{ key, ciphertext, nonce, associated_data? }`         | `{ plaintext }`                       |
//! | `crypto.random_bytes` | `{ len }`                                              | `{ bytes }`                           |
//!
//! Every byte field on the wire is lowercase hex (matching the rest of
//! the daemon — VFS hashes use the same encoding). Hex is chatty vs.
//! base64 but it's unambiguous across every JSON parser and plays
//! nicely with the emscripten C ABI which would otherwise have to
//! choose a base64 flavor.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use x25519_dalek::{PublicKey, StaticSecret};

/// The maximum number of bytes a single `crypto.random_bytes` call can
/// request. Bounded so a runaway caller can't OOM the daemon by asking
/// for a gigabyte at once — 64 KiB is enough for session keys, nonces,
/// and any salts the caller might want.
const MAX_RANDOM_BYTES: usize = 64 * 1024;

/// The crypto module. Stateless — all key material is passed in by the
/// caller. This keeps the module safe to carry on every platform
/// (including WASM) without having to reason about where a long-lived
/// keystore would live.
pub struct CryptoModule;

impl DaemonModule for CryptoModule {
    fn id(&self) -> &str {
        "prism.crypto"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let registry = builder.registry().clone();

        registry.register("crypto.keypair", |_payload| {
            let secret = StaticSecret::random_from_rng(OsRng);
            let public = PublicKey::from(&secret);
            Ok(json!({
                "secret_key": hex::encode(secret.to_bytes()),
                "public_key": hex::encode(public.as_bytes()),
            }))
        })?;

        registry.register("crypto.derive_public", |payload| {
            let args: SecretKeyArgs = parse(payload, "crypto.derive_public")?;
            let secret_bytes =
                decode_fixed::<32>(&args.secret_key, "secret_key", "crypto.derive_public")?;
            let secret = StaticSecret::from(secret_bytes);
            let public = PublicKey::from(&secret);
            Ok(json!({ "public_key": hex::encode(public.as_bytes()) }))
        })?;

        registry.register("crypto.shared_secret", |payload| {
            let args: SharedSecretArgs = parse(payload, "crypto.shared_secret")?;
            let secret_bytes =
                decode_fixed::<32>(&args.secret_key, "secret_key", "crypto.shared_secret")?;
            let peer_bytes = decode_fixed::<32>(
                &args.peer_public_key,
                "peer_public_key",
                "crypto.shared_secret",
            )?;
            let secret = StaticSecret::from(secret_bytes);
            let peer = PublicKey::from(peer_bytes);
            let shared = secret.diffie_hellman(&peer);
            Ok(json!({ "shared_secret": hex::encode(shared.as_bytes()) }))
        })?;

        registry.register("crypto.encrypt", |payload| {
            let args: EncryptArgs = parse(payload, "crypto.encrypt")?;
            let key_bytes = decode_fixed::<32>(&args.key, "key", "crypto.encrypt")?;
            let plaintext = decode_hex(&args.plaintext, "plaintext", "crypto.encrypt")?;
            let aad = match args.associated_data.as_deref() {
                Some(hex_str) => decode_hex(hex_str, "associated_data", "crypto.encrypt")?,
                None => Vec::new(),
            };
            let cipher = XChaCha20Poly1305::new((&key_bytes).into());
            let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
            let ct = cipher
                .encrypt(
                    &nonce,
                    Payload {
                        msg: &plaintext,
                        aad: &aad,
                    },
                )
                .map_err(|e| {
                    CommandError::handler("crypto.encrypt", format!("aead encrypt: {e}"))
                })?;
            Ok(json!({
                "ciphertext": hex::encode(ct),
                "nonce": hex::encode(nonce.as_slice()),
            }))
        })?;

        registry.register("crypto.decrypt", |payload| {
            let args: DecryptArgs = parse(payload, "crypto.decrypt")?;
            let key_bytes = decode_fixed::<32>(&args.key, "key", "crypto.decrypt")?;
            let nonce_bytes = decode_fixed::<24>(&args.nonce, "nonce", "crypto.decrypt")?;
            let ciphertext = decode_hex(&args.ciphertext, "ciphertext", "crypto.decrypt")?;
            let aad = match args.associated_data.as_deref() {
                Some(hex_str) => decode_hex(hex_str, "associated_data", "crypto.decrypt")?,
                None => Vec::new(),
            };
            let cipher = XChaCha20Poly1305::new((&key_bytes).into());
            let nonce = XNonce::from(nonce_bytes);
            let pt = cipher
                .decrypt(
                    &nonce,
                    Payload {
                        msg: &ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|e| {
                    CommandError::handler("crypto.decrypt", format!("aead decrypt: {e}"))
                })?;
            Ok(json!({ "plaintext": hex::encode(pt) }))
        })?;

        registry.register("crypto.random_bytes", |payload| {
            let args: RandomBytesArgs = parse(payload, "crypto.random_bytes")?;
            if args.len == 0 {
                return Err(CommandError::handler(
                    "crypto.random_bytes",
                    "len must be >= 1",
                ));
            }
            if args.len > MAX_RANDOM_BYTES {
                return Err(CommandError::handler(
                    "crypto.random_bytes",
                    format!("len must be <= {MAX_RANDOM_BYTES}"),
                ));
            }
            let mut buf = vec![0u8; args.len];
            OsRng.fill_bytes(&mut buf);
            Ok(json!({ "bytes": hex::encode(buf) }))
        })?;

        Ok(())
    }
}

// ── JSON arg shapes ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SecretKeyArgs {
    secret_key: String,
}

#[derive(Debug, Deserialize)]
struct SharedSecretArgs {
    secret_key: String,
    peer_public_key: String,
}

#[derive(Debug, Deserialize)]
struct EncryptArgs {
    key: String,
    plaintext: String,
    #[serde(default)]
    associated_data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DecryptArgs {
    key: String,
    ciphertext: String,
    nonce: String,
    #[serde(default)]
    associated_data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RandomBytesArgs {
    len: usize,
}

// ── Helpers ────────────────────────────────────────────────────────────

fn parse<T: for<'de> Deserialize<'de>>(
    payload: JsonValue,
    command: &'static str,
) -> Result<T, CommandError> {
    serde_json::from_value::<T>(payload).map_err(|e| CommandError::handler(command, e.to_string()))
}

fn decode_hex(s: &str, field: &str, command: &'static str) -> Result<Vec<u8>, CommandError> {
    hex::decode(s)
        .map_err(|e| CommandError::handler(command, format!("invalid hex in {field}: {e}")))
}

fn decode_fixed<const N: usize>(
    s: &str,
    field: &str,
    command: &'static str,
) -> Result<[u8; N], CommandError> {
    let bytes = decode_hex(s, field, command)?;
    if bytes.len() != N {
        return Err(CommandError::handler(
            command,
            format!("{field} must be {N} bytes (got {})", bytes.len()),
        ));
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DaemonBuilder;

    fn kernel() -> crate::DaemonKernel {
        DaemonBuilder::new()
            .with_module(CryptoModule)
            .build()
            .unwrap()
    }

    #[test]
    fn registers_every_command() {
        let k = kernel();
        let caps = k.capabilities();
        for name in [
            "crypto.keypair",
            "crypto.derive_public",
            "crypto.shared_secret",
            "crypto.encrypt",
            "crypto.decrypt",
            "crypto.random_bytes",
        ] {
            assert!(caps.contains(&name.to_string()), "missing {name}");
        }
    }

    #[test]
    fn keypair_returns_32_byte_hex_fields() {
        let k = kernel();
        let out = k.invoke("crypto.keypair", json!({})).unwrap();
        let sk = out["secret_key"].as_str().unwrap();
        let pk = out["public_key"].as_str().unwrap();
        assert_eq!(sk.len(), 64, "secret_key hex len");
        assert_eq!(pk.len(), 64, "public_key hex len");
        assert!(sk.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(pk.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn derive_public_matches_keypair_public() {
        let k = kernel();
        let kp = k.invoke("crypto.keypair", json!({})).unwrap();
        let derived = k
            .invoke(
                "crypto.derive_public",
                json!({ "secret_key": kp["secret_key"] }),
            )
            .unwrap();
        assert_eq!(derived["public_key"], kp["public_key"]);
    }

    #[test]
    fn ecdh_shared_secret_is_symmetric() {
        let k = kernel();
        let alice = k.invoke("crypto.keypair", json!({})).unwrap();
        let bob = k.invoke("crypto.keypair", json!({})).unwrap();

        let ab = k
            .invoke(
                "crypto.shared_secret",
                json!({
                    "secret_key": alice["secret_key"],
                    "peer_public_key": bob["public_key"],
                }),
            )
            .unwrap();
        let ba = k
            .invoke(
                "crypto.shared_secret",
                json!({
                    "secret_key": bob["secret_key"],
                    "peer_public_key": alice["public_key"],
                }),
            )
            .unwrap();

        assert_eq!(ab["shared_secret"], ba["shared_secret"]);
        assert_eq!(ab["shared_secret"].as_str().unwrap().len(), 64);
    }

    #[test]
    fn encrypt_then_decrypt_roundtrips_plaintext() {
        let k = kernel();
        let key = hex::encode([7u8; 32]);
        let plaintext = hex::encode(b"the fnord is in the details");

        let out = k
            .invoke(
                "crypto.encrypt",
                json!({ "key": key, "plaintext": plaintext }),
            )
            .unwrap();
        assert_eq!(out["nonce"].as_str().unwrap().len(), 48); // 24 bytes * 2 hex

        let back = k
            .invoke(
                "crypto.decrypt",
                json!({
                    "key": key,
                    "ciphertext": out["ciphertext"],
                    "nonce": out["nonce"],
                }),
            )
            .unwrap();
        assert_eq!(back["plaintext"], plaintext);
    }

    #[test]
    fn encrypt_nonces_are_unique_per_call() {
        let k = kernel();
        let key = hex::encode([3u8; 32]);
        let plaintext = hex::encode(b"same plaintext both times");
        let a = k
            .invoke(
                "crypto.encrypt",
                json!({ "key": key, "plaintext": plaintext }),
            )
            .unwrap();
        let b = k
            .invoke(
                "crypto.encrypt",
                json!({ "key": key, "plaintext": plaintext }),
            )
            .unwrap();
        assert_ne!(a["nonce"], b["nonce"]);
        assert_ne!(a["ciphertext"], b["ciphertext"]);
    }

    #[test]
    fn decrypt_rejects_tampered_ciphertext() {
        let k = kernel();
        let key = hex::encode([1u8; 32]);
        let out = k
            .invoke(
                "crypto.encrypt",
                json!({ "key": key, "plaintext": hex::encode(b"authenticated") }),
            )
            .unwrap();
        // Flip one byte of the ciphertext hex.
        let mut ct = out["ciphertext"].as_str().unwrap().to_string();
        ct.replace_range(0..2, "ff");

        let err = k
            .invoke(
                "crypto.decrypt",
                json!({
                    "key": key,
                    "ciphertext": ct,
                    "nonce": out["nonce"],
                }),
            )
            .unwrap_err();
        matches!(err, CommandError::Handler { .. });
    }

    #[test]
    fn associated_data_is_authenticated() {
        let k = kernel();
        let key = hex::encode([2u8; 32]);
        let out = k
            .invoke(
                "crypto.encrypt",
                json!({
                    "key": key,
                    "plaintext": hex::encode(b"with aad"),
                    "associated_data": hex::encode(b"context-tag"),
                }),
            )
            .unwrap();

        // Correct aad round-trips.
        let good = k
            .invoke(
                "crypto.decrypt",
                json!({
                    "key": key,
                    "ciphertext": out["ciphertext"],
                    "nonce": out["nonce"],
                    "associated_data": hex::encode(b"context-tag"),
                }),
            )
            .unwrap();
        assert_eq!(good["plaintext"], hex::encode(b"with aad"));

        // Wrong aad is rejected.
        let err = k
            .invoke(
                "crypto.decrypt",
                json!({
                    "key": key,
                    "ciphertext": out["ciphertext"],
                    "nonce": out["nonce"],
                    "associated_data": hex::encode(b"different-tag"),
                }),
            )
            .unwrap_err();
        matches!(err, CommandError::Handler { .. });
    }

    #[test]
    fn invalid_hex_in_key_is_rejected() {
        let k = kernel();
        let err = k
            .invoke("crypto.encrypt", json!({ "key": "zz", "plaintext": "00" }))
            .unwrap_err();
        if let CommandError::Handler { message, .. } = err {
            assert!(message.contains("invalid hex") || message.contains("must be"));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn wrong_length_key_is_rejected() {
        let k = kernel();
        let err = k
            .invoke(
                "crypto.encrypt",
                json!({ "key": hex::encode([0u8; 8]), "plaintext": "00" }),
            )
            .unwrap_err();
        if let CommandError::Handler { message, .. } = err {
            assert!(message.contains("must be 32 bytes"));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn random_bytes_returns_requested_length() {
        let k = kernel();
        let out = k
            .invoke("crypto.random_bytes", json!({ "len": 32 }))
            .unwrap();
        let hex_str = out["bytes"].as_str().unwrap();
        assert_eq!(hex_str.len(), 64);
        assert!(hex_str.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn random_bytes_rejects_zero_and_oversized_requests() {
        let k = kernel();
        let zero = k
            .invoke("crypto.random_bytes", json!({ "len": 0 }))
            .unwrap_err();
        assert!(matches!(zero, CommandError::Handler { .. }));

        let huge = k
            .invoke("crypto.random_bytes", json!({ "len": 10_000_000 }))
            .unwrap_err();
        assert!(matches!(huge, CommandError::Handler { .. }));
    }

    #[test]
    fn random_bytes_draws_are_distinct() {
        let k = kernel();
        let a = k
            .invoke("crypto.random_bytes", json!({ "len": 16 }))
            .unwrap();
        let b = k
            .invoke("crypto.random_bytes", json!({ "len": 16 }))
            .unwrap();
        assert_ne!(a["bytes"], b["bytes"]);
    }

    #[test]
    fn end_to_end_ecdh_then_aead_mimics_libsodium_box() {
        // Two peers derive a shared secret and use it as a symmetric key
        // for XChaCha20-Poly1305. This is the basic "cryptobox" flow the
        // SPEC prescribes for Mesh Trust.
        let k = kernel();
        let alice = k.invoke("crypto.keypair", json!({})).unwrap();
        let bob = k.invoke("crypto.keypair", json!({})).unwrap();

        let shared = k
            .invoke(
                "crypto.shared_secret",
                json!({
                    "secret_key": alice["secret_key"],
                    "peer_public_key": bob["public_key"],
                }),
            )
            .unwrap();

        let ct = k
            .invoke(
                "crypto.encrypt",
                json!({
                    "key": shared["shared_secret"],
                    "plaintext": hex::encode(b"hello from alice"),
                }),
            )
            .unwrap();

        // Bob derives the same shared secret and decrypts.
        let shared2 = k
            .invoke(
                "crypto.shared_secret",
                json!({
                    "secret_key": bob["secret_key"],
                    "peer_public_key": alice["public_key"],
                }),
            )
            .unwrap();
        let pt = k
            .invoke(
                "crypto.decrypt",
                json!({
                    "key": shared2["shared_secret"],
                    "ciphertext": ct["ciphertext"],
                    "nonce": ct["nonce"],
                }),
            )
            .unwrap();
        assert_eq!(pt["plaintext"], hex::encode(b"hello from alice"));
    }
}
