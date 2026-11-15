use std::path::PathBuf;

use dioxus::prelude::*;
use rfd::AsyncFileDialog;

use crate::application::context::ApplicationServices;
use crate::application::errors::{EditorError, ExportError};
use crate::application::persistence::unix_timestamp;
use crate::application::settings::{
    AUTO_FIRST_LINE_INDENT_STEP_POINTS, DEFAULT_TRASH_RETENTION_DAYS, EditorLineSpacing,
    EditorPreferences, MIN_AUTO_FIRST_LINE_INDENT_POINTS, SettingsService,
    max_auto_first_line_indent_points,
};
use crate::build_info::{APP_VERSION, BUILD_ID, FULL_BUILD_IDENTIFIER};
use crate::domain::{Note, Notebook, Section};

use super::editor_preferences::commit_editor_preferences;
use super::live_editor::live_editor_dispose_script;
use super::note_list_data::reload_note_list;
use super::note_metadata::format_file_size;
use super::settings_support::{
    PendingRestore, PendingRestoreSource, SettingsAction, SettingsHeading, ShortcutLine,
    load_initial_settings, load_settings_diagnostics,
};
use super::synchronization::{
    EditorSession, EditorSynchronizationContext, SynchronizationError, SynchronizationMode,
    synchronize_current_editor_scoped, synchronize_then_begin_whole_library_operation,
};
use super::{CompactPane, NotesListState, SettingsSection, refresh_notebooks, refresh_tags};
use crate::application::errors::DataAccessError;
use crate::application::executor::{
    ApplicationExecutionError, ApplicationExecutionStatus, ApplicationExecutor,
    ApplicationOperation, ExecutionFailure,
};

#[derive(Debug)]
enum SettingsExportError {
    Synchronization {
        context: &'static str,
        source: SynchronizationError,
    },
    Export(ExportError),
    Execution(ApplicationExecutionError),
}

impl SettingsExportError {
    fn synchronization(context: &'static str, source: SynchronizationError) -> Self {
        Self::Synchronization { context, source }
    }
}

impl std::fmt::Display for SettingsExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synchronization { context, source } => write!(formatter, "{context}: {source}"),
            Self::Export(error) => std::fmt::Display::fmt(error, formatter),
            Self::Execution(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for SettingsExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Synchronization { source, .. } => Some(source),
            Self::Export(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<ExportError> for SettingsExportError {
    fn from(error: ExportError) -> Self {
        Self::Export(error)
    }
}

impl From<ApplicationExecutionError> for SettingsExportError {
    fn from(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Debug)]
enum SettingsInspectionError {
    Synchronization(SynchronizationError),
    DataTask(super::task_error::DataTaskError),
}

impl std::fmt::Display for SettingsInspectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synchronization(error) => write!(
                formatter,
                "Could not finish saving before archive inspection: {error}"
            ),
            Self::DataTask(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for SettingsInspectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Synchronization(error) => Some(error),
            Self::DataTask(error) => Some(error),
        }
    }
}

#[derive(Debug)]
enum SettingsRestoreError {
    Editor(EditorError),
    Data(DataAccessError),
    Execution(ApplicationExecutionError),
    EditorDispose,
}

impl std::fmt::Display for SettingsRestoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Editor(error) => std::fmt::Display::fmt(error, formatter),
            Self::Data(error) => std::fmt::Display::fmt(error, formatter),
            Self::Execution(error) => std::fmt::Display::fmt(error, formatter),
            Self::EditorDispose => {
                formatter.write_str("Could not dispose the current editor before archive restore")
            }
        }
    }
}

impl std::error::Error for SettingsRestoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Editor(error) => Some(error),
            Self::Data(error) => Some(error),
            Self::Execution(error) => Some(error),
            Self::EditorDispose => None,
        }
    }
}

impl From<EditorError> for SettingsRestoreError {
    fn from(error: EditorError) -> Self {
        Self::Editor(error)
    }
}

impl From<DataAccessError> for SettingsRestoreError {
    fn from(error: DataAccessError) -> Self {
        Self::Data(error)
    }
}

impl ExecutionFailure for SettingsRestoreError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

fn format_backup_bytes(bytes: u64) -> String {
    format_file_size(i64::try_from(bytes).unwrap_or(i64::MAX))
}

fn format_backup_time(value: Option<i64>) -> String {
    let Some(timestamp) = value else {
        return "None yet".to_string();
    };
    let age = unix_timestamp().saturating_sub(timestamp);
    if age < 60 {
        "Just now".to_string()
    } else if age < 60 * 60 {
        format!("{} minute(s) ago", age / 60)
    } else if age < 24 * 60 * 60 {
        format!("{} hour(s) ago", age / (60 * 60))
    } else {
        format!("{} day(s) ago", age / (24 * 60 * 60))
    }
}

fn settings_operation(status: &ApplicationExecutionStatus) -> Option<ApplicationOperation> {
    status
        .current
        .into_iter()
        .chain(status.queued)
        .find(|operation| {
            matches!(
                operation,
                ApplicationOperation::ExportPortability
                    | ApplicationOperation::ImportPortability
                    | ApplicationOperation::UndoPortabilityImport
                    | ApplicationOperation::ExportArchive
                    | ApplicationOperation::InspectArchive
                    | ApplicationOperation::RestoreArchive
                    | ApplicationOperation::ManageBackups
                    | ApplicationOperation::SaveHistoryLimits
                    | ApplicationOperation::SaveTrashRetention
                    | ApplicationOperation::SaveStorageLocations
                    | ApplicationOperation::PruneHistory
            )
        })
}

fn reset_settings_content_scroll() {
    spawn(async move {
        let _ = document::eval(
            r#"
requestAnimationFrame(() => {
    requestAnimationFrame(() => {
        const content = document.getElementById('settings-content');
        if (content) content.scrollTop = 0;
    });
});
return true;
"#,
        )
        .join::<bool>()
        .await;
    });
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct SettingsOverlayView {
    pub services: Signal<ApplicationServices>,
    pub notebooks: Signal<Vec<Notebook>>,
    pub tags: Signal<Vec<String>>,
    pub library_state: NotesListState,
    pub compact_pane: Signal<CompactPane>,
    pub show_settings: Signal<bool>,
    pub show_shortcuts: Signal<bool>,
    pub dark_mode: Signal<bool>,
    pub appearance_save_generation: Signal<u64>,
}

#[component]
pub(super) fn SettingsOverlay(view: SettingsOverlayView) -> Element {
    let SettingsOverlayView {
        services,
        notebooks,
        tags,
        library_state,
        mut compact_pane,
        mut show_settings,
        mut show_shortcuts,
        mut dark_mode,
        mut appearance_save_generation,
    } = view;
    let privacy_epoch = library_state.privacy_epoch;
    let NotesListState {
        mut section,
        mut selected_notebook,
        mut search,
        mut selected_note,
        mut status,
        editor_session,
        list_generation,
        note_selection_generation,
        ..
    } = library_state;

    rsx! {
        SettingsPanel {
            view: SettingsProps {
                services: services.read().clone(),
                selected_note,
                editor_session,
                dark_mode_enabled: *dark_mode.read(),
                privacy_epoch,
                list_generation,
                note_selection_generation,
            },
            actions: SettingsActions {
                onthemechange: EventHandler::new(move |enabled: bool| {
                    dark_mode.set(enabled);
                    status.set(if enabled {
                        "Dark appearance enabled".to_string()
                    } else {
                        "Light appearance enabled".to_string()
                    });
                    let next_generation =
                        (*appearance_save_generation.read()).wrapping_add(1);
                    appearance_save_generation.set(next_generation);
                }),
                onclose: EventHandler::new(move |_| show_settings.set(false)),
                onrestored: EventHandler::new(move |_| {
                    if !refresh_notebooks(&services.read().library(), notebooks, status) {
                        return;
                    }
                    if !refresh_tags(&services.read().library(), tags, status) {
                        return;
                    }
                    selected_note.set(None);
                    compact_pane.set(CompactPane::Notes);
                    selected_notebook.set(None);
                    section.set(Section::Notes);
                    search.set(String::new());
                    reload_note_list(library_state);
                }),
                onopen_shortcuts: EventHandler::new(move |_| {
                    show_settings.set(false);
                    show_shortcuts.set(true);
                }),
                onstatus: EventHandler::new(move |message: String| status.set(message)),
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct SettingsProps {
    pub services: ApplicationServices,
    pub selected_note: Signal<Option<Note>>,
    pub editor_session: Signal<Option<EditorSession>>,
    pub dark_mode_enabled: bool,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub list_generation: Signal<u64>,
    pub note_selection_generation: Signal<u64>,
}

#[derive(Clone, PartialEq)]
pub(super) struct SettingsActions {
    pub onthemechange: EventHandler<bool>,
    pub onclose: EventHandler<()>,
    pub onrestored: EventHandler<()>,
    pub onopen_shortcuts: EventHandler<()>,
    pub onstatus: EventHandler<String>,
}

#[derive(Clone, Copy)]
struct StorageLocationSignals {
    data_directory: Signal<PathBuf>,
    backup_directory: Signal<PathBuf>,
    restart_required: Signal<bool>,
    feedback: Signal<String>,
}

fn persist_storage_locations(
    data_directory: PathBuf,
    backup_directory: PathBuf,
    active_settings: SettingsService,
    background: ApplicationExecutor,
    onstatus: EventHandler<String>,
    signals: StorageLocationSignals,
) {
    let operation = ApplicationOperation::SaveStorageLocations;
    let mut feedback = signals.feedback;
    feedback.set(operation.progress_label().to_string());
    onstatus.call(operation.progress_label().to_string());
    let active_paths = active_settings.active_paths();
    let active_data_directory = active_paths.data_directory;
    let active_backup_directory = active_paths.automatic_backup_directory;

    spawn(async move {
        let result = background
            .execute(operation, move || {
                SettingsService::save_configured_locations(data_directory, backup_directory)
                    .map_err(super::task_error::DataTaskError::from)
            })
            .await;
        match result {
            Ok(locations) => {
                let restart_required = locations.data_directory != active_data_directory
                    || locations.backup_directory != active_backup_directory;
                let mut configured_data_directory = signals.data_directory;
                let mut configured_backup_directory = signals.backup_directory;
                let mut restart_signal = signals.restart_required;
                configured_data_directory.set(locations.data_directory);
                configured_backup_directory.set(locations.backup_directory);
                restart_signal.set(restart_required);
                let message = if restart_required {
                    "Storage locations saved. Restart Rustnote to use them; existing files were not moved."
                } else {
                    "Storage locations saved."
                };
                feedback.set(message.to_string());
                onstatus.call(message.to_string());
            }
            Err(error) => {
                let message = format!("Could not save storage locations: {error}");
                feedback.set(message.clone());
                onstatus.call(message);
            }
        }
    });
}

#[component]
pub(super) fn SettingsPanel(view: SettingsProps, actions: SettingsActions) -> Element {
    let SettingsProps {
        services,
        mut selected_note,
        editor_session,
        dark_mode_enabled,
        privacy_epoch,
        list_generation,
        note_selection_generation,
    } = view;
    let SettingsActions {
        onthemechange,
        onclose,
        onrestored,
        onopen_shortcuts,
        onstatus,
    } = actions;
    let settings = services.settings();
    let notes = services.notes();
    let editor_service = services.editor();
    let export_service = services.export();
    let privacy_service = services.privacy();
    let privacy_operation_service = services.privacy_operations();
    let background_coordinator = use_context::<ApplicationExecutor>();
    let mut editor_preferences = use_context::<Signal<EditorPreferences>>();
    let export_archive_background = background_coordinator.clone();
    let inspect_archive_background = background_coordinator.clone();
    let inspect_repository_background = background_coordinator.clone();
    let restore_archive_background = background_coordinator.clone();
    let manage_backups_background = background_coordinator.clone();
    let save_limits_background = background_coordinator.clone();
    let save_trash_retention_background = background_coordinator.clone();
    let prune_history_background = background_coordinator.clone();
    let save_storage_locations_background = background_coordinator.clone();
    let editor_auto_indent_settings = settings.clone();
    let editor_indent_decrease_settings = settings.clone();
    let editor_indent_increase_settings = settings.clone();
    let editor_line_spacing_settings = settings.clone();
    let editor_spellcheck_settings = settings.clone();
    let mut active_section = use_signal(SettingsSection::default);
    let initial_settings_service = settings.clone();
    let initial_settings = use_signal(move || load_initial_settings(&initial_settings_service));
    let initial_settings_value = initial_settings.read().clone();
    let initial_revision_count = initial_settings_value.retention.max_revisions_per_note;
    let initial_revision_age = initial_settings_value.retention.max_age_days;
    let initial_revision_megabytes =
        initial_settings_value.retention.max_total_bytes / (1024 * 1024);
    let mut revision_count = use_signal(move || initial_revision_count.to_string());
    let mut revision_age = use_signal(move || initial_revision_age.to_string());
    let mut revision_megabytes = use_signal(move || initial_revision_megabytes.to_string());
    let initial_trash_retention = initial_settings_value.trash_retention_days;
    let mut trash_retention_enabled = use_signal(move || initial_trash_retention.is_some());
    let mut trash_retention_days = use_signal(move || {
        initial_trash_retention
            .unwrap_or(DEFAULT_TRASH_RETENTION_DAYS)
            .to_string()
    });
    let mut revision_usage = use_signal(crate::domain::RevisionStorageUsage::default);
    let mut backup_status = use_signal(|| crate::domain::BackupRepositoryStatus {
        budget_bytes: 10 * 1024 * 1024 * 1024,
        last_result: "Loading backup repository status…".to_string(),
        ..crate::domain::BackupRepositoryStatus::default()
    });
    let mut backup_budget_gib = use_signal(|| "10".to_string());
    let mut diagnostics_loading = use_signal(|| true);
    let mut diagnostics_feedback = use_signal(|| None::<String>);
    let initial_storage_locations = initial_settings_value.storage_locations.clone();
    let active_paths = settings.active_paths();
    let initial_restart_required = initial_storage_locations.data_directory
        != active_paths.data_directory
        || initial_storage_locations.backup_directory != active_paths.automatic_backup_directory;
    let initial_data_directory = initial_storage_locations.data_directory;
    let initial_backup_directory = initial_storage_locations.backup_directory;
    let configured_data_directory = use_signal(move || initial_data_directory);
    let configured_backup_directory = use_signal(move || initial_backup_directory);
    let storage_restart_required = use_signal(move || initial_restart_required);
    let initial_feedback = initial_settings_value.feedback;
    let mut feedback = use_signal(move || initial_feedback);
    let mut pending_restore = use_signal(|| None::<PendingRestore>);
    let execution_status = use_context::<Signal<ApplicationExecutionStatus>>();
    let mut file_dialog_active = use_signal(|| false);
    let execution_snapshot = execution_status.read().clone();
    let current_operation = settings_operation(&execution_snapshot);
    let operation_active = current_operation.is_some() || *file_dialog_active.read();
    let selected_settings_section = *active_section.read();
    let schema_version = initial_settings_value
        .schema_version
        .map(|value| value.to_string())
        .unwrap_or_else(|| "Unavailable".to_string());
    let last_opened_build = match (
        initial_settings_value.last_opened_version.as_deref(),
        initial_settings_value.last_opened_build.as_deref(),
    ) {
        (Some(version), Some(build)) => format!("{version}+{build}"),
        (Some(version), None) => version.to_string(),
        _ => "Unavailable".to_string(),
    };
    let settings_sections = [
        SettingsSection::Appearance,
        SettingsSection::Editor,
        SettingsSection::ImportExport,
        SettingsSection::Backups,
        SettingsSection::History,
        SettingsSection::Storage,
        SettingsSection::Shortcuts,
        SettingsSection::Diagnostics,
    ];
    let storage_location_signals = StorageLocationSignals {
        data_directory: configured_data_directory,
        backup_directory: configured_backup_directory,
        restart_required: storage_restart_required,
        feedback,
    };
    let library_location_background = save_storage_locations_background.clone();
    let backup_location_background = save_storage_locations_background.clone();
    let default_location_background = save_storage_locations_background.clone();
    let library_location_status = onstatus;
    let backup_location_status = onstatus;
    let default_location_status = onstatus;
    let library_location_settings = settings.clone();
    let backup_location_settings = settings.clone();
    let default_location_settings = settings.clone();
    let diagnostics_settings = settings.clone();
    let _settings_diagnostics_loader = use_future(move || {
        let task_settings = diagnostics_settings.clone();
        async move {
            let result = ApplicationExecutor::read("settings diagnostics", move || {
                Ok::<_, ApplicationExecutionError>(load_settings_diagnostics(&task_settings))
            })
            .await;
            diagnostics_loading.set(false);
            match result {
                Ok(data) => {
                    revision_usage.set(data.revision_usage);
                    let budget_gib =
                        (data.backup_status.budget_bytes / (1024 * 1024 * 1024)).max(1);
                    backup_budget_gib.set(budget_gib.to_string());
                    backup_status.set(data.backup_status);
                    diagnostics_feedback.set(data.feedback);
                }
                Err(error) => {
                    diagnostics_feedback
                        .set(Some(format!("Could not load storage diagnostics: {error}")));
                }
            }
        }
    });

    let diagnostics_are_loading = *diagnostics_loading.read();
    let diagnostics_message = diagnostics_feedback.read().clone();
    let quarantine_count = backup_status.read().quarantined_artifact_count;
    let quarantine_artifacts = backup_status.read().quarantined_artifacts.clone();
    let pending_backup_privacy_resets = backup_status.read().pending_privacy_reset_count;

    rsx! {
        div {
            class: "settings-backdrop",
            "data-transient-close": "settings-dialog",
            onclick: move |_| {
                if settings_operation(&execution_status.read()).is_none() && !*file_dialog_active.read() {
                    onclose.call(());
                }
            },
            section {
                id: "settings-dialog",
                class: "settings-window",
                role: "dialog",
                aria_modal: "true",
                aria_label: "Rustnote settings",
                aria_busy: operation_active,
                "data-transient-surface": "true",
                "data-transient-kind": "dialog",
                "data-dismiss-outside": if operation_active { "false" } else { "true" },
                "data-return-focus": "settings-trigger",
                "data-fallback-focus": "note-list-main note-editor-main",
                onclick: move |event| event.stop_propagation(),
                header { class: "settings-header",
                    div {
                        p { class: "eyebrow", "RUSTNOTE" }
                        h2 { "Settings" }
                        p { "Keep everyday note-taking simple; move administrative controls here." }
                    }
                    button {
                        class: "settings-close",
                        title: "Close settings",
                        aria_label: "Close settings",
                        "data-initial-focus": "true",
                        "data-transient-close": "settings-dialog",
                        disabled: operation_active,
                        onclick: move |_| {
                            if settings_operation(&execution_status.read()).is_none() && !*file_dialog_active.read() {
                                onclose.call(());
                            }
                        },
                        "×"
                    }
                }

                div { class: "settings-layout",
                    nav { class: "settings-nav", aria_label: "Settings sections",
                        for item in settings_sections {
                            button {
                                class: if selected_settings_section == item { "settings-nav-item active" } else { "settings-nav-item" },
                                aria_current: if selected_settings_section == item { "page" } else { "false" },
                                disabled: operation_active,
                                onclick: move |_| {
                                    active_section.set(item);
                                    reset_settings_content_scroll();
                                },
                                span { class: "settings-nav-symbol", aria_hidden: "true", "{item.symbol()}" }
                                span { "{item.label()}" }
                            }
                        }
                    }

                    main {
                        id: "settings-content",
                        class: "settings-content",
                        match selected_settings_section {
                            SettingsSection::Appearance => rsx! {
                                SettingsHeading {
                                    title: "Appearance",
                                    description: "Choose a calm reading environment. The choice is saved locally."
                                }
                                div { class: "appearance-grid",
                                    button {
                                        class: if !dark_mode_enabled { "appearance-card active" } else { "appearance-card" },
                                        aria_pressed: if !dark_mode_enabled { "true" } else { "false" },
                                        disabled: operation_active,
                                        onclick: {
                                            move |_| {
                                                onthemechange.call(false);
                                                feedback.set("Light appearance enabled.".to_string());
                                            }
                                        },
                                        div { class: "appearance-preview light-preview",
                                            span {}
                                            span {}
                                            span {}
                                        }
                                        strong { "Light" }
                                        small { "Bright paper-like workspace" }
                                    }
                                    button {
                                        class: if dark_mode_enabled { "appearance-card active" } else { "appearance-card" },
                                        aria_pressed: if dark_mode_enabled { "true" } else { "false" },
                                        disabled: operation_active,
                                        onclick: {
                                            move |_| {
                                                onthemechange.call(true);
                                                feedback.set("Dark appearance enabled.".to_string());
                                            }
                                        },
                                        div { class: "appearance-preview dark-preview",
                                            span {}
                                            span {}
                                            span {}
                                        }
                                        strong { "Dark" }
                                        small { "Low-glare workspace" }
                                    }
                                }
                            },
                            SettingsSection::Editor => rsx! {
                                SettingsHeading {
                                    title: "Editor",
                                    description: "Set lightweight writing defaults and editor preferences. Auto-formatted paragraphs follow these defaults; explicit paragraph formatting remains unchanged."
                                }
                                div { class: "settings-action-list",
                                    div { class: "settings-action-card",
                                        div {
                                            strong { "Auto-indent new paragraphs" }
                                            p { "When enabled, a new eligible paragraph receives the configured first-line indent. Manually removing the indent always wins." }
                                        }
                                        button {
                                            class: "secondary-settings-button",
                                            role: "switch",
                                            aria_checked: if editor_preferences.read().auto_indent { "true" } else { "false" },
                                            onclick: move |_| {
                                                let mut next = *editor_preferences.peek();
                                                next.auto_indent = !next.auto_indent;
                                                commit_editor_preferences(
                                                    &mut editor_preferences,
                                                    editor_auto_indent_settings.clone(),
                                                    next,
                                                    onstatus,
                                                );
                                            },
                                            if editor_preferences.read().auto_indent { "On" } else { "Off" }
                                        }
                                    }
                                    div { class: "settings-action-card",
                                        div {
                                            strong { "Default auto first-line indent" }
                                            p { "Used only when auto-indent creates a new paragraph and the source paragraph does not already have an explicit first-line indent." }
                                        }
                                        div { class: "settings-editor-stepper",
                                            button {
                                                class: "secondary-settings-button",
                                                aria_label: "Decrease default auto first-line indent",
                                                disabled: editor_preferences.read().auto_first_line_indent <= MIN_AUTO_FIRST_LINE_INDENT_POINTS,
                                                onclick: move |_| {
                                                    let mut next = *editor_preferences.peek();
                                                    next.auto_first_line_indent = next.auto_first_line_indent
                                                        .saturating_sub(AUTO_FIRST_LINE_INDENT_STEP_POINTS)
                                                        .max(MIN_AUTO_FIRST_LINE_INDENT_POINTS);
                                                    commit_editor_preferences(
                                                        &mut editor_preferences,
                                                        editor_indent_decrease_settings.clone(),
                                                        next,
                                                        onstatus,
                                                    );
                                                },
                                                "−"
                                            }
                                            output { "{editor_preferences.read().auto_first_line_indent} pt" }
                                            button {
                                                class: "secondary-settings-button",
                                                aria_label: "Increase default auto first-line indent",
                                                disabled: editor_preferences.read().auto_first_line_indent >= max_auto_first_line_indent_points(),
                                                onclick: move |_| {
                                                    let mut next = *editor_preferences.peek();
                                                    next.auto_first_line_indent = next.auto_first_line_indent
                                                        .saturating_add(AUTO_FIRST_LINE_INDENT_STEP_POINTS)
                                                        .min(max_auto_first_line_indent_points());
                                                    commit_editor_preferences(
                                                        &mut editor_preferences,
                                                        editor_indent_increase_settings.clone(),
                                                        next,
                                                        onstatus,
                                                    );
                                                },
                                                "+"
                                            }
                                        }
                                    }
                                    div { class: "settings-action-card",
                                        div {
                                            strong { "Default paragraph line spacing" }
                                            p { "Applied to newly created normal paragraphs when they do not already inherit explicit spacing. Auto leaves them on Rustnote's normal paragraph spacing." }
                                        }
                                        select {
                                            aria_label: "Default paragraph line spacing",
                                            value: editor_preferences.read().default_line_spacing.map(EditorLineSpacing::as_str).unwrap_or("auto"),
                                            onchange: move |event| {
                                                let mut next = *editor_preferences.peek();
                                                next.default_line_spacing = EditorLineSpacing::parse(&event.value());
                                                commit_editor_preferences(
                                                    &mut editor_preferences,
                                                    editor_line_spacing_settings.clone(),
                                                    next,
                                                    onstatus,
                                                );
                                            },
                                            option { value: "auto", "Auto" }
                                            for spacing in EditorLineSpacing::ALL {
                                                option { value: spacing.as_str(), "{spacing.label()}" }
                                            }
                                        }
                                    }
                                    div { class: "settings-action-card",
                                        div {
                                            strong { "Spellcheck" }
                                            p { "Use the platform spellchecker. Rustnote does not ship or run its own dictionary." }
                                        }
                                        button {
                                            class: "secondary-settings-button",
                                            role: "switch",
                                            aria_checked: if editor_preferences.read().spellcheck { "true" } else { "false" },
                                            onclick: move |_| {
                                                let mut next = *editor_preferences.peek();
                                                next.spellcheck = !next.spellcheck;
                                                commit_editor_preferences(
                                                    &mut editor_preferences,
                                                    editor_spellcheck_settings.clone(),
                                                    next,
                                                    onstatus,
                                                );
                                            },
                                            if editor_preferences.read().spellcheck { "On" } else { "Off" }
                                        }
                                    }
                                }
                                div { class: "shortcut-summary settings-editor-shortcuts",
                                    ShortcutLine { keys: "⌘/Ctrl V", action: "Paste with supported rich formatting" }
                                    ShortcutLine { keys: "⌘/Ctrl ⇧ V", action: "Paste as plain text" }
                                }
                                p { class: "settings-note", "Normal paste remains the default. Plain-text paste deliberately removes rich formatting while keeping the pasted text and line structure." }
                            },
                            SettingsSection::ImportExport => rsx! {
                                super::portability::PortabilitySettings {
                                    services: services.clone(),
                                    selected_note,
                                    editor_session,
                                    privacy_epoch,
                                    list_generation,
                                    note_selection_generation,
                                    disabled: operation_active,
                                    onimported: onrestored,
                                    onstatus,
                                }
                            },
                            SettingsSection::Backups => rsx! {
                                SettingsHeading {
                                    title: "Backups",
                                    description: "Recovery snapshots and complete-library disaster recovery."
                                }
                                div { class: "history-usage-card backup-repository-usage", aria_busy: diagnostics_are_loading,
                                    if diagnostics_are_loading {
                                        div {
                                            strong { "Loading…" }
                                            span { "Reading backup repository status" }
                                        }
                                    } else {
                                        div {
                                            strong { "{backup_status.read().snapshot_count} snapshots" }
                                            span { "{backup_status.read().automatic_snapshot_count} automatic · {backup_status.read().safety_snapshot_count} safety" }
                                        }
                                        div {
                                            strong { "{format_backup_bytes(backup_status.read().total_bytes)}" }
                                            span { "of {format_backup_bytes(backup_status.read().budget_bytes)} budget" }
                                        }
                                        div {
                                            strong { "{format_backup_bytes(backup_status.read().blob_bytes)}" }
                                            span { "deduplicated attachment blobs" }
                                        }
                                        div {
                                            strong { "{format_backup_time(backup_status.read().oldest_snapshot_at)}" }
                                            span { "oldest · newest {format_backup_time(backup_status.read().newest_snapshot_at)}" }
                                        }
                                    }
                                }
                                if !diagnostics_are_loading && pending_backup_privacy_resets > 0 {
                                    p { class: "settings-note",
                                        "Encryption is complete locally. {pending_backup_privacy_resets} offline or unavailable backup location(s) still require privacy invalidation. Rustnote will clean each repository before using it again."
                                    }
                                }
                                if !diagnostics_are_loading
                                    && quarantine_count > 0
                                {
                                    p { class: "settings-note danger-copy",
                                        "Rustnote preserved {quarantine_count} damaged or incomplete backup artifact(s) in quarantine. Details are available under Diagnostics."
                                    }
                                }
                                div { class: "settings-form-grid backup-budget-form",
                                    label {
                                        span { "Automatic-backup budget (GB)" }
                                        input {
                                            value: "{backup_budget_gib}",
                                            disabled: operation_active || diagnostics_are_loading,
                                            inputmode: "numeric",
                                            oninput: move |event| backup_budget_gib.set(event.value())
                                        }
                                        small { "1–1,024 GB; snapshot count and total size are both enforced" }
                                    }
                                }
                                div { class: "settings-button-row",
                                    button {
                                        class: "primary-settings-button",
                                        disabled: operation_active || diagnostics_are_loading,
                                        aria_busy: current_operation == Some(ApplicationOperation::ManageBackups),
                                        onclick: {
                                            let settings = settings.clone();
                                            move |_| {
                                                let value = backup_budget_gib.read().trim().parse::<u64>();
                                                let Ok(gib) = value else {
                                                    let message = "Backup budget must be a whole number of GB.".to_string();
                                                    feedback.set(message.clone());
                                                    onstatus.call(message);
                                                    return;
                                                };
                                                let Some(bytes) = gib.checked_mul(1024 * 1024 * 1024) else {
                                                    let message = "Backup budget is too large.".to_string();
                                                    feedback.set(message.clone());
                                                    onstatus.call(message);
                                                    return;
                                                };
                                                let operation = ApplicationOperation::ManageBackups;
                                                feedback.set(operation.progress_label().to_string());
                                                onstatus.call(operation.progress_label().to_string());
                                                let task_settings = settings.clone();
                                                let task_background = manage_backups_background.clone();
                                                spawn(async move {
                                                    let result = task_background.execute(operation, move || {
                                                        task_settings
                                                            .set_backup_repository_budget_bytes(bytes)
                                                            .map_err(super::task_error::DataTaskError::from)
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok(status_value) => {
                                                            backup_status.set(status_value);
                                                            feedback.set("Backup budget saved and retention applied.".to_string());
                                                            onstatus.call("Backup retention updated".to_string());
                                                        }
                                                        Err(error) => {
                                                            let message = format!("Could not update backup retention: {error}");
                                                            feedback.set(message.clone());
                                                            onstatus.call(message);
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        if current_operation == Some(ApplicationOperation::ManageBackups) {
                                            "Applying…"
                                        } else {
                                            "Save budget and prune"
                                        }
                                    }
                                }
                                p { class: "settings-note", "Last backup result: {backup_status.read().last_result}" }
                                if backup_status.read().snapshots.is_empty() {
                                    p { class: "settings-note", "No automatic or safety snapshots have been retained yet." }
                                } else {
                                    div { class: "backup-snapshot-list",
                                        for snapshot in backup_status.read().snapshots.clone() {
                                            div { class: "backup-snapshot-card",
                                                div {
                                                    strong { "{snapshot.kind.label()} snapshot" }
                                                    span { "{format_backup_time(Some(snapshot.created_at))} · {format_backup_bytes(snapshot.expanded_bytes)} expanded · {snapshot.blob_count} blob(s)" }
                                                }
                                                button {
                                                    class: "secondary-settings-button",
                                                    disabled: operation_active,
                                                    aria_busy: current_operation == Some(ApplicationOperation::InspectArchive),
                                                    onclick: {
                                                        let settings = settings.clone();
                                                        let snapshot_id = snapshot.snapshot_id.clone();
                                                        let snapshot_background = inspect_repository_background.clone();
                                                        move |_| {
                                                            let operation = ApplicationOperation::InspectArchive;
                                                            feedback.set("Verifying retained backup…".to_string());
                                                            onstatus.call("Verifying retained backup…".to_string());
                                                            let task_settings = settings.clone();
                                                            let task_snapshot_id = snapshot_id.clone();
                                                            let pending_snapshot_id = snapshot_id.clone();
                                                            let task_background = snapshot_background.clone();
                                                            spawn(async move {
                                                                let result = task_background.execute(operation, move || {
                                                                    task_settings
                                                                        .inspect_backup_repository_snapshot(&task_snapshot_id)
                                                                        .map_err(super::task_error::DataTaskError::from)
                                                                })
                                                                .await;
                                                                match result {
                                                                    Ok(summary) => {
                                                                        feedback.set("Backup verified. Review the summary before restoring.".to_string());
                                                                        pending_restore.set(Some(PendingRestore {
                                                                            source: PendingRestoreSource::RepositorySnapshot(pending_snapshot_id),
                                                                            summary,
                                                                        }));
                                                                    }
                                                                    Err(error) => {
                                                                        let message = format!("Backup verification failed: {error}");
                                                                        feedback.set(message.clone());
                                                                        onstatus.call(message);
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    },
                                                    "Verify & restore"
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "settings-action-list",
                                    SettingsAction {
                                        title: "Export complete archive",
                                        description: "Checksummed database, attachments, settings, and version history.",
                                        action: "Save archive…",
                                        disabled: operation_active,
                                        busy: current_operation == Some(ApplicationOperation::ExportArchive),
                                        onclick: {
                                            let settings = settings.clone();
                                            let editor_service = editor_service.clone();
                                            let export_service = export_service.clone();
                                            let privacy_service = privacy_service.clone();
                                            let privacy_operation_service = privacy_operation_service.clone();
                                            move |_| {
                                                if settings_operation(&execution_status.read()).is_some()
                                                    || *file_dialog_active.read()
                                                {
                                                    return;
                                                }
                                                file_dialog_active.set(true);
                                                let task_editor_service = editor_service.clone();
                                                let task_export_service = export_service.clone();
                                                let task_privacy = privacy_service.clone();
                                                let task_privacy_operations = privacy_operation_service.clone();
                                                let default_archive = settings.default_archive_path();
                                                let task_background = export_archive_background.clone();
                                                let mut task_dialog = file_dialog_active;
                                                spawn(async move {
                                                    let mut dialog = AsyncFileDialog::new()
                                                        .add_filter("Rustnote complete archive", &["zip"])
                                                        .set_file_name("rustnote-complete.rustnote.zip");
                                                    if let Some(parent) = default_archive.parent() {
                                                        dialog = dialog.set_directory(parent);
                                                    }
                                                    let selected = dialog.save_file().await;
                                                    task_dialog.set(false);
                                                    let Some(path) = selected
                                                        .map(|handle| handle.path().to_path_buf())
                                                    else {
                                                        return;
                                                    };
                                                    let operation = ApplicationOperation::ExportArchive;
                                                    feedback.set(operation.progress_label().to_string());
                                                    onstatus.call(operation.progress_label().to_string());
                                                    let synchronization = EditorSynchronizationContext::new(
                                                        task_editor_service,
                                                        selected_note,
                                                        editor_session,
                                                        privacy_epoch,
                                                        &task_privacy,
                                                        list_generation,
                                                        note_selection_generation,
                                                    );
                                                    let result = match task_background
                                                        .begin_operation(operation)
                                                        .await
                                                    {
                                                        Ok(operation_turn) => match synchronize_then_begin_whole_library_operation(
                                                            &operation_turn,
                                                            &synchronization,
                                                            &task_privacy_operations,
                                                            SynchronizationMode::Persist,
                                                        )
                                                        .await
                                                        {
                                                            Ok(archive_lease) => operation_turn
                                                                .execute(move || {
                                                                    task_export_service
                                                                        .clone()
                                                                        .export_archive(&archive_lease, &path)
                                                                        .map(|()| path)
                                                                })
                                                                .await
                                                                .map_err(SettingsExportError::from),
                                                            Err(error) => Err(
                                                                SettingsExportError::synchronization(
                                                                    "Could not finish saving before archive export",
                                                                    error,
                                                                ),
                                                            ),
                                                        },
                                                        Err(error) => Err(SettingsExportError::from(error)),
                                                    };
                                                    match result {
                                                        Ok(path) => {
                                                            feedback.set(format!(
                                                                "Complete archive exported to {}",
                                                                path.display()
                                                            ));
                                                            onstatus.call("Complete archive exported".to_string());
                                                        }
                                                        Err(error) => {
                                                            let message = format!("Archive export failed: {error}");
                                                            feedback.set(message.clone());
                                                            onstatus.call(message);
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                    }
                                    SettingsAction {
                                        title: "Restore complete archive",
                                        description: "Replace the local library after integrity verification and a safety backup.",
                                        action: "Choose archive…",
                                        danger: true,
                                        disabled: operation_active,
                                        busy: current_operation == Some(ApplicationOperation::InspectArchive),
                                        onclick: {
                                            let settings = settings.clone();
                                            let editor_service = editor_service.clone();
                                            let privacy_service = privacy_service.clone();
                                            move |_| {
                                                if settings_operation(&execution_status.read()).is_some()
                                                    || *file_dialog_active.read()
                                                {
                                                    return;
                                                }
                                                file_dialog_active.set(true);
                                                let task_editor_service = editor_service.clone();
                                                let task_privacy = privacy_service.clone();
                                                let default_archive = settings.default_archive_path();
                                                let task_settings = settings.clone();
                                                let task_background = inspect_archive_background.clone();
                                                let mut task_dialog = file_dialog_active;
                                                spawn(async move {
                                                    let mut dialog = AsyncFileDialog::new()
                                                        .add_filter("Rustnote complete archive", &["zip"]);
                                                    if let Some(parent) = default_archive.parent() {
                                                        dialog = dialog.set_directory(parent);
                                                    }
                                                    let selected = dialog.pick_file().await;
                                                    task_dialog.set(false);
                                                    let Some(path) = selected
                                                        .map(|handle| handle.path().to_path_buf())
                                                    else {
                                                        return;
                                                    };
                                                    let operation = ApplicationOperation::InspectArchive;
                                                    feedback.set(operation.progress_label().to_string());
                                                    onstatus.call(operation.progress_label().to_string());
                                                    let inspected_path = path.clone();
                                                    let synchronization = EditorSynchronizationContext::new(
                                                        task_editor_service,
                                                        selected_note,
                                                        editor_session,
                                                        privacy_epoch,
                                                        &task_privacy,
                                                        list_generation,
                                                        note_selection_generation,
                                                    );
                                                    let result = match synchronize_current_editor_scoped(
                                                        &synchronization,
                                                        SynchronizationMode::Persist,
                                                    )
                                                    .await
                                                    {
                                                        Ok(Some(_)) => task_background.execute(operation, move || {
                                                            task_settings
                                                                .inspect_complete_archive(&inspected_path)
                                                                .map_err(super::task_error::DataTaskError::from)
                                                        })
                                                        .await
                                                        .map_err(SettingsInspectionError::DataTask),
                                                        Ok(None) => return,
                                                        Err(error) => Err(SettingsInspectionError::Synchronization(error)),
                                                    };
                                                    match result {
                                                        Ok(summary) => {
                                                            feedback.set(
                                                                "Archive verified. Review the summary before restoring."
                                                                    .to_string(),
                                                            );
                                                            pending_restore.set(Some(PendingRestore {
                                                                source: PendingRestoreSource::PortableArchive(path),
                                                                summary,
                                                            }));
                                                        }
                                                        Err(error) => {
                                                            let message = format!(
                                                                "Archive inspection failed: {error}"
                                                            );
                                                            feedback.set(message.clone());
                                                            onstatus.call(message);
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                                if let Some(pending) = pending_restore.read().clone() {
                                    div {
                                        class: "restore-confirmation-card",
                                        role: "alert",
                                        aria_busy: current_operation == Some(ApplicationOperation::RestoreArchive),
                                        h4 { "Verified backup ready to restore" }
                                        p {
                                            "Created {pending.summary.created_at}; "
                                            "{pending.summary.notebook_count} notebook(s), "
                                            "{pending.summary.note_count} note(s), "
                                            "{pending.summary.attachment_count} attachment(s), and "
                                            "{pending.summary.revision_count} revision(s)."
                                        }
                                        p {
                                            class: "restore-size-summary",
                                            "Backup {format_file_size(pending.summary.archive_bytes.min(i64::MAX as u64) as i64)} · "
                                            "Expanded {format_file_size(pending.summary.expanded_bytes.min(i64::MAX as u64) as i64)}"
                                        }
                                        p { class: "danger-copy",
                                            "Restoring replaces the current library after Rustnote creates a complete safety archive."
                                        }
                                        div { class: "restore-confirmation-actions",
                                            button {
                                                class: "secondary-settings-button",
                                                disabled: operation_active,
                                                onclick: move |_| pending_restore.set(None),
                                                "Cancel"
                                            }
                                            button {
                                                class: "secondary-settings-button danger",
                                                disabled: operation_active,
                                                aria_busy: current_operation == Some(ApplicationOperation::RestoreArchive),
                                                onclick: {
                                                    let settings = settings.clone();
                                                    let editor_service = editor_service.clone();
                                                    let privacy_service = privacy_service.clone();
                                                    let restore_source = pending.source.clone();
                                                    move |_| {
                                                        let operation = ApplicationOperation::RestoreArchive;
                                                        let editing_pause = restore_archive_background.pause_editing();
                                                        feedback.set(operation.progress_label().to_string());
                                                        onstatus.call(operation.progress_label().to_string());
                                                        let task_settings = settings.clone();
                                                        let task_editor = editor_service.clone();
                                                        let task_privacy = privacy_service.clone();
                                                        let task_source = restore_source.clone();
                                                        let task_background = restore_archive_background.clone();
                                                        let mut task_selected_note = selected_note;
                                                        spawn(async move {
                                                            let _editing_pause = editing_pause;
                                                            let operation_turn = match task_background
                                                                .begin_operation(operation)
                                                                .await
                                                            {
                                                                Ok(turn) => turn,
                                                                Err(error) => {
                                                                    let message = format!(
                                                                        "Archive restore could not enter the storage queue: {error}"
                                                                    );
                                                                    feedback.set(message.clone());
                                                                    onstatus.call(message);
                                                                    return;
                                                                }
                                                            };
                                                            let synchronization = EditorSynchronizationContext::new(
                                                                task_editor.clone(),
                                                                task_selected_note,
                                                                editor_session,
                                                                privacy_epoch,
                                                                &task_privacy,
                                                                list_generation,
                                                                note_selection_generation,
                                                            );
                                                            let preparation = synchronize_current_editor_scoped(
                                                                &synchronization,
                                                                SynchronizationMode::Checkpoint,
                                                            )
                                                            .await
                                                            .map(|_| ());

                                                            if let Err(error) = preparation {
                                                                drop(operation_turn);
                                                                let message = format!(
                                                                    "Archive restore could not start: {error}"
                                                                );
                                                                feedback.set(message.clone());
                                                                onstatus.call(message);
                                                                return;
                                                            }

                                                            // Stage 2 for Restore is the storage-owned *library privacy
                                                            // transition*, not a read-side whole-library lease. Holding a
                                                            // read lease across Restore would deadlock its exclusive
                                                            // admission. Revoke every old UI plaintext publication first;
                                                            // restore_complete_archive / restore_backup_repository_snapshot
                                                            // then acquire the fresh coordinator library transition before
                                                            // replacing canonical state and revoking all Vault sessions.
                                                            let privacy_revocation = privacy_epoch.begin_library_ui_revocation();
                                                            task_selected_note.set(None);
                                                            let disposed = document::eval(
                                                                &live_editor_dispose_script(false),
                                                            )
                                                            .join::<bool>()
                                                            .await
                                                            .unwrap_or(false);

                                                            let result = if disposed {
                                                                operation_turn
                                                                    .execute(move || {
                                                                        task_editor
                                                                            .reset_library()
                                                                            .map_err(SettingsRestoreError::from)?;
                                                                        let summary = match task_source {
                                                                            PendingRestoreSource::PortableArchive(path) => {
                                                                                task_settings
                                                                                    .restore_complete_archive(&path)
                                                                            }
                                                                            PendingRestoreSource::RepositorySnapshot(
                                                                                snapshot_id,
                                                                            ) => task_settings
                                                                                .restore_backup_repository_snapshot(
                                                                                    &snapshot_id,
                                                                                ),
                                                                        }
                                                                        .map_err(SettingsRestoreError::from)?;
                                                                        let repository_status = task_settings
                                                                            .backup_repository_status()
                                                                            .ok();
                                                                        Ok((summary, repository_status))
                                                                    })
                                                                    .await
                                                            } else {
                                                                drop(operation_turn);
                                                                Err(SettingsRestoreError::EditorDispose)
                                                            };
                                                            drop(privacy_revocation);
                                                            // Restore (including a failed Restore after editor disposal)
                                                            // must always reconstruct from canonical state under fresh
                                                            // post-transition aggregate authority. A one-shot capture can
                                                            // legitimately lose a race with another privacy transition and
                                                            // must not strand the UI in its revoked/empty state.
                                                            let _post_restore_publication_lease = loop {
                                                                if let Some(token) = privacy_epoch
                                                                    .capture_multi_note_publication(
                                                                        &task_privacy,
                                                                        list_generation,
                                                                    )
                                                                    && let Some(lease) = token.begin_publication()
                                                                {
                                                                    break lease;
                                                                }
                                                                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                                                            };
                                                            match result {
                                                                Ok((summary, repository_status)) => {
                                                                    pending_restore.set(None);
                                                                    if let Some(repository_status) = repository_status {
                                                                        backup_status.set(repository_status);
                                                                    }
                                                                    onrestored.call(());
                                                                    feedback.set(format!(
                                                                        "Restored {} note(s), {} attachment(s), and {} revision(s).",
                                                                        summary.note_count,
                                                                        summary.attachment_count,
                                                                        summary.revision_count
                                                                    ));
                                                                    onstatus.call(
                                                                        "Complete archive restored"
                                                                            .to_string(),
                                                                    );
                                                                }
                                                                Err(error) => {
                                                                    onrestored.call(());
                                                                    let message = format!(
                                                                        "Archive restore failed: {error}"
                                                                    );
                                                                    feedback.set(message.clone());
                                                                    onstatus.call(message);
                                                                }
                                                            }
                                                        });
                                                    }
                                                },
                                                if current_operation == Some(ApplicationOperation::RestoreArchive) {
                                                    "Restoring…"
                                                } else {
                                                    "Restore verified backup"
                                                }
                                            }
                                        }
                                    }
                                }
                                p { class: "settings-note",
                                    "Automatic backups use complete SQLite snapshots and store each attachment blob only once. Pre-restore safety snapshots share the same repository and retention budget. Manual .rustnote.zip exports remain standalone portable archives."
                                }
                            },
                            SettingsSection::History => rsx! {
                                SettingsHeading {
                                    title: "Version history",
                                    description: "Bound recovery history by count, age, and total storage."
                                }
                                div { class: "history-usage-card", aria_busy: diagnostics_are_loading,
                                    if diagnostics_are_loading {
                                        div {
                                            strong { "Loading…" }
                                            span { "Calculating retained history" }
                                        }
                                    } else {
                                        div {
                                            strong { "{revision_usage.read().revision_count} versions" }
                                            span { "currently retained" }
                                        }
                                        div {
                                            strong { "{format_file_size(revision_usage.read().total_bytes)}" }
                                            span { "history storage" }
                                        }
                                    }
                                }
                                div { class: "settings-form-grid",
                                    label {
                                        span { "Versions per note" }
                                        input {
                                            value: "{revision_count}",
                                            disabled: operation_active,
                                            inputmode: "numeric",
                                            oninput: move |event| revision_count.set(event.value())
                                        }
                                        small { "10–2,000" }
                                    }
                                    label {
                                        span { "Maximum age in days" }
                                        input {
                                            value: "{revision_age}",
                                            disabled: operation_active,
                                            inputmode: "numeric",
                                            oninput: move |event| revision_age.set(event.value())
                                        }
                                        small { "0 keeps versions indefinitely" }
                                    }
                                    label {
                                        span { "Total history budget (MB)" }
                                        input {
                                            value: "{revision_megabytes}",
                                            disabled: operation_active,
                                            inputmode: "numeric",
                                            oninput: move |event| revision_megabytes.set(event.value())
                                        }
                                        small { "16–10,240 MB" }
                                    }
                                }
                                div { class: "settings-button-row",
                                    button {
                                        class: "primary-settings-button",
                                        disabled: operation_active,
                                        aria_busy: current_operation == Some(ApplicationOperation::SaveHistoryLimits),
                                        onclick: {
                                            let settings = settings.clone();
                                            move |_| {
                                                let count = revision_count.read().trim().parse::<i64>();
                                                let age = revision_age.read().trim().parse::<i64>();
                                                let megabytes = revision_megabytes.read().trim().parse::<i64>();
                                                let (Ok(count), Ok(age), Ok(megabytes)) = (count, age, megabytes) else {
                                                    feedback.set("Enter whole numbers for all history limits.".to_string());
                                                    return;
                                                };
                                                let Some(bytes) = megabytes.checked_mul(1024 * 1024) else {
                                                    feedback.set("The history storage limit is too large.".to_string());
                                                    return;
                                                };
                                                let operation = ApplicationOperation::SaveHistoryLimits;
                                                feedback.set(operation.progress_label().to_string());
                                                onstatus.call(operation.progress_label().to_string());
                                                let task_settings = settings.clone();
                                                let task_background = save_limits_background.clone();
                                                spawn(async move {
                                                    let result = task_background.execute(operation, move || {
                                                        task_settings
                                                            .set_revision_retention(count, age)
                                                            .map_err(super::task_error::DataTaskError::from)?;
                                                        task_settings
                                                            .set_revision_storage_limit(bytes)
                                                            .map_err(super::task_error::DataTaskError::from)?;
                                                        task_settings
                                                            .revision_storage_usage()
                                                            .map_err(super::task_error::DataTaskError::from)
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok(usage) => {
                                                            revision_usage.set(usage);
                                                            feedback.set(
                                                                "Version-history limits saved and applied."
                                                                    .to_string(),
                                                            );
                                                            onstatus.call(
                                                                "Version-history limits applied".to_string(),
                                                            );
                                                        }
                                                        Err(error) => {
                                                            let message = format!(
                                                                "Could not apply history limits: {error}"
                                                            );
                                                            feedback.set(message.clone());
                                                            onstatus.call(message);
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        if current_operation == Some(ApplicationOperation::SaveHistoryLimits) {
                                            "Applying…"
                                        } else {
                                            "Save limits"
                                        }
                                    }
                                    button {
                                        class: "secondary-settings-button",
                                        disabled: operation_active,
                                        aria_busy: current_operation == Some(ApplicationOperation::PruneHistory),
                                        onclick: {
                                            let settings = settings.clone();
                                            move |_| {
                                                let operation = ApplicationOperation::PruneHistory;
                                                feedback.set(operation.progress_label().to_string());
                                                onstatus.call(operation.progress_label().to_string());
                                                let task_settings = settings.clone();
                                                let task_background = prune_history_background.clone();
                                                spawn(async move {
                                                    let result = task_background.execute(operation, move || {
                                                        let removed = task_settings
                                                            .prune_revision_history()
                                                            .map_err(super::task_error::DataTaskError::from)?;
                                                        let usage = task_settings
                                                            .revision_storage_usage()
                                                            .map_err(super::task_error::DataTaskError::from)?;
                                                        Ok::<_, super::task_error::DataTaskError>((removed, usage))
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok((removed, usage)) => {
                                                            revision_usage.set(usage);
                                                            feedback.set(format!(
                                                                "History pruned; removed {removed} nonessential version(s)."
                                                            ));
                                                            onstatus.call("Version history pruned".to_string());
                                                        }
                                                        Err(error) => {
                                                            let message = format!(
                                                                "Could not prune history: {error}"
                                                            );
                                                            feedback.set(message.clone());
                                                            onstatus.call(message);
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        if current_operation == Some(ApplicationOperation::PruneHistory) {
                                            "Pruning…"
                                        } else {
                                            "Prune now"
                                        }
                                    }
                                }
                            },
                            SettingsSection::Storage => rsx! {
                                SettingsHeading {
                                    title: "Storage",
                                    description: "Manage Trash retention, library locations, and automatic-backup storage."
                                }
                                div { class: "settings-action-card trash-retention-card",
                                    div {
                                        strong { "Trash retention" }
                                        p { "Notes in Trash are read-only. By default they are permanently deleted after 30 days; choose any positive day count or disable automatic deletion." }
                                    }
                                    div { class: "trash-retention-controls",
                                        select {
                                            aria_label: "Automatic Trash deletion",
                                            disabled: operation_active,
                                            value: if *trash_retention_enabled.read() { "automatic" } else { "never" },
                                            onchange: move |event| {
                                                trash_retention_enabled.set(event.value() == "automatic");
                                            },
                                            option { value: "automatic", "Delete automatically" }
                                            option { value: "never", "Never delete automatically" }
                                        }
                                        input {
                                            r#type: "number",
                                            min: "1",
                                            step: "1",
                                            inputmode: "numeric",
                                            aria_label: "Days before permanent deletion",
                                            disabled: operation_active || !*trash_retention_enabled.read(),
                                            value: "{trash_retention_days}",
                                            oninput: move |event| trash_retention_days.set(event.value()),
                                        }
                                        span { "days" }
                                        button {
                                            class: "secondary-settings-button",
                                            disabled: operation_active,
                                            aria_busy: current_operation == Some(ApplicationOperation::SaveTrashRetention),
                                            onclick: {
                                                let settings = settings.clone();
                                                let notes = notes.clone();
                                                move |_| {
                                                    let retention = if *trash_retention_enabled.read() {
                                                        match trash_retention_days.read().trim().parse::<u32>() {
                                                            Ok(days) if days > 0 => Some(days),
                                                            _ => {
                                                                feedback.set(
                                                                    "Trash retention must be a positive whole number of days."
                                                                        .to_string(),
                                                                );
                                                                return;
                                                            }
                                                        }
                                                    } else {
                                                        None
                                                    };
                                                    let operation = ApplicationOperation::SaveTrashRetention;
                                                    feedback.set(operation.progress_label().to_string());
                                                    onstatus.call(operation.progress_label().to_string());
                                                    let task_settings = settings.clone();
                                                    let task_notes = notes.clone();
                                                    let task_background = save_trash_retention_background.clone();
                                                    let selected_id = selected_note
                                                        .read()
                                                        .as_ref()
                                                        .filter(|note| note.trashed)
                                                        .map(|note| note.id.clone());
                                                    spawn(async move {
                                                        let result = task_background.execute(operation, move || {
                                                            let updated = task_settings
                                                                .set_trash_retention_days(retention)
                                                                .map_err(super::task_error::DataTaskError::from)?;
                                                            let refreshed = selected_id.as_ref().map(|note_id| {
                                                                task_notes.get(note_id)
                                                            })
                                                                .transpose()
                                                                .map_err(super::task_error::DataTaskError::from)?
                                                                .flatten();
                                                            Ok::<_, super::task_error::DataTaskError>((updated, refreshed))
                                                        })
                                                        .await;
                                                        match result {
                                                            Ok((updated, refreshed)) => {
                                                                if let Some(note) = refreshed {
                                                                    selected_note.set(Some(note));
                                                                }
                                                                let message = match retention {
                                                                    Some(days) => format!(
                                                                        "Trash retention set to {days} day(s). Countdown reset for {updated} note(s) already in Trash."
                                                                    ),
                                                                    None => format!(
                                                                        "Automatic Trash deletion disabled for {updated} note(s)."
                                                                    ),
                                                                };
                                                                feedback.set(message);
                                                                onstatus.call("Trash retention updated".to_string());
                                                            }
                                                            Err(error) => {
                                                                let message = format!(
                                                                    "Could not update Trash retention: {error}"
                                                                );
                                                                feedback.set(message.clone());
                                                                onstatus.call(message);
                                                            }
                                                        }
                                                    });
                                                }
                                            },
                                            if current_operation == Some(ApplicationOperation::SaveTrashRetention) {
                                                "Applying…"
                                            } else {
                                                "Save"
                                            }
                                        }
                                    }
                                }
                                div { class: "storage-principles",
                                    div {
                                        strong { "One library folder" }
                                        p { "The SQLite database, attachments, emergency journals, and ownership lock stay together." }
                                    }
                                    div {
                                        strong { "Separate backups" }
                                        p { "The deduplicated automatic-backup repository can use a different folder or drive." }
                                    }
                                    div {
                                        strong { "Restart to apply" }
                                        p { "Changing these paths does not copy or delete the current library." }
                                    }
                                }
                                div { class: "settings-action-card",
                                    div {
                                        strong { "Library folder" }
                                        p {
                                            title: "{configured_data_directory.read().display()}",
                                            "{configured_data_directory.read().display()}"
                                        }
                                    }
                                    button {
                                        class: "secondary-settings-button",
                                        disabled: operation_active,
                                        onclick: move |_| {
                                            if settings_operation(&execution_status.read()).is_some()
                                                || *file_dialog_active.read()
                                            {
                                                return;
                                            }
                                            file_dialog_active.set(true);
                                            let initial_directory = configured_data_directory.read().clone();
                                            let backup_directory = configured_backup_directory.read().clone();
                                            let active_settings = library_location_settings.clone();
                                            let background = library_location_background.clone();
                                            let mut task_dialog = file_dialog_active;
                                            spawn(async move {
                                                let selected = AsyncFileDialog::new()
                                                    .set_title("Choose Rustnote library folder")
                                                    .set_directory(initial_directory)
                                                    .pick_folder()
                                                    .await;
                                                task_dialog.set(false);
                                                if let Some(path) = selected
                                                    .map(|handle| handle.path().to_path_buf())
                                                {
                                                    persist_storage_locations(
                                                        path,
                                                        backup_directory,
                                                        active_settings,
                                                        background,
                                                        library_location_status,
                                                        storage_location_signals,
                                                    );
                                                }
                                            });
                                        },
                                        "Choose folder…"
                                    }
                                }
                                div { class: "settings-action-card",
                                    div {
                                        strong { "Automatic backup folder" }
                                        p {
                                            title: "{configured_backup_directory.read().display()}",
                                            "{configured_backup_directory.read().display()}"
                                        }
                                    }
                                    button {
                                        class: "secondary-settings-button",
                                        disabled: operation_active,
                                        onclick: move |_| {
                                            if settings_operation(&execution_status.read()).is_some()
                                                || *file_dialog_active.read()
                                            {
                                                return;
                                            }
                                            file_dialog_active.set(true);
                                            let initial_directory = configured_backup_directory.read().clone();
                                            let data_directory = configured_data_directory.read().clone();
                                            let active_settings = backup_location_settings.clone();
                                            let background = backup_location_background.clone();
                                            let mut task_dialog = file_dialog_active;
                                            spawn(async move {
                                                let selected = AsyncFileDialog::new()
                                                    .set_title("Choose Rustnote automatic backup folder")
                                                    .set_directory(initial_directory)
                                                    .pick_folder()
                                                    .await;
                                                task_dialog.set(false);
                                                if let Some(path) = selected
                                                    .map(|handle| handle.path().to_path_buf())
                                                {
                                                    persist_storage_locations(
                                                        data_directory,
                                                        path,
                                                        active_settings,
                                                        background,
                                                        backup_location_status,
                                                        storage_location_signals,
                                                    );
                                                }
                                            });
                                        },
                                        "Choose folder…"
                                    }
                                }
                                div { class: "settings-button-row",
                                    button {
                                        class: "secondary-settings-button",
                                        disabled: operation_active,
                                        onclick: move |_| {
                                            let defaults = SettingsService::default_locations();
                                            persist_storage_locations(
                                                defaults.data_directory,
                                                defaults.backup_directory,
                                                default_location_settings.clone(),
                                                default_location_background.clone(),
                                                default_location_status,
                                                storage_location_signals,
                                            );
                                        },
                                        "Use default folders"
                                    }
                                }
                                if *storage_restart_required.read() {
                                    p { class: "settings-note",
                                        "Restart Rustnote to use the selected folders. Existing library and backup files remain in their current locations."
                                    }
                                }
                                dl { class: "path-list",
                                    div {
                                        dt { "Active library" }
                                        dd {
                                            title: "{active_paths.data_directory.display()}",
                                            "{active_paths.data_directory.display()}"
                                        }
                                    }
                                    div {
                                        dt { "Active attachments" }
                                        dd {
                                            title: "{active_paths.attachments_directory.display()}",
                                            "{active_paths.attachments_directory.display()}"
                                        }
                                    }
                                    div {
                                        dt { "Active backup repository" }
                                        dd {
                                            title: "{active_paths.automatic_backup_directory.display()}",
                                            "{active_paths.automatic_backup_directory.display()}"
                                        }
                                    }
                                }
                            },
                            SettingsSection::Shortcuts => rsx! {
                                SettingsHeading {
                                    title: "Keyboard shortcuts",
                                    description: "Use a small, memorable command set for everyday work."
                                }
                                div { class: "shortcut-summary",
                                    ShortcutLine { keys: "⌘/Ctrl N", action: "Create a new note" }
                                    ShortcutLine { keys: "⌘/Ctrl F", action: "Find within the open note" }
                                    ShortcutLine { keys: "⌘/Ctrl ⇧ F", action: "Search the library" }
                                    ShortcutLine { keys: "⌘/Ctrl V", action: "Paste with supported rich formatting" }
                                    ShortcutLine { keys: "⌘/Ctrl ⇧ V", action: "Paste as plain text" }
                                    ShortcutLine { keys: "⌘/Ctrl S", action: "Save immediately" }
                                    ShortcutLine { keys: "⌘/Ctrl ⇧ P", action: "Pin or unpin the selected note" }
                                    ShortcutLine { keys: "⌘/Ctrl /", action: "Open the full shortcut reference" }
                                }
                                button {
                                    id: "settings-shortcuts-trigger",
                                    class: "primary-settings-button",
                                    aria_controls: "shortcuts-dialog",
                                    aria_haspopup: "dialog",
                                    aria_expanded: "false",
                                    "data-transient-trigger": "true",
                                    onclick: move |_| onopen_shortcuts.call(()),
                                    "Open full shortcut reference"
                                }
                            },
                            SettingsSection::Diagnostics => rsx! {
                                SettingsHeading {
                                    title: "Diagnostics",
                                    description: "Technical details stay available without occupying the everyday interface."
                                }
                                dl { class: "diagnostics-list",
                                    div {
                                        dt { "Application" }
                                        dd { "Rustnote v{FULL_BUILD_IDENTIFIER}" }
                                    }
                                    div {
                                        dt { "Application version" }
                                        dd { "{APP_VERSION}" }
                                    }
                                    div {
                                        dt { "Build ID" }
                                        dd { "{BUILD_ID}" }
                                    }
                                    div {
                                        dt { "Database last opened by" }
                                        dd { "{last_opened_build}" }
                                    }
                                    div {
                                        dt { "Schema" }
                                        dd { "Fresh development baseline {schema_version}" }
                                    }
                                    div {
                                        dt { "Database" }
                                        dd {
                                            title: "{active_paths.database_path.display()}",
                                            "{active_paths.database_path.display()}"
                                        }
                                    }
                                    div {
                                        dt { "Journal mode" }
                                        dd { "SQLite WAL · synchronous NORMAL · explicit full save/close checkpoints" }
                                    }
                                    div {
                                        dt { "Storage diagnostics" }
                                        dd {
                                            if diagnostics_are_loading {
                                                "Loading…"
                                            } else if let Some(message) = diagnostics_message.as_ref() {
                                                "{message}"
                                            } else {
                                                "Available"
                                            }
                                        }
                                    }
                                    div {
                                        dt { "Backup quarantine" }
                                        dd {
                                            if diagnostics_are_loading {
                                                "Loading…"
                                            } else {
                                                "{quarantine_count} retained artifact(s)"
                                            }
                                        }
                                    }
                                    if !diagnostics_are_loading
                                        && !quarantine_artifacts.is_empty()
                                    {
                                        div {
                                            dt { "Quarantined backup artifacts" }
                                            dd {
                                                for artifact in quarantine_artifacts.iter() {
                                                    span { class: "diagnostic-detail", "{artifact}" }
                                                }
                                            }
                                        }
                                    }
                                }
                                p { class: "settings-note",
                                    "Paths are intentionally shown only here; the status bar reports saving state, not implementation details."
                                }
                            },
                        }
                    }
                }

                footer {
                    class: "settings-footer",
                    aria_live: "polite",
                    span { class: "status-dot" }
                    span {
                        if let Some(operation) = current_operation {
                            "{operation.progress_label()}"
                        } else {
                            "{feedback}"
                        }
                    }
                }
            }
        }
    }
}
