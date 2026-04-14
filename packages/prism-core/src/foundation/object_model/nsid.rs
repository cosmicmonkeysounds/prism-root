//! NSID — Namespaced Identifiers for cross-Node type interoperability.
//! Port of `foundation/object-model/nsid.ts`.
//!
//! Format: `authority.segment1.segment2...` (AT Protocol lexicon
//! style). A valid NSID has at least three dot-separated segments
//! where each segment is lowercase alphanumeric + hyphens, starting
//! with a letter.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A validated NSID string. Constructed through [`Nsid::parse`] or
/// [`nsid`]; all other call sites should receive owned copies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Nsid(String);

/// A validated `prism://` object address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct PrismAddress(String);

#[derive(Debug, Error)]
pub enum NsidError {
    #[error("Invalid NSID: '{0}'. Must be reverse-DNS with 3+ segments, lowercase.")]
    Invalid(String),
}

impl Nsid {
    pub fn parse(s: impl Into<String>) -> Option<Self> {
        let s = s.into();
        if is_valid_nsid(&s) {
            Some(Self(s))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// First two dot-separated segments — the authority portion.
    pub fn authority(&self) -> String {
        self.0.split('.').take(2).collect::<Vec<_>>().join(".")
    }

    /// The last dot-separated segment.
    pub fn name(&self) -> &str {
        self.0.rsplit('.').next().unwrap_or("")
    }
}

impl std::fmt::Display for Nsid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Nsid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Nsid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Nsid::parse(s).ok_or_else(|| serde::de::Error::custom("invalid NSID"))
    }
}

impl PrismAddress {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PrismAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PrismAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if is_valid_prism_address(&s) {
            Ok(Self(s))
        } else {
            Err(serde::de::Error::custom("invalid Prism address"))
        }
    }
}

// ── Validation ──────────────────────────────────────────────────────

/// Validate an NSID string against the legacy regex
/// `^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*){2,}$` (authority + 2+
/// trailing segments = 3+ total).
pub fn is_valid_nsid(s: &str) -> bool {
    let mut segments = s.split('.');
    let mut count = 0;
    for seg in segments.by_ref() {
        count += 1;
        if seg.is_empty() {
            return false;
        }
        let mut chars = seg.chars();
        let first = chars.next().unwrap();
        if !first.is_ascii_lowercase() {
            return false;
        }
        for ch in chars {
            let ok = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-';
            if !ok {
                return false;
            }
        }
    }
    count >= 3
}

pub fn parse_nsid(s: &str) -> Option<Nsid> {
    Nsid::parse(s.to_string())
}

/// Create an NSID from parts. Panics if the joined value is not a
/// valid NSID — use [`Nsid::parse`] for fallible construction.
pub fn nsid<I, S>(parts: I) -> Nsid
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let joined = parts
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>()
        .join(".");
    Nsid::parse(joined.clone()).unwrap_or_else(|| {
        panic!("Invalid NSID: '{joined}'. Must be reverse-DNS with 3+ segments, lowercase.")
    })
}

pub fn nsid_authority(id: &Nsid) -> String {
    id.authority()
}

pub fn nsid_name(id: &Nsid) -> &str {
    id.name()
}

// ── Prism addresses ─────────────────────────────────────────────────

pub fn is_valid_prism_address(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("prism://") else {
        return false;
    };
    let Some(idx) = rest.find("/objects/") else {
        return false;
    };
    let node = &rest[..idx];
    let id = &rest[idx + "/objects/".len()..];
    if node.is_empty() || id.is_empty() {
        return false;
    }
    !node.contains('/')
}

pub fn prism_address(node_did: &str, obj: &str) -> PrismAddress {
    PrismAddress(format!("prism://{node_did}/objects/{obj}"))
}

pub fn parse_prism_address(addr: &str) -> Option<ParsedPrismAddress> {
    let rest = addr.strip_prefix("prism://")?;
    let idx = rest.find("/objects/")?;
    let node = rest[..idx].to_string();
    let object = rest[idx + "/objects/".len()..].to_string();
    if node.is_empty() || object.is_empty() || node.contains('/') {
        return None;
    }
    Some(ParsedPrismAddress {
        node_did: node,
        object_id: object,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPrismAddress {
    pub node_did: String,
    pub object_id: String,
}

// ── NSID Registry ───────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct NsidRegistry {
    nsid_to_type: HashMap<Nsid, String>,
    type_to_nsid: HashMap<String, Nsid>,
}

impl NsidRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        local_type: impl Into<String>,
        nsid_str: &str,
    ) -> Result<(), NsidError> {
        let id = parse_nsid(nsid_str).ok_or_else(|| NsidError::Invalid(nsid_str.to_string()))?;
        let local = local_type.into();
        self.nsid_to_type.insert(id.clone(), local.clone());
        self.type_to_nsid.insert(local, id);
        Ok(())
    }

    pub fn get_nsid(&self, local_type: &str) -> Option<&Nsid> {
        self.type_to_nsid.get(local_type)
    }

    pub fn get_local_type(&self, nsid: &Nsid) -> Option<&str> {
        self.nsid_to_type.get(nsid).map(|s| s.as_str())
    }

    pub fn has_nsid(&self, nsid: &Nsid) -> bool {
        self.nsid_to_type.contains_key(nsid)
    }

    pub fn entries(&self) -> &HashMap<Nsid, String> {
        &self.nsid_to_type
    }

    pub fn clear(&mut self) {
        self.nsid_to_type.clear();
        self.type_to_nsid.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_nsids() {
        assert!(is_valid_nsid("io.prismapp.productivity.task"));
        assert!(is_valid_nsid("com.mystudio.mygame.faction"));
        assert!(is_valid_nsid("io.prism-app.product.task"));
    }

    #[test]
    fn invalid_nsids() {
        assert!(!is_valid_nsid("io.prismapp")); // only 2 segments
        assert!(!is_valid_nsid("IO.prismapp.task")); // uppercase
        assert!(!is_valid_nsid("1io.prismapp.task")); // leading digit
        assert!(!is_valid_nsid("io..prismapp.task")); // empty segment
        assert!(!is_valid_nsid(""));
    }

    #[test]
    fn nsid_parts() {
        let id = nsid(["io.prismapp", "productivity", "task"]);
        assert_eq!(id.authority(), "io.prismapp");
        assert_eq!(id.name(), "task");
    }

    #[test]
    fn prism_address_roundtrip() {
        let addr = prism_address("did:web:node.example.com", "abc-123");
        assert_eq!(
            addr.as_str(),
            "prism://did:web:node.example.com/objects/abc-123"
        );
        let parsed = parse_prism_address(addr.as_str()).unwrap();
        assert_eq!(parsed.node_did, "did:web:node.example.com");
        assert_eq!(parsed.object_id, "abc-123");
    }

    #[test]
    fn nsid_registry_round_trips() {
        let mut reg = NsidRegistry::new();
        reg.register("task", "io.prismapp.productivity.task")
            .unwrap();
        let id = reg.get_nsid("task").unwrap().clone();
        assert_eq!(reg.get_local_type(&id), Some("task"));
        assert!(reg.has_nsid(&id));
        reg.clear();
        assert!(!reg.has_nsid(&id));
    }
}
