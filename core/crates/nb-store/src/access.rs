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

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        if !group_ok || key.is_some_and(|k| key_role(k).is_none()) {
            return None;
        }
        Some(Self {
            group: group.into(),
            key: key.unwrap_or("").into(),
        })
    }

    /// Role this code gives (codes without a key: write, on groups that have no keys).
    pub fn role(&self) -> Role {
        key_role(&self.key).unwrap_or(Role::Write)
    }

    /// The read-only code for the same notebook (`None` unless this is a write code).
    pub fn read_only(&self) -> Option<Self> {
        Some(Self {
            group: self.group.clone(),
            key: read_key(&self.key)?,
        })
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
        assert_eq!(doc_group("nabc"), "nabc");
    }
}
