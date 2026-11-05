use std::path::PathBuf;

use dioxus::prelude::*;

use crate::domain::{
    BackupRepositoryStatus, CompleteArchiveSummary, RevisionRetention, RevisionStorageUsage,
};
use crate::application::settings::{DEFAULT_TRASH_RETENTION_DAYS, SettingsService, StorageLocations};

#[derive(Clone)]
pub(super) struct InitialSettingsData {
    pub(super) retention: RevisionRetention,
    pub(super) trash_retention_days: Option<u32>,
    pub(super) storage_locations: StorageLocations,
    pub(super) schema_version: Option<i64>,
    pub(super) last_opened_version: Option<String>,
    pub(super) last_opened_build: Option<String>,
    pub(super) feedback: String,
}

pub(super) fn load_initial_settings(settings: &SettingsService) -> InitialSettingsData {
    let mut errors = Vec::new();
    let retention = match settings.revision_retention() {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("Could not load history retention: {error}"));
            RevisionRetention::default()
        }
    };
    let trash_retention_days = match settings.trash_retention_days() {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("Could not load Trash retention: {error}"));
            Some(DEFAULT_TRASH_RETENTION_DAYS)
        }
    };
    let storage_locations = SettingsService::configured_locations().unwrap_or_else(|error| {
        errors.push(format!("Could not load configured storage locations: {error}"));
        StorageLocations {
            data_directory: settings.active_paths().data_directory,
            backup_directory: settings.active_paths().automatic_backup_directory,
        }
    });
    let schema_version = match settings.schema_version() {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(format!("Could not read schema version: {error}"));
            None
        }
    };
    let (last_opened_version, last_opened_build) = match settings.build_diagnostics() {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("Could not read build diagnostics: {error}"));
            (None, None)
        }
    };
    InitialSettingsData {
        retention,
        trash_retention_days,
        storage_locations,
        schema_version,
        last_opened_version,
        last_opened_build,
        feedback: if errors.is_empty() {
            "Settings are stored locally on this device.".to_string()
        } else {
            errors.join(" ")
        },
    }
}


#[derive(Clone)]
pub(super) struct SettingsDiagnosticsData {
    pub(super) revision_usage: RevisionStorageUsage,
    pub(super) backup_status: BackupRepositoryStatus,
    pub(super) feedback: Option<String>,
}

pub(super) fn load_settings_diagnostics(settings: &SettingsService) -> SettingsDiagnosticsData {
    let mut errors = Vec::new();
    let revision_usage = match settings.revision_storage_usage() {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("Could not load history usage: {error}"));
            RevisionStorageUsage::default()
        }
    };
    let backup_status = match settings.backup_repository_status() {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("Could not load backup repository status: {error}"));
            BackupRepositoryStatus {
                budget_bytes: 10 * 1024 * 1024 * 1024,
                last_result: "Backup repository status is unavailable.".to_string(),
                ..BackupRepositoryStatus::default()
            }
        }
    };
    SettingsDiagnosticsData {
        revision_usage,
        backup_status,
        feedback: (!errors.is_empty()).then(|| errors.join(" ")),
    }
}

#[derive(Clone, PartialEq)]
pub(super) enum PendingRestoreSource {
    PortableArchive(PathBuf),
    RepositorySnapshot(String),
}

#[derive(Clone, PartialEq)]
pub(super) struct PendingRestore {
    pub(super) source: PendingRestoreSource,
    pub(super) summary: CompleteArchiveSummary,
}

#[component]
pub(super) fn SettingsHeading(title: &'static str, description: &'static str) -> Element {
    rsx! {
        header { class: "settings-section-heading",
            h3 { "{title}" }
            p { "{description}" }
        }
    }
}

#[component]
pub(super) fn SettingsAction(
    title: &'static str,
    description: &'static str,
    action: &'static str,
    #[props(default = false)] danger: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] busy: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "settings-action-card", aria_busy: busy,
            div {
                strong { "{title}" }
                p { "{description}" }
            }
            button {
                class: if danger { "secondary-settings-button danger" } else { "secondary-settings-button" },
                disabled: disabled,
                aria_busy: busy,
                onclick: move |event| onclick.call(event),
                if busy { "Working…" } else { "{action}" }
            }
        }
    }
}

#[component]
pub(super) fn ShortcutLine(keys: &'static str, action: &'static str) -> Element {
    rsx! {
        div { class: "shortcut-line",
            kbd { "{keys}" }
            span { "{action}" }
        }
    }
}
