//! Application-facing security boundary.
//!
//! Rustnote Vault boundary. Privacy transitions and Rustnote-managed sensitive
//! plaintext lifetime are centralized here; storage/features consume the typed authority.

// Temporary staged authority; consumed by the next backup-storage pass.
#[expect(
    dead_code,
    reason = "staged backup key authority awaits encrypted backup repository integration"
)]
mod backup_key_authority;
mod derivative;
mod plaintext;
mod privacy_coordinator;
mod runtime;
pub(crate) mod sensitive_data;
pub(crate) mod vault_object_spec;
pub(crate) use derivative::PlaintextDerivativePermit;
#[cfg(test)]
pub(crate) use plaintext::PlaintextLifecycle;
pub(crate) use plaintext::{DurablePlaintextNamespace, OwnedPlaintext, managed_path_is_present};
pub(crate) use privacy_coordinator::{
    LibraryPrivacyMutationGuard, LibraryPrivacyTransition, MultiNotePlaintextOperationPermit,
    MultiNotePrivacyMutationGuard, NotePrivacyMutationGuard, NotePrivacyTransition,
    PlaintextDerivativeCleanupGuard, PlaintextOperationMutationGuard, PlaintextOperationPermit,
    PrivacyCoordinator, PrivacyPublicationAuthorization, PrivacyPublicationLease,
    ProtectedNoteMutationGuard, ProtectedNoteRestoreGuard, ProtectedNotesMutationGuard,
    RetainedProtectedNoteMutationGuard, WholeLibraryOperationLease,
};
pub(crate) use runtime::{
    VaultNoteAccess, VaultPlaintextPath, VaultReadPermit, is_vault_runtime_path,
};

pub use rustnote_vault::{
    Argon2idParams, DEFAULT_PROTECTED_BLOB_CHUNK_BYTES, MAX_PROTECTED_BLOB_CHUNK_BYTES,
    MIN_PROTECTED_BLOB_CHUNK_BYTES, ProtectedBlobInfo, VaultContext, VaultDomain, VaultError,
    VaultKey, VaultService,
};

/// Rust-owned password/secret text that zeroizes its owned bytes on drop.
///
/// Browser input widgets necessarily hold their own transient text while the
/// user is typing. Rustnote moves submitted passwords into this type at the
/// first Rust-side boundary so application-owned copies do not linger as
/// ordinary `String` allocations.
pub(crate) struct SecretString(Vec<u8>);

impl SecretString {
    pub(crate) fn take_from(value: &mut String) -> Self {
        Self(std::mem::take(value).into_bytes())
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.fill(0);
        // `String`/`Vec` capacity can exceed its current length after editing.
        // Clear spare allocation bytes too so Rust-owned password remnants do
        // not survive merely because the user shortened the value before submit.
        for byte in self.0.spare_capacity_mut() {
            byte.write(0);
        }
    }
}

/// Replace Rust-owned password input text after actively wiping the previous
/// allocation. Browser controls necessarily retain their own transient copies;
/// this limits Rustnote's additional copies to the currently displayed value.
pub(crate) fn replace_secret_input(target: &mut String, replacement: String) {
    clear_secret_input(target);
    *target = replacement;
}

pub(crate) fn clear_secret_input(target: &mut String) {
    let mut bytes = std::mem::take(target).into_bytes();
    bytes.fill(0);
    for byte in bytes.spare_capacity_mut() {
        byte.write(0);
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}
