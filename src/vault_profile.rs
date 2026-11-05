//! Opt-in timing telemetry for Vault operations.
//!
//! Profiling is disabled unless `RUSTNOTE_VAULT_PROFILE=1` is present in the
//! process environment. Labels are closed enums so profiling can never include
//! note identifiers, titles, paths, passwords, hashes, byte counts, or other
//! user-derived values. The disabled path performs only a cached boolean check;
//! timestamps and formatting are skipped entirely.

use std::{sync::OnceLock, time::Instant};

const VAULT_PROFILE_ENV: &str = "RUSTNOTE_VAULT_PROFILE";
static VAULT_PROFILE_ENABLED: OnceLock<bool> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VaultProfileOperation {
    Protect,
    Lock,
    Unlock,
    RemoveEncryption,
}

impl VaultProfileOperation {
    const fn label(self) -> &'static str {
        match self {
            Self::Protect => "protect",
            Self::Lock => "lock",
            Self::Unlock => "unlock",
            Self::RemoveEncryption => "remove_encryption",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VaultProfileStage {
    UiCheckpoint,
    AutosaveBarrier,
    AutosaveRelease,
    PrivacyDrains,
    ProtectPreparation,
    Crypto,
    AttachmentsEncrypt,
    AttachmentsStage,
    AttachmentsCleanup,
    ManagedResidueScrub,
    CacheCleanup,
    StagingCleanup,
    HandoffCleanup,
    BackupRepositoryReset,
    SqliteResidueScrub,
    TransitionFinalize,
    AttachmentsDecrypt,
    AttachmentsFinalize,
    ObjectWrites,
    Fts,
    Wal,
    CanonicalPublication,
}

impl VaultProfileStage {
    const fn label(self) -> &'static str {
        match self {
            Self::UiCheckpoint => "ui_checkpoint",
            Self::AutosaveBarrier => "autosave_barrier",
            Self::AutosaveRelease => "autosave_release",
            Self::PrivacyDrains => "privacy_drains",
            Self::ProtectPreparation => "protect_preparation",
            Self::Crypto => "crypto",
            Self::AttachmentsEncrypt => "attachments_encrypt",
            Self::AttachmentsStage => "attachments_stage",
            Self::AttachmentsCleanup => "attachments_cleanup",
            Self::ManagedResidueScrub => "managed_residue_scrub",
            Self::CacheCleanup => "cache_cleanup",
            Self::StagingCleanup => "staging_cleanup",
            Self::HandoffCleanup => "handoff_cleanup",
            Self::BackupRepositoryReset => "backup_repository_reset",
            Self::SqliteResidueScrub => "sqlite_residue_scrub",
            Self::TransitionFinalize => "transition_finalize",
            Self::AttachmentsDecrypt => "attachments_decrypt",
            Self::AttachmentsFinalize => "attachments_finalize",
            Self::ObjectWrites => "object_writes",
            Self::Fts => "fts",
            Self::Wal => "wal",
            Self::CanonicalPublication => "canonical_publication",
        }
    }
}

#[derive(Clone, Copy)]
enum VaultProfileKind {
    Operation,
    Stage,
}

impl VaultProfileKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Operation => "operation",
            Self::Stage => "stage",
        }
    }
}

pub(crate) struct VaultProfileSpan {
    kind: VaultProfileKind,
    name: &'static str,
    started_at: Option<Instant>,
}

impl VaultProfileSpan {
    #[inline]
    pub(crate) fn operation(operation: VaultProfileOperation) -> Self {
        Self::new(VaultProfileKind::Operation, operation.label())
    }

    #[inline]
    pub(crate) fn stage(stage: VaultProfileStage) -> Self {
        Self::new(VaultProfileKind::Stage, stage.label())
    }

    #[inline]
    fn new(kind: VaultProfileKind, name: &'static str) -> Self {
        Self {
            kind,
            name,
            started_at: vault_profile_enabled().then(Instant::now),
        }
    }
}

impl Drop for VaultProfileSpan {
    fn drop(&mut self) {
        let Some(started_at) = self.started_at else {
            return;
        };
        eprintln!(
            "[rustnote-vault-profile] kind={} name={} elapsed_us={}",
            self.kind.label(),
            self.name,
            started_at.elapsed().as_micros(),
        );
    }
}

#[inline]
fn vault_profile_enabled() -> bool {
    *VAULT_PROFILE_ENABLED.get_or_init(|| {
        std::env::var(VAULT_PROFILE_ENV)
            .ok()
            .as_deref()
            .is_some_and(profile_value_enables)
    })
}

fn profile_value_enables(value: &str) -> bool {
    value == "1"
}

#[cfg(test)]
mod tests {
    use super::{VaultProfileOperation, VaultProfileStage, profile_value_enables};

    #[test]
    fn profiling_requires_exact_opt_in_value() {
        assert!(profile_value_enables("1"));
        assert!(!profile_value_enables(""));
        assert!(!profile_value_enables("0"));
        assert!(!profile_value_enables("true"));
        assert!(!profile_value_enables(" 1"));
    }

    #[test]
    fn profiling_vocabulary_is_static_and_sensitive_value_free() {
        let operations = [
            VaultProfileOperation::Protect,
            VaultProfileOperation::Lock,
            VaultProfileOperation::Unlock,
            VaultProfileOperation::RemoveEncryption,
        ];
        assert_eq!(
            operations.map(VaultProfileOperation::label),
            ["protect", "lock", "unlock", "remove_encryption"]
        );

        let stages = [
            VaultProfileStage::UiCheckpoint,
            VaultProfileStage::AutosaveBarrier,
            VaultProfileStage::AutosaveRelease,
            VaultProfileStage::PrivacyDrains,
            VaultProfileStage::ProtectPreparation,
            VaultProfileStage::Crypto,
            VaultProfileStage::AttachmentsEncrypt,
            VaultProfileStage::AttachmentsStage,
            VaultProfileStage::AttachmentsCleanup,
            VaultProfileStage::ManagedResidueScrub,
            VaultProfileStage::CacheCleanup,
            VaultProfileStage::StagingCleanup,
            VaultProfileStage::HandoffCleanup,
            VaultProfileStage::BackupRepositoryReset,
            VaultProfileStage::SqliteResidueScrub,
            VaultProfileStage::TransitionFinalize,
            VaultProfileStage::AttachmentsDecrypt,
            VaultProfileStage::AttachmentsFinalize,
            VaultProfileStage::ObjectWrites,
            VaultProfileStage::Fts,
            VaultProfileStage::Wal,
            VaultProfileStage::CanonicalPublication,
        ];
        assert_eq!(
            stages.map(VaultProfileStage::label),
            [
                "ui_checkpoint",
                "autosave_barrier",
                "autosave_release",
                "privacy_drains",
                "protect_preparation",
                "crypto",
                "attachments_encrypt",
                "attachments_stage",
                "attachments_cleanup",
                "managed_residue_scrub",
                "cache_cleanup",
                "staging_cleanup",
                "handoff_cleanup",
                "backup_repository_reset",
                "sqlite_residue_scrub",
                "transition_finalize",
                "attachments_decrypt",
                "attachments_finalize",
                "object_writes",
                "fts",
                "wal",
                "canonical_publication",
            ]
        );
    }
}
