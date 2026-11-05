use dioxus::prelude::*;
use rfd::AsyncFileDialog;

use crate::{
    application::{
        context::ApplicationServices,
        errors::PortabilityError,
        executor::{ApplicationExecutionError, ApplicationExecutor, ApplicationOperation},
        portability::{destination_for_collection, portable_format_filter},
    },
    domain::Note,
    portability::{ImportPlan, ImportUndoToken, PortabilityFormat},
};

use super::synchronization::{
    EditorSession, EditorSynchronizationContext, SynchronizationError, SynchronizationMode,
    synchronize_then_begin_whole_library_operation,
};


#[derive(Debug)]
enum LibraryPortabilityExportError {
    Synchronization(SynchronizationError),
    Portability(PortabilityError),
    Execution(ApplicationExecutionError),
}

impl std::fmt::Display for LibraryPortabilityExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synchronization(error) => write!(
                formatter,
                "Could not finish saving before library export: {error}",
            ),
            Self::Portability(error) => std::fmt::Display::fmt(error, formatter),
            Self::Execution(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl From<PortabilityError> for LibraryPortabilityExportError {
    fn from(error: PortabilityError) -> Self {
        Self::Portability(error)
    }
}

impl From<ApplicationExecutionError> for LibraryPortabilityExportError {
    fn from(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingImport {
    plan: ImportPlan,
}

#[derive(Clone, PartialEq, Props)]
pub(super) struct PortabilitySettingsProps {
    pub services: ApplicationServices,
    pub selected_note: Signal<Option<Note>>,
    pub editor_session: Signal<Option<EditorSession>>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub list_generation: Signal<u64>,
    pub note_selection_generation: Signal<u64>,
    pub disabled: bool,
    pub onimported: EventHandler<()>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn PortabilitySettings(props: PortabilitySettingsProps) -> Element {
    let PortabilitySettingsProps {
        services,
        selected_note,
        editor_session,
        privacy_epoch,
        list_generation,
        note_selection_generation,
        disabled,
        onimported,
        onstatus,
    } = props;
    let background = use_context::<ApplicationExecutor>();
    let mut busy = use_signal(|| false);
    let mut pending = use_signal(|| None::<PendingImport>);
    let mut last_import = use_signal(|| None::<ImportUndoToken>);
    let controls_disabled = disabled || *busy.read();

    let inspect_file = {
        let services = services.clone();
        move |_| {
            if controls_disabled { return; }
            let service = services.portability();
            busy.set(true);
            spawn(async move {
                let selected = AsyncFileDialog::new()
                    .set_title("Import into Rustnote")
                    .add_filter("Rustnote-supported files", &["txt", "md", "markdown", "html", "htm", "rustnote"])
                    .pick_file()
                    .await;
                let Some(handle) = selected else { busy.set(false); return; };
                let source = handle.path().to_path_buf();
                match service.inspect_source(source).await {
                    Ok(plan) => pending.set(Some(PendingImport { plan })),
                    Err(error) => onstatus.call(format!("Could not inspect import: {error}")),
                }
                busy.set(false);
            });
        }
    };

    let inspect_folder = {
        let services = services.clone();
        move |_| {
            if controls_disabled { return; }
            let service = services.portability();
            busy.set(true);
            spawn(async move {
                let selected = AsyncFileDialog::new().set_title("Import a folder into Rustnote").pick_folder().await;
                let Some(handle) = selected else { busy.set(false); return; };
                let source = handle.path().to_path_buf();
                match service.inspect_source(source).await {
                    Ok(plan) => pending.set(Some(PendingImport { plan })),
                    Err(error) => onstatus.call(format!("Could not inspect import: {error}")),
                }
                busy.set(false);
            });
        }
    };

    rsx! {
        super::settings_support::SettingsHeading {
            title: "Import & export",
            description: "Move notes without lock-in. TXT, Markdown and HTML are interoperable; Rustnote bundles preserve Rustnote-owned state."
        }

        div { class: "settings-action-list",
            super::settings_support::SettingsAction {
                title: "Import file",
                description: "TXT, Markdown, HTML, or a lossless Rustnote bundle.",
                action: "Choose file…",
                disabled: controls_disabled,
                busy: *busy.read(),
                onclick: inspect_file,
            }
            super::settings_support::SettingsAction {
                title: "Import folder",
                description: "Recursively import supported editable files, local Markdown/HTML assets, notebook folders, and internal links.",
                action: "Choose folder…",
                disabled: controls_disabled,
                busy: false,
                onclick: inspect_folder,
            }
        }

        if let Some(candidate) = pending.read().clone() {
            div { class: "history-usage-card",
                div { strong { "{candidate.plan.inspection().notes} notes" } span { "ready to import" } }
                if candidate.plan.inspection().plain_text > 0 {
                    div { strong { "{candidate.plan.inspection().plain_text}" } span { "plain-text files" } }
                }
                if candidate.plan.inspection().markdown > 0 {
                    div { strong { "{candidate.plan.inspection().markdown}" } span { "Markdown files" } }
                }
                if candidate.plan.inspection().html > 0 {
                    div { strong { "{candidate.plan.inspection().html}" } span { "HTML files" } }
                }
                if candidate.plan.inspection().rustnote_bundles > 0 {
                    div { strong { "{candidate.plan.inspection().rustnote_bundles}" } span { "Rustnote bundle" } }
                }
                div { strong { "{candidate.plan.inspection().attachments} attachments" } span { "verified/local resources" } }
                if candidate.plan.inspection().notebooks > 0 {
                    div { strong { "{candidate.plan.inspection().notebooks} notebooks" } span { "Rustnote hierarchy" } }
                }
                if candidate.plan.inspection().revisions > 0 {
                    div { strong { "{candidate.plan.inspection().revisions} revisions" } span { "version history" } }
                }
                if candidate.plan.inspection().sources > 0 {
                    div { strong { "{candidate.plan.inspection().sources} sources" } span { "note sources" } }
                }
                if candidate.plan.inspection().citations > 0 {
                    div { strong { "{candidate.plan.inspection().citations} citations" } span { "source citations" } }
                }
                div { strong { "{candidate.plan.inspection().total_bytes} bytes" } span { "selected source" } }
            }
            if !candidate.plan.inspection().warnings.is_empty() {
                p { class: "settings-note", {candidate.plan.inspection().warnings.join(" · ")} }
            }
            div { class: "settings-button-row",
                button {
                    class: "secondary-settings-button",
                    disabled: controls_disabled,
                    onclick: move |_| pending.set(None),
                    "Cancel"
                }
                button {
                    class: "primary-settings-button",
                    disabled: controls_disabled,
                    onclick: {
                        let services = services.clone();
                        let background = background.clone();
                        let plan = candidate.plan.clone();
                        move |_| {
                            if *busy.read() { return; }
                            let service = services.portability();
                            let background = background.clone();
                            let plan = plan.clone();
                            busy.set(true);
                            spawn(async move {
                                match service.import_plan(&background, plan).await {
                                    Ok(summary) => {
                                        pending.set(None);
                                        last_import.set(summary.undo.clone());
                                        onstatus.call(format!("Imported {} notes and {} attachments", summary.notes, summary.attachments));
                                        onimported.call(());
                                    }
                                    Err(error) => onstatus.call(format!("Import failed: {error}")),
                                }
                                busy.set(false);
                            });
                        }
                    },
                    "Import"
                }
            }
        }

        if let Some(token) = last_import.read().clone() {
            div { class: "settings-action-list",
                super::settings_support::SettingsAction {
                    title: "Undo last import",
                    description: "Remove the notes and notebooks created by the most recent import in this settings session.",
                    action: "Undo…",
                    disabled: controls_disabled,
                    busy: false,
                    onclick: {
                        let services = services.clone();
                        let background = background.clone();
                        move |_| {
                            if *busy.read() { return; }
                            let service = services.portability();
                            let background = background.clone();
                            let token = token.clone();
                            busy.set(true);
                            spawn(async move {
                                match service.undo_import(&background, token).await {
                                    Ok(()) => {
                                        last_import.set(None);
                                        onstatus.call("Last import removed".to_string());
                                        onimported.call(());
                                    }
                                    Err(error) => onstatus.call(format!("Could not undo import: {error}")),
                                }
                                busy.set(false);
                            });
                        }
                    },
                }
            }
        }

        div { class: "settings-section-divider" }
        h3 { "Export library" }
        p { class: "settings-note", "Editable formats export into a new folder. Rustnote bundle exports one lossless .rustnote file. PDF remains a note-level rendered export." }
        div { class: "settings-action-list",
            for format in PortabilityFormat::ALL {
                LibraryExportAction {
                    services: services.clone(),
                    selected_note,
                    editor_session,
                    privacy_epoch,
                    list_generation,
                    note_selection_generation,
                    format,
                    disabled: controls_disabled,
                    busy,
                    onstatus,
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Props)]
struct LibraryExportActionProps {
    services: ApplicationServices,
    selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
    privacy_epoch: super::privacy::UiPrivacyEpoch,
    list_generation: Signal<u64>,
    note_selection_generation: Signal<u64>,
    format: PortabilityFormat,
    disabled: bool,
    busy: Signal<bool>,
    onstatus: EventHandler<String>,
}

#[component]
fn LibraryExportAction(props: LibraryExportActionProps) -> Element {
    let LibraryExportActionProps {
        services,
        selected_note,
        editor_session,
        privacy_epoch,
        list_generation,
        note_selection_generation,
        format,
        disabled,
        mut busy,
        onstatus,
    } = props;
    let background = use_context::<ApplicationExecutor>();
    rsx! {
        super::settings_support::SettingsAction {
            title: format.label(),
            description: format.capabilities().description(),
            action: "Export…",
            disabled,
            busy: false,
            onclick: move |_| {
                if *busy.read() { return; }
                let service = services.portability();
                let editor_service = services.editor();
                let publication_privacy = services.privacy();
                let operation_privacy = services.privacy_operations();
                let background = background.clone();
                busy.set(true);
                spawn(async move {
                    let destination = if format == PortabilityFormat::RustnoteBundle {
                        let (label, extensions) = portable_format_filter(format);
                        AsyncFileDialog::new()
                            .set_title("Export Rustnote library bundle")
                            .add_filter(label, extensions)
                            .set_file_name("Rustnote library.rustnote")
                            .save_file()
                            .await
                            .map(|handle| handle.path().to_path_buf())
                    } else {
                        AsyncFileDialog::new()
                            .set_title("Choose a folder for the library export")
                            .pick_folder()
                            .await
                            .map(|handle| destination_for_collection(handle.path(), "Rustnote library", format))
                    };
                    if let Some(destination) = destination {
                        let operation = ApplicationOperation::ExportPortability;
                        let synchronization = EditorSynchronizationContext::new(
                            editor_service,
                            selected_note,
                            editor_session,
                            privacy_epoch,
                            &publication_privacy,
                            list_generation,
                            note_selection_generation,
                        );
                        let result = match background.begin_operation(operation).await {
                            Ok(operation_turn) => match synchronize_then_begin_whole_library_operation(
                                &operation_turn,
                                &synchronization,
                                &operation_privacy,
                                SynchronizationMode::Persist,
                            )
                            .await
                            {
                                Ok(library_lease) => service
                                    .export_library_scope(
                                        operation_turn,
                                        &library_lease,
                                        format,
                                        destination,
                                    )
                                    .await
                                    .map_err(LibraryPortabilityExportError::from),
                                Err(error) => {
                                    Err(LibraryPortabilityExportError::Synchronization(error))
                                }
                            },
                            Err(error) => Err(LibraryPortabilityExportError::from(error)),
                        };
                        match result {
                            Ok(path) => {
                                onstatus.call(format!("Library exported to {}", path.display()))
                            }
                            Err(error) => onstatus.call(format!("Export failed: {error}")),
                        }
                    }
                    busy.set(false);
                });
            }
        }
    }
}
