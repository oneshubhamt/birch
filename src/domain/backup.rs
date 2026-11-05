use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub path: String,
    pub byte_size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveManifest {
    pub format_version: u32,
    #[serde(default)]
    pub build_identifier: String,
    pub created_at: i64,
    pub database_sha256: String,
    pub entries: Vec<ArchiveEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompleteArchiveSummary {
    pub created_at: i64,
    pub archive_bytes: u64,
    pub expanded_bytes: u64,
    pub notebook_count: usize,
    pub note_count: usize,
    pub attachment_count: usize,
    pub revision_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupSnapshotKind {
    #[default]
    Automatic,
    Safety,
}

impl BackupSnapshotKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Safety => "Pre-restore safety",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupRepositorySnapshot {
    pub snapshot_id: String,
    pub kind: BackupSnapshotKind,
    pub created_at: i64,
    pub database_bytes: u64,
    pub expanded_bytes: u64,
    pub blob_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupRepositoryStatus {
    pub total_bytes: u64,
    pub blob_bytes: u64,
    pub database_bytes: u64,
    pub snapshot_count: usize,
    pub automatic_snapshot_count: usize,
    pub safety_snapshot_count: usize,
    pub oldest_snapshot_at: Option<i64>,
    pub newest_snapshot_at: Option<i64>,
    pub budget_bytes: u64,
    pub last_attempt_at: Option<i64>,
    pub last_result: String,
    pub quarantined_artifact_count: usize,
    pub quarantined_artifacts: Vec<String>,
    pub pending_privacy_reset_count: usize,
    pub snapshots: Vec<BackupRepositorySnapshot>,
}

