//! # Message content checksums
//!
//! A [`MessageSnapshot`](crate::MessageSnapshot) carries a `checksum`: a
//! deterministic, content-only digest of a single traced message. It exists
//! for content integrity, dedup, and as a stable reference downstream
//! consumers can rely on. One message, one digest — this is **not** an
//! event-chain or tamper-evident log, and checksums are never chained across
//! events.
//!
//! ## Algorithm and encoding
//!
//! SHA-256, stored algorithm-prefixed as `"sha256:<64 lowercase hex>"`. The
//! prefix makes the scheme self-describing for any verifier and leaves room to
//! migrate later without breaking the `String` wire type.
//!
//! ## Hash domain — content only
//!
//! The digest is computed over the **message value and nothing else**. Event
//! metadata — `event_id`, `timestamp`, `span_id`, `actor_id`, port names,
//! sizes — is explicitly excluded. Two identical payloads sent at different
//! times by different actors therefore produce the same checksum, which is
//! what makes dedup and content-identity work.
//!
//! ## Canonical form (so the digest is reproducible everywhere)
//!
//! Determinism is the load-bearing requirement: the checksum must be identical
//! across processes, hosts, CPU architectures, and SDK languages. That requires
//! a documented *canonical serialization*, not the in-memory layout. We use
//! **RFC 8785-style canonical JSON (JCS)**, realized as follows — these rules
//! are written as prose so any independent verifier can re-derive the exact
//! bytes that get hashed:
//!
//! 1. Serialize the message value to JSON via its serde representation. For
//!    Reflow's `Message` that is the externally-tagged form
//!    `{"type": <variant>, "data": <payload>}` (the `data` key is absent for
//!    unit variants such as `Flow`).
//! 2. **Object keys are sorted** in ascending order. (Implementation note: we
//!    sort by Rust `str` ordering, i.e. Unicode scalar / UTF-8 byte order. This
//!    matches RFC 8785's UTF-16 code-unit ordering for every character in the
//!    Basic Multilingual Plane; the two differ only for supplementary-plane
//!    characters (≥ U+10000) appearing *in object keys*, which Reflow message
//!    field names never use.)
//! 3. **No insignificant whitespace** — the compact JSON form.
//! 4. **Strings** use standard JSON escaping (RFC 8259): `"` and `\` are
//!    escaped, C0 control characters are emitted as `\u00xx`.
//! 5. **Numbers**: integers are emitted exactly; finite floating-point values
//!    use the shortest round-tripping decimal, with exponential notation in the
//!    ECMAScript style (`1e+21`, `1e-7`). Non-finite floats (NaN/±Inf) are not
//!    representable in JSON and serialize to `null`.
//! 6. The hash input is the UTF-8 bytes of the resulting string.
//!
//! The digest is always computed over the **decompressed** content, so toggling
//! transport/storage compression never changes the checksum.
//!
//! ## Edge cases
//!
//! Signal-only and empty payloads (`Message::Flow`, `Optional(None)`,
//! zero-byte) hash their canonical typed representation and so yield a stable,
//! well-defined value — never an empty string. Callers should reserve the
//! empty string strictly for "not computed".

use serde::Serialize;
use sha2::{Digest, Sha256};

/// The canonical-form prefix; bump only if the algorithm changes.
pub const CHECKSUM_PREFIX: &str = "sha256:";

/// Canonical JSON (JCS-style) bytes for a serializable value — the exact bytes
/// the checksum is computed over. Routing through `serde_json::Value` is what
/// gives us sorted object keys (its map is a `BTreeMap`); the compact
/// serializer gives us no insignificant whitespace.
pub fn canonical_json<T: Serialize>(content: &T) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(content)?;
    serde_json::to_string(&value)
}

/// Content checksum of a serializable message value, encoded
/// `"sha256:<64 lowercase hex>"`.
///
/// Infallible: a value that cannot be represented as JSON (which, for Reflow
/// `Message`s, does not occur — non-finite floats serialize to `null`) falls
/// back to hashing a stable typed marker rather than producing an empty or
/// non-deterministic result.
pub fn content_checksum<T: Serialize>(content: &T) -> String {
    match canonical_json(content) {
        Ok(canonical) => checksum_of_canonical(&canonical),
        Err(_) => digest(b"reflow:uncanonicalizable"),
    }
}

/// Checksum of an already-canonical JSON string. Use this when you have already
/// computed the canonical form (e.g. to also measure its size or capture it as
/// content) to avoid canonicalizing twice.
pub fn checksum_of_canonical(canonical: &str) -> String {
    digest(canonical.as_bytes())
}

/// `"sha256:<hex>"` digest of raw bytes.
fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{}{}", CHECKSUM_PREFIX, hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checksum_is_prefixed_64_hex() {
        let c = content_checksum(&json!({"a": 1}));
        assert!(c.starts_with("sha256:"));
        assert_eq!(c.len(), "sha256:".len() + 64);
        assert!(c["sha256:".len()..]
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));
    }

    #[test]
    fn checksum_is_content_only_and_order_independent() {
        // Same content, different key insertion order -> same digest.
        let a = content_checksum(&json!({"x": 1, "y": 2}));
        let b = content_checksum(&json!({"y": 2, "x": 1}));
        assert_eq!(a, b, "object key order must not affect the checksum");
    }

    #[test]
    fn distinct_content_distinct_checksum() {
        assert_ne!(
            content_checksum(&json!({"x": 1})),
            content_checksum(&json!({"x": 2}))
        );
    }

    #[test]
    fn empty_and_signal_values_have_stable_nonempty_digests() {
        let flow = content_checksum(&json!(null));
        assert!(flow.starts_with("sha256:"));
        assert_ne!(flow, "");
        // Stable across calls.
        assert_eq!(flow, content_checksum(&json!(null)));
    }

    #[test]
    fn canonical_json_sorts_nested_keys_compactly() {
        let canonical = canonical_json(&json!({"b": 1, "a": {"d": 1, "c": 2}})).unwrap();
        assert_eq!(canonical, r#"{"a":{"c":2,"d":1},"b":1}"#);
    }

    #[test]
    fn known_vector_sha256_of_empty_object() {
        // Canonical form of {} is the two bytes "{}"; pin the digest so any
        // SDK re-implementation can check itself against this vector.
        assert_eq!(
            content_checksum(&json!({})),
            "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
    }
}
