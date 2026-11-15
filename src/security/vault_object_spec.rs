//! Shared 2b encrypted-object identity vocabulary.
//!
//! Object identifiers are opaque random values. They deliberately carry no
//! plaintext note/revision metadata; authenticated object context binds them to
//! the owning note, object kind and payload format when ciphertext is sealed.

use std::fmt;

use serde::{Deserialize, Serialize};

pub(crate) const VAULT_OBJECT_ID_BYTES: usize = 16;
pub(crate) const VAULT_OBJECT_ID_HEX_LEN: usize = VAULT_OBJECT_ID_BYTES * 2;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub(crate) struct VaultObjectId(String);

impl VaultObjectId {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.len() != VAULT_OBJECT_ID_HEX_LEN
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err("vault object identifier is invalid");
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for VaultObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for VaultObjectId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(<D::Error as serde::de::Error>::custom)
    }
}
