//! Per-notebook access keys, shared by the relay (`nb-relay`) and the app (`nb-ffi`).
//!
//! A shared notebook is a *group* of relay documents: `<group>` (the outline) and
//! `<group>-<page-id>` (pages). Access to a group is granted by a key:
//!
//! - a **write key** `w<hex>` (editors; whoever creates the notebook makes it);
//! - the **read key** `r<hex>` derived from it by [`read_key`] (read-only viewers, e.g. the
//!   students of a class). Holders of the write key can always derive the read key; holders
//!   of the read key cannot recover the write key.
//!
//! The relay stores only hashes of the two keys ([`key_hash`]). A share code is
//! `<group>.<key>`; codes without a key (older versions) still work on groups that never
//! had keys.
//!
//! ## End-to-end encrypted codes
//!
//! Codes whose key starts with `e` (editor) or `v` (viewer, derived from the editor key by
//! [`viewer_key`]) turn on end-to-end encryption. These secret keys **never reach the
//! relay**:
//!
//! - the relay sees only credentials derived from them ([`ShareCode::relay_key`]): `w…` for an
//!   editor, `r…` for a viewer, plus, when an editor claims the group, the hash of the viewer
//!   credential ([`ShareCode::relay_read_hash`]), which the relay cannot compute itself;
//! - both editors and viewers derive the same 256-bit content key from the viewer key
//!   ([`ShareCode::content_key`]); `nb-ffi` encrypts every update with it before upload.
//!
//! All derivations are SHA-256 with a distinct domain-separation prefix; the keys carry
//! 128 bits of randomness.

use sha2::{Digest, Sha256};

/// First bytes of an end-to-end encrypted update (`KHE1 ‖ nonce ‖ ciphertext ‖ tag`, see
/// `nb-ffi`). Loro updates start with `loro`, so the two never collide.
pub const SEALED_MAGIC: &[u8; 4] = b"KHE1";

/// Whether a relay payload is an end-to-end encrypted update.
pub fn is_sealed(update: &[u8]) -> bool {
    update.starts_with(SEALED_MAGIC)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Read,
    Write,
}

/// `w` or `r` followed by 16 to 64 lowercase hex digits.
pub fn key_role(key: &str) -> Option<Role> {
    let role = match key.as_bytes().first()? {
        b'w' => Role::Write,
        b'r' => Role::Read,
        _ => return None,
    };
    let hex = &key[1..];
    let ok = (16..=64).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    ok.then_some(role)
}

/// Role of a key in a share code: relay keys `w`/`r` and end-to-end keys `e`/`v`.
/// Returns the role and whether the key is an end-to-end key.
pub fn code_key_role(key: &str) -> Option<(Role, bool)> {
    let (rest, enc) = match key.as_bytes().first()? {
        b'e' => (format!("w{}", &key[1..]), true),
        b'v' => (format!("r{}", &key[1..]), true),
        _ => (key.to_string(), false),
    };
    Some((key_role(&rest)?, enc))
}

fn derive(domain: &str, key: &str) -> [u8; 32] {
    Sha256::digest(format!("{domain}:{key}").as_bytes()).into()
}

/// The viewer key that goes with an end-to-end editor key `e…` (`None` otherwise).
pub fn viewer_key(editor_key: &str) -> Option<String> {
    if code_key_role(editor_key)? != (Role::Write, true) {
        return None;
    }
    Some(format!("v{}", &hex(&derive("kahie-viewer-key", editor_key))[..32]))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hex SHA-256 of a key, as the relay stores it.
pub fn key_hash(key: &str) -> String {
    hex(&Sha256::digest(key.as_bytes()))
}

/// The read key that goes with a write key (`None` if `write_key` is not a write key).
pub fn read_key(write_key: &str) -> Option<String> {
    if key_role(write_key)? != Role::Write {
        return None;
    }
    let d = Sha256::digest(format!("kahie-read-key:{write_key}").as_bytes());
    Some(format!("r{}", &hex(&d)[..32]))
}

/// Document ids become file names: keep them boring.
pub fn valid_doc_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The access group of a relay document: the part before the first `-`.
pub fn doc_group(doc_id: &str) -> &str {
    doc_id.split('-').next().unwrap_or(doc_id)
}

/// A parsed share code `<group>[.<key>]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareCode {
    pub group: String,
    /// Empty for codes without a key.
    pub key: String,
}

impl ShareCode {
    pub fn parse(code: &str) -> Option<Self> {
        let code = code.trim();
        let (group, key) = match code.split_once('.') {
            Some((g, k)) => (g, Some(k)),
            None => (code, None),
        };
        let group_ok = valid_doc_id(group) && !group.contains('-') && group.len() <= 40;
        if !group_ok || key.is_some_and(|k| code_key_role(k).is_none()) {
            return None;
        }
        Some(Self {
            group: group.into(),
            key: key.unwrap_or("").into(),
        })
    }

    /// Role this code gives (codes without a key: write, on groups that have no keys).
    pub fn role(&self) -> Role {
        code_key_role(&self.key).map_or(Role::Write, |(r, _)| r)
    }

    /// Whether updates are end-to-end encrypted (keys `e…`/`v…`).
    pub fn encrypted(&self) -> bool {
        code_key_role(&self.key).is_some_and(|(_, e)| e)
    }

    /// The read-only code for the same notebook (`None` unless this is a write code).
    pub fn read_only(&self) -> Option<Self> {
        let key = if self.encrypted() {
            viewer_key(&self.key)?
        } else {
            read_key(&self.key)?
        };
        Some(Self {
            group: self.group.clone(),
            key,
        })
    }

    /// The key to present to the relay (`X-Kahie-Key`): the key itself for plain codes, a
    /// credential derived from it for end-to-end codes (empty for codes without a key).
    pub fn relay_key(&self) -> String {
        match code_key_role(&self.key) {
            Some((role, true)) => {
                let prefix = if role == Role::Write { 'w' } else { 'r' };
                format!("{prefix}{}", &hex(&derive("kahie-relay-auth", &self.key))[..32])
            }
            _ => self.key.clone(),
        }
    }

    /// For end-to-end editor codes: hash of the viewers' relay credential, sent when
    /// claiming the group (`X-Kahie-Read-Hash`), since the relay cannot derive it.
    pub fn relay_read_hash(&self) -> Option<String> {
        if !(self.encrypted() && self.role() == Role::Write) {
            return None;
        }
        Some(key_hash(&self.read_only()?.relay_key()))
    }

    /// The 256-bit content-encryption key of an end-to-end code (same for editors and
    /// viewers); `None` for plain codes.
    pub fn content_key(&self) -> Option<[u8; 32]> {
        if !self.encrypted() {
            return None;
        }
        let viewer = match self.role() {
            Role::Write => viewer_key(&self.key)?,
            Role::Read => self.key.clone(),
        };
        Some(derive("kahie-content-key", &viewer))
    }

    pub fn to_code(&self) -> String {
        if self.key.is_empty() {
            self.group.clone()
        } else {
            format!("{}.{}", self.group, self.key)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_roles_and_codes() {
        let w = "w0123456789abcdef0123456789abcdef";
        assert_eq!(key_role(w), Some(Role::Write));
        let r = read_key(w).unwrap();
        assert_eq!(key_role(&r), Some(Role::Read));
        assert_eq!(read_key(&r), None, "a read key gives nothing more");
        assert_eq!(r, read_key(w).unwrap(), "deterministic");
        assert_ne!(key_hash(w), key_hash(&r));
        for bad in [
            "",
            "w",
            "x0123456789abcdef",
            "w0123",
            "wABCDEF0123456789",
            "w0123456789abcdeg",
        ] {
            assert_eq!(key_role(bad), None, "{bad}");
        }

        let c = ShareCode::parse(&format!("nabc123.{w}")).unwrap();
        assert_eq!((c.group.as_str(), c.role()), ("nabc123", Role::Write));
        let ro = c.read_only().unwrap();
        assert_eq!(ro.role(), Role::Read);
        assert_eq!(ShareCode::parse(&ro.to_code()), Some(ro.clone()));
        assert!(ro.read_only().is_none());
        let legacy = ShareCode::parse("n0123abc").unwrap();
        assert_eq!(
            (legacy.key.as_str(), legacy.role(), legacy.to_code()),
            ("", Role::Write, "n0123abc".into())
        );
        for bad in ["", "a-b", "a/b", "nabc.", "nabc.zz", "nabc.w12"] {
            assert!(ShareCode::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(doc_group("nabc-p123"), "nabc");
        assert_eq!(c.relay_key(), w, "plain codes present their key");
        assert!(!c.encrypted() && c.content_key().is_none() && c.relay_read_hash().is_none());
    }

    #[test]
    fn end_to_end_codes() {
        let e = "e0123456789abcdef0123456789abcdef";
        let c = ShareCode::parse(&format!("nabc.{e}")).unwrap();
        assert!(c.encrypted() && c.role() == Role::Write);
        let v = c.read_only().unwrap();
        assert!(v.encrypted() && v.role() == Role::Read && v.key.starts_with('v'));
        assert!(v.read_only().is_none());
        assert_eq!(ShareCode::parse(&v.to_code()), Some(v.clone()));
        // Same content key for editors and viewers; different from anything the relay sees.
        let k = c.content_key().unwrap();
        assert_eq!(v.content_key(), Some(k));
        let (wa, ra) = (c.relay_key(), v.relay_key());
        assert_eq!(key_role(&wa), Some(Role::Write));
        assert_eq!(key_role(&ra), Some(Role::Read));
        for seen in [&wa, &ra] {
            assert!(!seen.contains(&e[1..]) && !seen.contains(&v.key[1..]));
        }
        assert_eq!(c.relay_read_hash(), Some(key_hash(&ra)));
        assert_eq!(v.relay_read_hash(), None);
        // Another editor key gives another content key.
        let c2 = ShareCode::parse("nabc.e1123456789abcdef0123456789abcdef").unwrap();
        assert_ne!(c2.content_key(), Some(k));
        assert_eq!(doc_group("nabc"), "nabc");
    }
}
