use std::path::{Path, PathBuf};

use crate::domain::{
    BackupRepositoryStatus, CompleteArchiveSummary, RevisionRetention, RevisionStorageUsage,
};
use crate::storage::SettingsStore;

use super::errors::{DataAccessError, DataResult};

pub(crate) const DEFAULT_TRASH_RETENTION_DAYS: u32 =
    crate::storage::DEFAULT_TRASH_RETENTION_DAYS;
pub(crate) const DEFAULT_AUTO_FIRST_LINE_INDENT_POINTS: u32 = 18;
pub(crate) const MIN_AUTO_FIRST_LINE_INDENT_POINTS: u32 = 6;
pub(crate) const AUTO_FIRST_LINE_INDENT_STEP_POINTS: u32 = 6;

pub(crate) fn max_auto_first_line_indent_points() -> u32 {
    u32::try_from(crate::editor_document_spec::limit("paragraph_first_line_indent_max"))
        .unwrap_or(u32::MAX)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorLineSpacing {
    One,
    OneFifteen,
    OneFive,
    OneSeventyFive,
    Two,
}

impl EditorLineSpacing {
    pub(crate) const ALL: [Self; 5] = [
        Self::One,
        Self::OneFifteen,
        Self::OneFive,
        Self::OneSeventyFive,
        Self::Two,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::OneFifteen => "1.15",
            Self::OneFive => "1.5",
            Self::OneSeventyFive => "1.75",
            Self::Two => "2",
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::One => "1.0",
            Self::OneFifteen => "1.15",
            Self::OneFive => "1.5",
            Self::OneSeventyFive => "1.75",
            Self::Two => "2.0",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "1" => Some(Self::One),
            "1.15" => Some(Self::OneFifteen),
            "1.5" => Some(Self::OneFive),
            "1.75" => Some(Self::OneSeventyFive),
            "2" => Some(Self::Two),
            _ => None,
        }
    }
}

/// User preferences, diagnostics, backup/history policy, and configured library locations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SettingsService {
    storage: SettingsStore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EditorPreferences {
    pub(crate) auto_indent: bool,
    pub(crate) auto_first_line_indent: u32,
    pub(crate) default_line_spacing: Option<EditorLineSpacing>,
    pub(crate) spellcheck: bool,
}

impl EditorPreferences {
    pub(crate) fn normalized(mut self) -> Self {
        self.auto_first_line_indent = self.auto_first_line_indent.clamp(
            MIN_AUTO_FIRST_LINE_INDENT_POINTS,
            max_auto_first_line_indent_points(),
        );
        self
    }
}

impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            auto_indent: false,
            auto_first_line_indent: DEFAULT_AUTO_FIRST_LINE_INDENT_POINTS,
            default_line_spacing: None,
            spellcheck: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveStoragePaths {
    pub(crate) data_directory: PathBuf,
    pub(crate) attachments_directory: PathBuf,
    pub(crate) automatic_backup_directory: PathBuf,
    pub(crate) database_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StorageLocations {
    pub(crate) data_directory: PathBuf,
    pub(crate) backup_directory: PathBuf,
}

impl From<crate::storage::StorageLocations> for StorageLocations {
    fn from(value: crate::storage::StorageLocations) -> Self {
        Self {
            data_directory: value.data_directory,
            backup_directory: value.backup_directory,
        }
    }
}

impl SettingsService {
    pub(crate) fn new(storage: SettingsStore) -> Self {
        Self { storage }
    }

    pub(crate) fn expected_data_directory() -> PathBuf {
        SettingsStore::expected_data_directory()
    }

    pub(crate) fn configured_locations() -> DataResult<StorageLocations> {
        SettingsStore::configured_locations()
            .map(StorageLocations::from)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn default_locations() -> StorageLocations {
        SettingsStore::default_locations().into()
    }

    pub(crate) fn save_configured_locations(
        data_directory: PathBuf,
        backup_directory: PathBuf,
    ) -> DataResult<StorageLocations> {
        SettingsStore::save_configured_locations(data_directory, backup_directory)
            .map(StorageLocations::from)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn reset_configured_locations() -> DataResult<()> {
        SettingsStore::reset_configured_locations().map_err(DataAccessError::from)
    }

    pub(crate) fn active_paths(&self) -> ActiveStoragePaths {
        ActiveStoragePaths {
            data_directory: self.storage.data_directory().to_path_buf(),
            attachments_directory: self.storage.attachments_directory().to_path_buf(),
            automatic_backup_directory: self.storage.automatic_backup_directory().to_path_buf(),
            database_path: self.storage.database_path().to_path_buf(),
        }
    }

    pub(crate) fn default_archive_path(&self) -> PathBuf {
        self.storage.default_archive_path()
    }

    pub(crate) fn editor_preferences(&self) -> DataResult<EditorPreferences> {
        self.storage
            .editor_preferences()
            .map(|(auto_indent, auto_first_line_indent, default_line_spacing, spellcheck)| {
                EditorPreferences {
                    auto_indent,
                    auto_first_line_indent: auto_first_line_indent
                        .unwrap_or(DEFAULT_AUTO_FIRST_LINE_INDENT_POINTS),
                    default_line_spacing: default_line_spacing
                        .as_deref()
                        .and_then(EditorLineSpacing::parse),
                    spellcheck,
                }
                .normalized()
            })
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_editor_preferences(&self, preferences: EditorPreferences) -> DataResult<()> {
        let preferences = preferences.normalized();
        self.storage
            .set_editor_preferences(
                preferences.auto_indent,
                preferences.auto_first_line_indent,
                preferences.default_line_spacing.map(EditorLineSpacing::as_str),
                preferences.spellcheck,
            )
            .map_err(DataAccessError::from)
    }

    pub(crate) fn dark_mode(&self) -> DataResult<bool> {
        self.storage.dark_mode().map_err(DataAccessError::from)
    }

    pub(crate) fn set_dark_mode(&self, enabled: bool) -> DataResult<()> {
        self.storage.set_dark_mode(enabled).map_err(DataAccessError::from)
    }

    pub(crate) fn layout_widths(&self) -> DataResult<(i32, i32)> {
        self.storage.layout_widths().map_err(DataAccessError::from)
    }

    pub(crate) fn set_layout_widths(&self, sidebar: i32, note_list: i32) -> DataResult<()> {
        self.storage
            .set_layout_widths(sidebar, note_list)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn right_panel_width(&self) -> DataResult<i32> {
        self.storage.right_panel_width().map_err(DataAccessError::from)
    }

    pub(crate) fn set_right_panel_width(&self, width: i32) -> DataResult<()> {
        self.storage
            .set_right_panel_width(width)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn schema_version(&self) -> DataResult<i64> {
        self.storage.schema_version().map_err(DataAccessError::from)
    }

    pub(crate) fn build_diagnostics(&self) -> DataResult<(Option<String>, Option<String>)> {
        self.storage.build_diagnostics().map_err(DataAccessError::from)
    }

    pub(crate) fn revision_retention(&self) -> DataResult<RevisionRetention> {
        self.storage.revision_retention().map_err(DataAccessError::from)
    }

    pub(crate) fn revision_storage_usage(&self) -> DataResult<RevisionStorageUsage> {
        self.storage
            .revision_storage_usage()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_revision_retention(
        &self,
        max_revisions_per_note: i64,
        max_age_days: i64,
    ) -> DataResult<usize> {
        self.storage
            .set_revision_retention(max_revisions_per_note, max_age_days)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_revision_storage_limit(&self, max_total_bytes: i64) -> DataResult<usize> {
        self.storage
            .set_revision_storage_limit(max_total_bytes)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn prune_revision_history(&self) -> DataResult<usize> {
        self.storage
            .prune_revision_history()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn trash_retention_days(&self) -> DataResult<Option<u32>> {
        self.storage
            .trash_retention_days()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_trash_retention_days(&self, days: Option<u32>) -> DataResult<usize> {
        self.storage
            .set_trash_retention_days(days)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn backup_repository_status(&self) -> DataResult<BackupRepositoryStatus> {
        self.storage
            .backup_repository_status()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_backup_repository_budget_bytes(
        &self,
        bytes: u64,
    ) -> DataResult<BackupRepositoryStatus> {
        self.storage
            .set_backup_repository_budget_bytes(bytes)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn inspect_backup_repository_snapshot(
        &self,
        snapshot_id: &str,
    ) -> DataResult<CompleteArchiveSummary> {
        self.storage
            .inspect_backup_repository_snapshot(snapshot_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn inspect_complete_archive(
        &self,
        path: &Path,
    ) -> DataResult<CompleteArchiveSummary> {
        self.storage
            .inspect_complete_archive(path)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn restore_complete_archive(
        &self,
        path: &Path,
    ) -> DataResult<CompleteArchiveSummary> {
        self.storage
            .restore_complete_archive(path)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn restore_backup_repository_snapshot(
        &self,
        snapshot_id: &str,
    ) -> DataResult<CompleteArchiveSummary> {
        self.storage
            .restore_backup_repository_snapshot(snapshot_id)
            .map_err(DataAccessError::from)
    }
}


#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_AUTO_FIRST_LINE_INDENT_POINTS, EditorLineSpacing, EditorPreferences,
        MIN_AUTO_FIRST_LINE_INDENT_POINTS, max_auto_first_line_indent_points,
    };

    #[test]
    fn editor_preferences_have_stable_writing_defaults() {
        let preferences = EditorPreferences::default();
        assert!(!preferences.auto_indent);
        assert_eq!(
            preferences.auto_first_line_indent,
            DEFAULT_AUTO_FIRST_LINE_INDENT_POINTS
        );
        assert_eq!(preferences.default_line_spacing, None);
        assert!(preferences.spellcheck);
    }

    #[test]
    fn editor_line_spacing_values_round_trip_through_persistence_strings() {
        for spacing in EditorLineSpacing::ALL {
            assert_eq!(EditorLineSpacing::parse(spacing.as_str()), Some(spacing));
        }
        assert_eq!(EditorLineSpacing::parse("auto"), None);
        assert_eq!(EditorLineSpacing::parse("1.25"), None);
    }

    #[test]
    fn editor_preferences_clamp_auto_indent_amount_to_supported_bounds() {
        let below = EditorPreferences {
            auto_first_line_indent: 0,
            ..EditorPreferences::default()
        }
        .normalized();
        assert_eq!(
            below.auto_first_line_indent,
            MIN_AUTO_FIRST_LINE_INDENT_POINTS
        );

        let above = EditorPreferences {
            auto_first_line_indent: u32::MAX,
            ..EditorPreferences::default()
        }
        .normalized();
        assert_eq!(
            above.auto_first_line_indent,
            max_auto_first_line_indent_points()
        );
    }
}
