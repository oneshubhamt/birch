use std::{collections::HashMap, sync::Arc};

use dioxus::prelude::*;
use rfd::AsyncFileDialog;

use crate::application::context::ApplicationServices;
use crate::application::errors::{ExportError, PortabilityError};
use crate::application::editor::EditorService;
use crate::application::notes::{NoteService, RevisionSnapshotResource};
use crate::application::persistence::{NoteChangeScope, unix_timestamp};
use crate::application::export::{
    cancel_note_pdf_print_script, prepare_note_pdf_print_script, wait_note_pdf_print_script,
};
use crate::application::portability::{destination_for_collection, portable_format_filter, suggested_portable_note_file_name};
use crate::application::privacy::PrivacyPublicationService;
use crate::portability::{PortabilityFormat, PortabilityScope};
use crate::{
    editor_document::DocumentValidationError,
    editor_projection::derive_editor_projections,
    markdown::render_markdown,
    application::errors::DataAccessError,
};
use crate::domain::{
    Note, NoteId, NoteRevisionSummary, Notebook, RevisionRetention, RevisionSnapshot,
    RevisionStorageUsage,
};

use crate::application::executor::{
    ApplicationExecutionError, ApplicationExecutor, ApplicationOperation, ExecutionFailure,
};
use super::editor::schedule_draft_scoped;
use super::eval_bridge;
use super::entity_commands::{TagCommand, execute_tag_command_blocking};
use super::synchronization::{
    EditorSession, SynchronizationError, SynchronizationMode, begin_draft_flush_scoped,
    complete_draft_flush, synchronize_current_editor,
};
use super::note_card::relative_time;
use super::sidebar::{indented_notebook_name, notebook_name_for};
use super::{EditorPopover, TagCatalogMutation, storage_or_report};


#[derive(Debug)]
enum RevisionPreviewError {
    Data(DataAccessError),
    Projection(DocumentValidationError),
    Execution(ApplicationExecutionError),
}

impl std::fmt::Display for RevisionPreviewError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Data(error) => std::fmt::Display::fmt(error, formatter),
            Self::Projection(error) => {
                write!(formatter, "Could not regenerate revision preview: {error}")
            }
            Self::Execution(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for RevisionPreviewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Data(error) => Some(error),
            Self::Projection(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<DataAccessError> for RevisionPreviewError {
    fn from(error: DataAccessError) -> Self { Self::Data(error) }
}

impl From<DocumentValidationError> for RevisionPreviewError {
    fn from(error: DocumentValidationError) -> Self { Self::Projection(error) }
}

impl ExecutionFailure for RevisionPreviewError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteMetadataView {
    pub notebooks: Vec<Notebook>,
    pub services: ApplicationServices,
    pub selected_note: Signal<Option<Note>>,
    pub editor_session: Signal<Option<EditorSession>>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub library_generation: Signal<u64>,
    pub note_generation: Signal<u64>,
    pub draft: Signal<Note>,
    pub tags_text: Signal<String>,
    pub editor_popover: Signal<Option<EditorPopover>>,
    pub editing_available: bool,
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteMetadataActions {
    pub onchanged: EventHandler<Note>,
    pub onopentag: EventHandler<String>,
    pub ontagcatalogchanged: EventHandler<TagCatalogMutation>,
    pub onstatus: EventHandler<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MetadataExportAction {
    Note(PortabilityFormat),
    Notebook(PortabilityFormat),
    NotePdf,
}

#[derive(Clone)]
struct MetadataExportContext {
    services: ApplicationServices,
    selected_note: Signal<Option<Note>>,
    editor_session: Signal<Option<EditorSession>>,
    privacy_epoch: super::privacy::UiPrivacyEpoch,
    library_generation: Signal<u64>,
    note_generation: Signal<u64>,
    draft: Signal<Note>,
    background: ApplicationExecutor,
    busy: Signal<bool>,
    editor_popover: Signal<Option<EditorPopover>>,
    onstatus: EventHandler<String>,
    desktop: dioxus_desktop::DesktopContext,
}

fn begin_metadata_export(action: MetadataExportAction, context: MetadataExportContext) {
    if *context.busy.read() { return; }
    let publication_token = match action {
        MetadataExportAction::Notebook(_) => context.privacy_epoch
            .capture_multi_note_publication_with_lifecycle(
                &context.services.privacy(),
                context.library_generation,
                context.note_generation,
            ),
        MetadataExportAction::Note(_) | MetadataExportAction::NotePdf => context.privacy_epoch
            .capture_note_publication(
                &context.services.privacy(),
                &context.draft.peek().id,
                context.note_generation,
            ),
    };
    let Some(publication_token) = publication_token else {
        context
            .onstatus
            .call("Export is unavailable during a privacy transition".to_string());
        return;
    };
    let mut busy = context.busy;
    let mut editor_popover = context.editor_popover;
    busy.set(true);
    editor_popover.set(None);
    spawn(async move {
        let result = perform_metadata_export(action, &context, publication_token.clone()).await;
        busy.set(false);
        match result {
            Ok(Some(message)) => {
                publication_token.publish(|| context.onstatus.call(message));
            }
            Ok(None) => {}
            Err(error) => {
                publication_token.publish(|| {
                    context.onstatus.call(format!("Export failed: {error}"));
                });
            }
        }
    });
}

#[derive(Debug)]
enum MetadataExportError {
    Synchronization(SynchronizationError),
    Export(ExportError),
    Portability(PortabilityError),
    NoSelectedNote,
    PrivacyAuthorizationChanged,
    PdfPreparation(String),
    PdfPreparationRejected,
    PrintDialog(String),
}

impl std::fmt::Display for MetadataExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synchronization(error) => std::fmt::Display::fmt(error, formatter),
            Self::Export(error) => std::fmt::Display::fmt(error, formatter),
            Self::Portability(error) => std::fmt::Display::fmt(error, formatter),
            Self::NoSelectedNote => formatter.write_str("no note is selected"),
            Self::PrivacyAuthorizationChanged => formatter.write_str(
                "privacy authorization changed while the export was in progress",
            ),
            Self::PdfPreparation(message) => write!(formatter, "Could not prepare the note for PDF export: {message}"),
            Self::PdfPreparationRejected => formatter.write_str("the note could not be prepared for PDF export"),
            Self::PrintDialog(message) => write!(formatter, "Could not open the native print dialog: {message}"),
        }
    }
}

impl std::error::Error for MetadataExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Synchronization(error) => Some(error),
            Self::Export(error) => Some(error),
            Self::Portability(error) => Some(error),
            Self::NoSelectedNote
            | Self::PrivacyAuthorizationChanged
            | Self::PdfPreparation(_)
            | Self::PdfPreparationRejected
            | Self::PrintDialog(_) => None,
        }
    }
}

impl From<SynchronizationError> for MetadataExportError {
    fn from(error: SynchronizationError) -> Self { Self::Synchronization(error) }
}
impl From<ExportError> for MetadataExportError {
    fn from(error: ExportError) -> Self { Self::Export(error) }
}
impl From<PortabilityError> for MetadataExportError {
    fn from(error: PortabilityError) -> Self { Self::Portability(error) }
}

async fn portable_export_destination(
    action: MetadataExportAction,
    draft: &Note,
) -> Option<std::path::PathBuf> {
    let (format, collection) = match action {
        MetadataExportAction::Note(format) => (format, false),
        MetadataExportAction::Notebook(format) => (format, true),
        MetadataExportAction::NotePdf => return None,
    };
    if collection && format != PortabilityFormat::RustnoteBundle {
        let selected = AsyncFileDialog::new()
            .set_title("Choose a folder for the notebook export")
            .pick_folder()
            .await?;
        return Some(destination_for_collection(selected.path(), "Rustnote notebook", format));
    }
    let (label, extensions) = portable_format_filter(format);
    let file_name = if collection {
        format!("Rustnote notebook.{}", format.extension())
    } else {
        suggested_portable_note_file_name(&draft.title, format)
    };
    AsyncFileDialog::new()
        .set_title(if collection { "Export notebook" } else { "Export note" })
        .add_filter(label, extensions)
        .set_file_name(&file_name)
        .save_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

async fn perform_metadata_export(
    action: MetadataExportAction,
    context: &MetadataExportContext,
    publication_token: super::privacy::PrivacyPublicationToken,
) -> Result<Option<String>, MetadataExportError> {
    let note_id = context.draft.peek().id.clone();
    let export_service = context.services.export();
    let export_lifetime = export_service.begin_note_export_lifetime(note_id.clone()).await?;
    if !publication_token.is_publishable() || !export_lifetime.is_publishable() {
        return Err(MetadataExportError::PrivacyAuthorizationChanged);
    }

    let draft = context.draft.read().clone();
    if draft.id != note_id {
        return Err(MetadataExportError::PrivacyAuthorizationChanged);
    }
    let destination = if action == MetadataExportAction::NotePdf {
        None
    } else {
        let Some(path) = portable_export_destination(action, &draft).await else { return Ok(None); };
        if !publication_token.is_publishable() || !export_lifetime.is_publishable() {
            return Err(MetadataExportError::PrivacyAuthorizationChanged);
        }
        Some(path)
    };

    let synchronized = synchronize_current_editor(
        context.services.editor(),
        context.selected_note,
        context.editor_session,
        SynchronizationMode::Persist,
        publication_token.clone(),
    )
    .await?
    .ok_or(MetadataExportError::NoSelectedNote)?;
    if synchronized.id != note_id {
        return Err(MetadataExportError::PrivacyAuthorizationChanged);
    }

    match action {
        MetadataExportAction::Note(format) => {
            let path = destination.expect("portable note destination");
            let result = context.services.portability().export_scope(
                &context.background,
                PortabilityScope::Note(synchronized.id.clone()),
                format,
                path,
            ).await?;
            if !publication_token.is_publishable() || !export_lifetime.is_publishable() {
                return Err(MetadataExportError::PrivacyAuthorizationChanged);
            }
            Ok(Some(format!("{} exported to {}", format.label(), result.display())))
        }
        MetadataExportAction::Notebook(format) => {
            let path = destination.expect("portable notebook destination");
            let result = context.services.portability().export_scope(
                &context.background,
                PortabilityScope::Notebook(synchronized.notebook_id.clone()),
                format,
                path,
            ).await?;
            if !publication_token.is_publishable() || !export_lifetime.is_publishable() {
                return Err(MetadataExportError::PrivacyAuthorizationChanged);
            }
            Ok(Some(format!("Notebook exported to {}", result.display())))
        }
        MetadataExportAction::NotePdf => {
            if !publication_token.is_publishable() || !export_lifetime.is_publishable() {
                return Err(MetadataExportError::PrivacyAuthorizationChanged);
            }
            let script = prepare_note_pdf_print_script(&synchronized.title);
            let prepared = eval_bridge::join_idempotent::<bool>(&script)
                .await
                .map_err(|error| MetadataExportError::PdfPreparation(error.to_string()))?;
            if !publication_token.is_publishable() || !export_lifetime.is_publishable() {
                let _ = eval_bridge::join_idempotent::<bool>(cancel_note_pdf_print_script()).await;
                return Err(MetadataExportError::PrivacyAuthorizationChanged);
            }
            if !prepared { return Err(MetadataExportError::PdfPreparationRejected); }
            if let Err(error) = context.desktop.webview.print() {
                let _ = eval_bridge::join_idempotent::<bool>(cancel_note_pdf_print_script()).await;
                return Err(MetadataExportError::PrintDialog(error.to_string()));
            }
            let completed = eval_bridge::join_idempotent::<bool>(wait_note_pdf_print_script())
                .await
                .map_err(|error| MetadataExportError::PrintDialog(error.to_string()))?;
            if !completed {
                return Err(MetadataExportError::PrintDialog(
                    "the native print lifecycle ended without an afterprint signal".to_string(),
                ));
            }
            if !publication_token.is_publishable() || !export_lifetime.is_publishable() {
                return Err(MetadataExportError::PrivacyAuthorizationChanged);
            }
            Ok(Some("PDF print/export completed".to_string()))
        }
    }
}

#[component]
pub(super) fn NoteMetadata(view: NoteMetadataView, actions: NoteMetadataActions) -> Element {
    let NoteMetadataView {
        notebooks,
        services,
        selected_note,
        editor_session,
        privacy_epoch,
        library_generation,
        note_generation,
        mut draft,
        mut tags_text,
        mut editor_popover,
        editing_available,
    } = view;
    let NoteMetadataActions {
        onchanged,
        onopentag,
        ontagcatalogchanged,
        onstatus,
    } = actions;
    let editor_service = services.editor();
    let metadata_save_busy = use_signal(|| false);
    let export_busy = use_signal(|| false);
    let export_background = use_context::<ApplicationExecutor>();
    let desktop = dioxus_desktop::use_window();
    let note_read_only = draft.read().trashed || draft.read().protection.is_locked();
    let metadata_busy = *metadata_save_busy.read() || note_read_only || !editing_available;
    let export_disabled =
        *export_busy.read() || !editing_available || draft.read().protection.is_locked();
    let note_tags = draft.read().tags.clone();
    let current_notebook_name = notebook_name_for(&notebooks, &draft.read().notebook_id);
    let export_context = MetadataExportContext {
        services: services.clone(),
        selected_note,
        editor_session,
        privacy_epoch,
        library_generation,
        note_generation,
        draft,
        background: export_background,
        busy: export_busy,
        editor_popover,
        onstatus,
        desktop,
    };

    rsx! {
        div { class: "note-metadata-row",
            div { class: "metadata-popover-anchor",
                button {
                    id: "editor-folder-trigger",
                    class: "metadata-chip folder-chip",
                    title: "Move note to another notebook",
                    aria_controls: "editor-folder-popover",
                    aria_haspopup: "dialog",
                    aria_expanded: "{*editor_popover.read() == Some(EditorPopover::Folder)}",
                    "data-transient-trigger": "true",
                    disabled: metadata_busy,
                    onclick: move |_| {
                        let next = if *editor_popover.read() == Some(EditorPopover::Folder) {
                            None
                        } else {
                            Some(EditorPopover::Folder)
                        };
                        editor_popover.set(next);
                    },
                    span { aria_hidden: "true", "▱" }
                    strong { "{current_notebook_name}" }
                    span { class: "chip-caret", "⌄" }
                }
                if *editor_popover.read() == Some(EditorPopover::Folder) {
                    div {
                        id: "editor-folder-popover",
                        class: "editor-popover metadata-popover folder-popover",
                        role: "dialog",
                        aria_label: "Move note to notebook",
                        "data-transient-surface": "true",
                        "data-transient-kind": "popover",
                        "data-dismiss-outside": "true",
                        "data-return-focus": "editor-folder-trigger",
                        "data-fallback-focus": "note-editor-main",
                        p { class: "popover-label", "Move to notebook" }
                        div { class: "popover-scroll-list",
                            for notebook in notebooks.iter() {
                                button {
                                    class: if draft.read().notebook_id == notebook.id { "popover-choice active" } else { "popover-choice" },
                                    disabled: metadata_busy,
                                    onclick: {
                                        let notebook_id = notebook.id.clone();
                                        let editor = editor_service.clone();
                                        move |_| {
                                            if *metadata_save_busy.read() {
                                                return;
                                            }
                                            let mut updated = draft.read().clone();
                                            updated.notebook_id = notebook_id.clone();
                                            updated.updated_at = unix_timestamp();
                                            draft.set(updated.clone());
                                            let pending_flush = begin_draft_flush_scoped(
                                                &editor,
                                                updated,
                                                SynchronizationMode::Checkpoint,
                                                NoteChangeScope::State,
                                            );
                                            let mut task_busy = metadata_save_busy;
                                            task_busy.set(true);
                                            spawn(async move {
                                                let result = complete_draft_flush(pending_flush).await;
                                                task_busy.set(false);
                                                match result {
                                                    Ok(result) => {
                                                        editor_popover.set(None);
                                                        onstatus.call(
                                                            result.completion_message("Saved locally"),
                                                        );
                                                    }
                                                    Err(error) => {
                                                        onstatus.call(format!("Save failed: {error}"));
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    span { "{indented_notebook_name(notebook, &notebooks)}" }
                                    if draft.read().notebook_id == notebook.id {
                                        span { class: "choice-check", "✓" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if draft.read().pinned {
                span { class: "metadata-chip pinned-chip", "◆ Pinned" }
            }
            if draft.read().archived {
                span { class: "metadata-chip subdued-chip", "Archived" }
            }
            if draft.read().trashed {
                span { class: "metadata-chip danger-chip", "Trash" }
            }
            if draft.read().protection.is_protected() {
                span { class: "metadata-chip subdued-chip", "Encrypted" }
            }

            div { class: "metadata-tags",
                button {
                    id: "editor-tags-trigger",
                    class: "metadata-chip add-metadata-chip metadata-tags-trigger",
                    title: if note_tags.is_empty() { "Add tags" } else { "Edit tags" },
                    aria_controls: "editor-tags-popover",
                    aria_haspopup: "dialog",
                    aria_expanded: "{*editor_popover.read() == Some(EditorPopover::Tags)}",
                    "data-transient-trigger": "true",
                    disabled: metadata_busy,
                    onclick: move |_| {
                        let next = if *editor_popover.read() == Some(EditorPopover::Tags) {
                            None
                        } else {
                            Some(EditorPopover::Tags)
                        };
                        editor_popover.set(next);
                    },
                    if note_tags.is_empty() { "+ Add tags" } else { "+" }
                }
                div { class: "metadata-tags-list",
                    for tag in note_tags.iter().take(3) {
                        MetadataTagChip {
                            key: "{tag}",
                            view: MetadataTagChipView {
                                tag: tag.clone(),
                                notes: services.notes(),
                                editor: editor_service.clone(),
                                draft,
                                tags_text,
                                editor_popover,
                                mutation_busy: metadata_save_busy,
                                privacy_epoch,
                                privacy: services.privacy(),
                                library_generation,
                                note_generation,
                                read_only: note_read_only,
                                editing_available,
                            },
                            actions: MetadataTagChipActions {
                                onchanged,
                                onopentag,
                                ontagcatalogchanged,
                                onstatus,
                            },
                        }
                    }
                    if note_tags.len() > 3 {
                        button {
                            class: "metadata-chip subdued-chip",
                            title: "Edit all tags",
                            aria_controls: "editor-tags-popover",
                            aria_haspopup: "dialog",
                            aria_expanded: "{*editor_popover.read() == Some(EditorPopover::Tags)}",
                            "data-transient-trigger": "true",
                            disabled: metadata_busy,
                            onclick: move |_| editor_popover.set(Some(EditorPopover::Tags)),
                            "+{note_tags.len() - 3}"
                        }
                    }
                }
                if *editor_popover.read() == Some(EditorPopover::Tags) {
                    div {
                        id: "editor-tags-popover",
                        class: "editor-popover metadata-popover tags-popover",
                        role: "dialog",
                        aria_label: "Edit note tags",
                        "data-transient-surface": "true",
                        "data-transient-kind": "popover",
                        "data-dismiss-outside": "true",
                        "data-return-focus": "editor-tags-trigger",
                        "data-fallback-focus": "note-editor-main",
                        p { class: "popover-label", "Tags" }
                        input {
                            "data-initial-focus": "true",
                            value: "{tags_text}",
                            aria_label: "Comma-separated tags",
                            disabled: metadata_busy,
                            oninput: {
                                let editor = editor_service.clone();
                                move |event| {
                                    let value = event.value();
                                    tags_text.set(value.clone());
                                    let mut updated = draft.read().clone();
                                    updated.tags = NoteService::parse_tags(&value);
                                    updated.updated_at = unix_timestamp();
                                    draft.set(updated.clone());
                                    schedule_draft_scoped(
                                        &editor,
                                        updated,
                                        NoteChangeScope::Tags,
                                        onstatus,
                                    );
                                }
                            }
                        }
                        small { "Separate tags with commas." }
                        div { class: "popover-footer",
                            if !note_tags.is_empty() {
                                button {
                                    class: "popover-text-button danger",
                                    disabled: metadata_busy,
                                    onclick: {
                                        let editor = editor_service.clone();
                                        move |_| {
                                            if *metadata_save_busy.read() {
                                                return;
                                            }
                                            tags_text.set(String::new());
                                            let mut updated = draft.read().clone();
                                            updated.tags.clear();
                                            updated.updated_at = unix_timestamp();
                                            draft.set(updated.clone());
                                            let pending_flush = begin_draft_flush_scoped(
                                                &editor,
                                                updated,
                                                SynchronizationMode::Checkpoint,
                                                NoteChangeScope::Tags,
                                            );
                                            let mut task_busy = metadata_save_busy;
                                            task_busy.set(true);
                                            spawn(async move {
                                                let result = complete_draft_flush(pending_flush).await;
                                                task_busy.set(false);
                                                match result {
                                                    Ok(result) => {
                                                        editor_popover.set(None);
                                                        onstatus.call(
                                                            result.completion_message("Saved locally"),
                                                        );
                                                    }
                                                    Err(error) => {
                                                        onstatus.call(format!("Save failed: {error}"));
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    "Clear"
                                }
                            }
                            button {
                                class: "popover-primary-button",
                                disabled: metadata_busy,
                                onclick: {
                                    let editor = editor_service.clone();
                                    move |_| {
                                        if *metadata_save_busy.read() {
                                            return;
                                        }
                                        let updated = draft.read().clone();
                                        let pending_flush = begin_draft_flush_scoped(
                                            &editor,
                                            updated,
                                            SynchronizationMode::Checkpoint,
                                            NoteChangeScope::Tags,
                                        );
                                        let mut task_busy = metadata_save_busy;
                                        task_busy.set(true);
                                        spawn(async move {
                                            let result = complete_draft_flush(pending_flush).await;
                                            task_busy.set(false);
                                            match result {
                                                Ok(result) => {
                                                    editor_popover.set(None);
                                                    onstatus.call(
                                                        result.completion_message("Saved locally"),
                                                    );
                                                }
                                                Err(error) => {
                                                    onstatus.call(format!("Save failed: {error}"));
                                                }
                                            }
                                        });
                                    }
                                },
                                "Done"
                            }
                        }
                    }
                }
            }

            div { class: "metadata-popover-anchor metadata-export-anchor",
                button {
                    id: "editor-export-trigger",
                    class: "metadata-chip export-chip",
                    title: "Export note or library",
                    aria_controls: "editor-export-popover",
                    aria_haspopup: "dialog",
                    aria_expanded: "{*editor_popover.read() == Some(EditorPopover::Export)}",
                    "data-transient-trigger": "true",
                    disabled: export_disabled,
                    onclick: move |_| {
                        let next = if *editor_popover.read() == Some(EditorPopover::Export) {
                            None
                        } else {
                            Some(EditorPopover::Export)
                        };
                        editor_popover.set(next);
                    },
                    "Export"
                    span { class: "chip-caret", "⌄" }
                }
                if *editor_popover.read() == Some(EditorPopover::Export) {
                    div {
                        id: "editor-export-popover",
                        class: "editor-popover metadata-popover export-popover",
                        role: "dialog",
                        aria_label: "Export",
                        "data-transient-surface": "true",
                        "data-transient-kind": "popover",
                        "data-dismiss-outside": "true",
                        "data-return-focus": "editor-export-trigger",
                        "data-fallback-focus": "note-editor-main",
                        p { class: "popover-label", "Current note" }
                        for format in PortabilityFormat::ALL {
                            button {
                                class: "popover-choice export-choice",
                                disabled: export_disabled,
                                onclick: {
                                    let context = export_context.clone();
                                    move |_| begin_metadata_export(MetadataExportAction::Note(format), context.clone())
                                },
                                span { "{format.label()}…" }
                            }
                        }
                        button {
                            class: "popover-choice export-choice",
                            disabled: export_disabled,
                            onclick: {
                                let context = export_context.clone();
                                move |_| begin_metadata_export(MetadataExportAction::NotePdf, context.clone())
                            },
                            span { "PDF…" }
                        }
                        div { class: "popover-separator" }
                        p { class: "popover-label", "Current notebook" }
                        for format in PortabilityFormat::ALL {
                            button {
                                class: "popover-choice export-choice",
                                disabled: export_disabled,
                                onclick: {
                                    let context = export_context.clone();
                                    move |_| begin_metadata_export(MetadataExportAction::Notebook(format), context.clone())
                                },
                                span { "{format.label()}…" }
                            }
                        }
                    }
                }
            }
        }

    }
}


#[derive(Clone, PartialEq)]
struct MetadataTagChipView {
    tag: String,
    notes: NoteService,
    editor: EditorService,
    draft: Signal<Note>,
    tags_text: Signal<String>,
    editor_popover: Signal<Option<EditorPopover>>,
    mutation_busy: Signal<bool>,
    privacy_epoch: super::privacy::UiPrivacyEpoch,
    privacy: PrivacyPublicationService,
    library_generation: Signal<u64>,
    note_generation: Signal<u64>,
    read_only: bool,
    editing_available: bool,
}

#[derive(Clone, PartialEq)]
struct MetadataTagChipActions {
    onchanged: EventHandler<Note>,
    onopentag: EventHandler<String>,
    ontagcatalogchanged: EventHandler<TagCatalogMutation>,
    onstatus: EventHandler<String>,
}

#[derive(Clone)]
struct MetadataTagMutationContext {
    notes: NoteService,
    editor: EditorService,
    draft: Signal<Note>,
    tags_text: Signal<String>,
    busy: Signal<bool>,
    publication_scope: super::privacy::NotePrivacyPublicationScope,
    privacy_epoch: super::privacy::UiPrivacyEpoch,
    privacy: PrivacyPublicationService,
    library_generation: Signal<u64>,
    onchanged: EventHandler<Note>,
    ontagcatalogchanged: EventHandler<TagCatalogMutation>,
    onstatus: EventHandler<String>,
}

impl MetadataTagMutationContext {
    fn remove_from_note(&self, tag_name: String) {
        let notes = self.notes.clone();
        let editor = self.editor.clone();
        let mut draft = self.draft;
        let mut tags_text = self.tags_text;
        let mut busy = self.busy;
        let onchanged = self.onchanged;
        let onstatus = self.onstatus;

        if *busy.read() || draft.read().trashed || draft.read().protection.is_locked() {
            return;
        }
        let Some(publication_token) = self.publication_scope.capture() else {
            return;
        };
        let note_id = draft.read().id.clone();
        let pending = begin_draft_flush_scoped(
            &editor,
            draft.read().clone(),
            SynchronizationMode::Checkpoint,
            NoteChangeScope::Tags,
        );
        busy.set(true);
        spawn(async move {
            if let Err(error) = complete_draft_flush(pending).await {
                busy.set(false);
                publication_token.publish(|| {
                    onstatus.call(format!("Could not finish saving tags: {error}"));
                });
                return;
            }
            let worker_notes = notes.clone();
            let worker_note_id = note_id.clone();
            let result = ApplicationExecutor::read("tag removal", move || {
                execute_tag_command_blocking(
                    &worker_notes,
                    &worker_note_id,
                    &TagCommand::RemoveFromNote { name: tag_name },
                )
            })
            .await;
            busy.set(false);
            match result {
                Ok(result) => {
                    publication_token.publish(|| {
                        let mut current = draft.read().clone();
                        current.tags = result.current_tags;
                        current.updated_at = unix_timestamp();
                        tags_text.set(current.tags_text());
                        draft.set(current.clone());
                        onchanged.call(current);
                        onstatus.call("Tag removed from note".to_string());
                    });
                }
                Err(error) => {
                    publication_token.publish(|| {
                        onstatus.call(format!("Could not remove tag: {error}"));
                    });
                }
            }
        });
    }

    fn mutate_catalog(&self, mutation: TagCatalogMutation) {
        let notes = self.notes.clone();
        let editor = self.editor.clone();
        let mut draft = self.draft;
        let mut tags_text = self.tags_text;
        let mut busy = self.busy;
        let privacy_epoch = self.privacy_epoch;
        let privacy = self.privacy.clone();
        let library_generation = self.library_generation;
        let onchanged = self.onchanged;
        let ontagcatalogchanged = self.ontagcatalogchanged;
        let onstatus = self.onstatus;

        if *busy.read() || draft.read().trashed || draft.read().protection.is_locked() {
            return;
        }
        let Some(publication_token) = self.publication_scope.capture() else {
            return;
        };
        let note_id = draft.read().id.clone();
        let pending = begin_draft_flush_scoped(
            &editor,
            draft.read().clone(),
            SynchronizationMode::Checkpoint,
            NoteChangeScope::Tags,
        );
        busy.set(true);
        spawn(async move {
            if let Err(error) = complete_draft_flush(pending).await {
                busy.set(false);
                publication_token.publish(|| {
                    onstatus.call(format!("Could not finish saving tags: {error}"));
                });
                return;
            }
            let Some(catalog_publication_token) =
                privacy_epoch.capture_multi_note_publication(&privacy, library_generation)
            else {
                busy.set(false);
                return;
            };
            let Some(_catalog_publication_lease) = catalog_publication_token.begin_publication()
            else {
                busy.set(false);
                return;
            };
            let worker_notes = notes.clone();
            let worker_note_id = note_id.clone();
            let worker_mutation = mutation.clone();
            let result = ApplicationExecutor::read("tag mutation", move || {
                let command = match worker_mutation {
                    TagCatalogMutation::Rename { from, to } => TagCommand::Rename { from, to },
                    TagCatalogMutation::Delete { name } => TagCommand::Delete { name },
                };
                execute_tag_command_blocking(&worker_notes, &worker_note_id, &command)
            })
            .await;
            busy.set(false);
            match result {
                Ok(result) => {
                    catalog_publication_token.publish(|| {
                        let affected = result.affected_notes;
                        let mut current = draft.read().clone();
                        current.tags = result.current_tags;
                        tags_text.set(current.tags_text());
                        draft.set(current.clone());
                        ontagcatalogchanged.call(mutation.clone());
                        onchanged.call(current);
                        let noun = if affected == 1 { "note" } else { "notes" };
                        let message = match mutation {
                            TagCatalogMutation::Rename { .. } => {
                                format!("Tag renamed across {affected} {noun}")
                            }
                            TagCatalogMutation::Delete { .. } => {
                                format!("Tag deleted from {affected} {noun}")
                            }
                        };
                        onstatus.call(message);
                    });
                }
                Err(error) => {
                    catalog_publication_token.publish(|| {
                        onstatus.call(format!("Could not update tag: {error}"));
                    });
                }
            }
        });
    }
}

#[component]
fn MetadataTagChip(view: MetadataTagChipView, actions: MetadataTagChipActions) -> Element {
    use super::context_menu::{
        ContextMenuController, EntityContextMenu, EntityContextMenuActions, EntityContextMenuView,
    };
    use super::entity_actions::{
        EntityAction, EntityActionContext, EntityActionRegistry, EntityActionState,
        EntityActionTargets, EntitySurface,
    };

    let MetadataTagChipView {
        tag,
        notes,
        editor,
        draft,
        tags_text,
        mut editor_popover,
        mutation_busy,
        privacy_epoch,
        privacy,
        library_generation,
        note_generation,
        read_only,
        editing_available,
    } = view;
    let MetadataTagChipActions {
        onchanged,
        onopentag,
        ontagcatalogchanged,
        onstatus,
    } = actions;
    let mutation_context = MetadataTagMutationContext {
        notes: notes.clone(),
        editor: editor.clone(),
        draft,
        tags_text,
        busy: mutation_busy,
        publication_scope: privacy_epoch.note_publication_scope(
            privacy.clone(),
            draft.peek().id.clone(),
            note_generation,
        ),
        privacy_epoch,
        privacy,
        library_generation,
        onchanged,
        ontagcatalogchanged,
        onstatus,
    };
    let mut context_menu = use_signal(ContextMenuController::default);
    let mut renaming = use_signal(|| false);
    let mut rename_value = use_signal(|| tag.clone());
    let busy = *mutation_busy.read() || !editing_available;
    let chip_id = format!("editor-tag-chip-{}", stable_dom_fragment(&tag));
    let menu_id = format!("editor-tag-menu-{}", stable_dom_fragment(&tag));
    let rename_id = format!("editor-tag-rename-{}", stable_dom_fragment(&tag));
    let action_context = EntityActionContext {
        surface: EntitySurface::Tag,
        entity_id: tag.clone(),
        state: EntityActionState {
            read_only,
            busy,
            ..EntityActionState::default()
        },
    };
    let menu_entries =
        EntityActionRegistry::entries(&action_context, &EntityActionTargets::default());

    if *renaming.read() {
        return rsx! {
            input {
                id: "{rename_id}",
                class: "metadata-tag-rename-input",
                value: "{rename_value}",
                aria_label: "Rename tag {tag}",
                autofocus: true,
                disabled: busy || read_only,
                oninput: move |event| rename_value.set(event.value()),
                oncontextmenu: move |event| event.stop_propagation(),
                onkeydown: {
                    let original = tag.clone();
                    let mutation_context = mutation_context.clone();
                    move |event: KeyboardEvent| match event.key() {
                        Key::Enter => {
                            event.prevent_default();
                            event.stop_propagation();
                            let next = rename_value.read().trim().to_string();
                            renaming.set(false);
                            if next != original {
                                mutation_context.mutate_catalog(TagCatalogMutation::Rename {
                                    from: original.clone(),
                                    to: next,
                                });
                            }
                        }
                        Key::Escape => {
                            event.prevent_default();
                            event.stop_propagation();
                            rename_value.set(original.clone());
                            renaming.set(false);
                        }
                        _ => {}
                    }
                },
                onblur: {
                    let original = tag.clone();
                    let mutation_context = mutation_context.clone();
                    move |_| {
                        if !*renaming.peek() {
                            return;
                        }
                        let next = rename_value.read().trim().to_string();
                        renaming.set(false);
                        if next != original {
                            mutation_context.mutate_catalog(TagCatalogMutation::Rename {
                                from: original.clone(),
                                to: next,
                            });
                        }
                    }
                },
            }
        };
    }

    rsx! {
        button {
            id: "{chip_id}",
            class: if busy || read_only { "metadata-chip tag-chip context-disabled" } else { "metadata-chip tag-chip" },
            title: if read_only { "Tag actions are read-only in Trash" } else { "Edit tags" },
            aria_controls: "editor-tags-popover",
            aria_haspopup: "dialog",
            aria_expanded: "{*editor_popover.read() == Some(EditorPopover::Tags)}",
            aria_disabled: if busy || read_only { "true" } else { "false" },
            "data-transient-trigger": "true",
            "data-context-long-press": "true",
            "aria-keyshortcuts": "Shift+F10",
            onclick: move |_| {
                if !busy && !read_only {
                    editor_popover.set(Some(EditorPopover::Tags));
                }
            },
            oncontextmenu: {
                let chip_id = chip_id.clone();
                move |event: MouseEvent| {
                    event.prevent_default();
                    event.stop_propagation();
                    let coordinates = event.client_coordinates();
                    let x = coordinates.x.round() as i32;
                    let y = coordinates.y.round() as i32;
                    if x == 0 && y == 0 {
                        context_menu.write().open_anchor(chip_id.clone());
                    } else {
                        context_menu.write().open_pointer(x, y);
                    }
                }
            },
            onkeydown: {
                let chip_id = chip_id.clone();
                move |event: KeyboardEvent| {
                    if !event.is_composing()
                        && event.key() == Key::F10
                        && event.modifiers().shift()
                    {
                        event.prevent_default();
                        event.stop_propagation();
                        context_menu.write().open_anchor(chip_id.clone());
                    }
                }
            },
            span { "#" }
            strong { "{tag}" }
        }
        if context_menu.read().is_open() {
            EntityContextMenu {
                view: EntityContextMenuView {
                    id: menu_id,
                    label: format!("Actions for tag {tag}"),
                    return_focus_id: chip_id.clone(),
                    fallback_focus_id: "note-editor-main".to_string(),
                    controller: context_menu,
                    entries: menu_entries,
                },
                actions: EntityContextMenuActions {
                    onaction: EventHandler::new({
                        let tag_name = tag.clone();
                        let mutation_context = mutation_context.clone();
                        let rename_id = rename_id.clone();
                        move |action| match action {
                            EntityAction::Open => onopentag.call(tag_name.clone()),
                            EntityAction::Rename => {
                                rename_value.set(tag_name.clone());
                                renaming.set(true);
                                focus_and_select_metadata_input(rename_id.clone());
                            }
                            EntityAction::RemoveTagFromNote => {
                                mutation_context.remove_from_note(tag_name.clone())
                            },
                            EntityAction::DeleteTag => {
                                mutation_context.mutate_catalog(TagCatalogMutation::Delete {
                                    name: tag_name.clone(),
                                })
                            },
                            _ => {}
                        }
                    }),
                },
            }
        }
    }
}

fn stable_dom_fragment(value: &str) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(value.len().saturating_mul(2).max(1));
    for byte in value.as_bytes() {
        let _ = write!(&mut result, "{byte:02x}");
    }
    result
}

fn focus_and_select_metadata_input(element_id: String) {
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let script = format!(
            "const input = document.getElementById({element_id:?}); if (input) {{ input.focus(); input.select(); return true; }} return false;"
        );
        let _ = document::eval(&script).join::<bool>().await;
    });
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NoteRevisionRestoreRequest {
    pub revision_id: String,
    pub editor_state: Arc<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RevisionLimitChange {
    Count(i64),
    Age(i64),
    StorageBytes(i64),
}

const REVISION_WINDOW_SIZE: usize = 80;
const REVISION_PREVIEW_CACHE_SIZE: usize = 8;

#[derive(Debug, PartialEq, Eq)]
struct RevisionPreview {
    snapshot: RevisionSnapshot,
    html: String,
}

#[derive(Debug)]
struct AuthorizedRevisionPreview {
    preview: RevisionPreview,
    _authorization: RevisionSnapshotResource,
}

struct RevisionLimitContext {
    notes: NoteService,
    note_id: NoteId,
    retention: Signal<RevisionRetention>,
    revisions: Signal<Vec<NoteRevisionSummary>>,
    selected_revision_id: Signal<Option<String>>,
    storage_usage: Signal<crate::domain::RevisionStorageUsage>,
    history_operation: Signal<Option<ApplicationOperation>>,
    background_coordinator: ApplicationExecutor,
    publication_token: super::privacy::PrivacyPublicationToken,
    onstatus: EventHandler<String>,
}

#[derive(Clone, PartialEq)]
pub(super) struct RevisionHistoryPanelView {
    pub note_id: NoteId,
    pub notebooks: Vec<Notebook>,
    pub notes: NoteService,
    pub restoring: bool,
    pub read_only: bool,
    pub history_operation: Signal<Option<ApplicationOperation>>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub library_generation: Signal<u64>,
    pub note_generation: Signal<u64>,
}

#[derive(Clone, PartialEq)]
pub(super) struct RevisionHistoryPanelActions {
    pub onrestore: EventHandler<NoteRevisionRestoreRequest>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn RevisionHistoryPanel(
    view: RevisionHistoryPanelView,
    actions: RevisionHistoryPanelActions,
) -> Element {
    let RevisionHistoryPanelView {
        note_id,
        notebooks,
        notes,
        restoring,
        read_only,
        history_operation,
        privacy_epoch,
        library_generation,
        note_generation,
    } = view;
    let RevisionHistoryPanelActions { onrestore, onstatus } = actions;
    let background_coordinator = use_context::<ApplicationExecutor>();
    let history_notes = notes.clone();
    let history_note_id = note_id.clone();
    let initial_history_status = onstatus;
    let revisions = use_signal(move || {
        storage_or_report(
            history_notes.revision_summaries(&history_note_id),
            "Could not load revision history",
            initial_history_status,
        )
        .unwrap_or_default()
    });
    let mut selected_revision_id = use_signal(|| None::<String>);
    let mut revision_window_start = use_signal(|| 0_usize);
    let mut preview_cache = use_signal(HashMap::<String, Arc<RevisionPreview>>::new);
    let mut preview_cache_order = use_signal(Vec::<String>::new);
    let mut preview_loading_id = use_signal(|| None::<String>);
    let mut preview_error = use_signal(|| None::<(String, String)>);
    let mut preview_request_generation = use_signal(|| 0_u64);
    let mut preview_publication_token =
        use_signal(|| None::<super::privacy::PrivacyPublicationToken>);
    let privacy = use_context::<PrivacyPublicationService>();
    let note_privacy = super::privacy::use_note_privacy(privacy_epoch, note_id.clone());
    let _privacy_reactivity = *note_privacy.read();
    let previews_are_current = preview_publication_token
        .read()
        .as_ref()
        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable);

    let retention_notes = notes.clone();
    let initial_retention_status = onstatus;
    let retention = use_signal(move || {
        storage_or_report(
            retention_notes.revision_retention(),
            "Could not load revision retention settings",
            initial_retention_status,
        )
        .unwrap_or_default()
    });
    let usage_notes = notes.clone();
    let initial_usage_status = onstatus;
    let storage_usage = use_signal(move || {
        storage_or_report(
            usage_notes.revision_storage_usage(),
            "Could not load revision storage usage",
            initial_usage_status,
        )
        .unwrap_or_default()
    });
    let history_busy = history_operation.read().is_some();
    let count_limit_background = background_coordinator.clone();
    let age_limit_background = background_coordinator.clone();
    let storage_limit_background = background_coordinator.clone();
    let count_limit_privacy = privacy.clone();
    let age_limit_privacy = privacy.clone();
    let storage_limit_privacy = privacy.clone();

    let revision_items = revisions.read().clone();
    let selected_revision = selected_revision_id
        .read()
        .as_ref()
        .and_then(|id| revision_items.iter().find(|revision| &revision.id == id))
        .cloned()
        .or_else(|| revision_items.first().cloned());
    let selected_id = selected_revision.as_ref().map(|revision| revision.id.clone());

    let revision_privacy = note_privacy;
    let _privacy_invalidator = use_effect(move || {
        let _privacy_generation = *revision_privacy.read();
        // Decrypted revision previews are process-memory plaintext. A privacy
        // transition for this note (or the whole library) invalidates them; an
        // unrelated note transition does not.
        preview_cache.write().clear();
        preview_cache_order.write().clear();
        preview_publication_token.set(None);
        preview_loading_id.set(None);
        preview_error.set(None);
        let next = (*preview_request_generation.peek()).wrapping_add(1);
        preview_request_generation.set(next);
    });

    let preview_notes = notes.clone();
    let preview_note_id = note_id.clone();
    let _preview_loader = use_effect(move || {
        let Some(publication_token) = privacy_epoch.capture_note_publication(
            &privacy,
            &preview_note_id,
            note_generation,
        ) else {
            preview_loading_id.set(None);
            return;
        };
        let revision_id = selected_revision_id
            .read()
            .clone()
            .or_else(|| revisions.read().first().map(|revision| revision.id.clone()));
        let Some(revision_id) = revision_id else {
            return;
        };
        if preview_cache.read().contains_key(&revision_id)
            || preview_loading_id.read().as_ref() == Some(&revision_id)
            || preview_error
                .read()
                .as_ref()
                .is_some_and(|(error_id, _)| error_id == &revision_id)
        {
            return;
        }
        let request_generation = (*preview_request_generation.read()).wrapping_add(1);
        preview_request_generation.set(request_generation);
        preview_loading_id.set(Some(revision_id.clone()));
        preview_error.set(None);
        let notes = preview_notes.clone();
        let note_id = preview_note_id.clone();
        spawn(async move {
            let task_revision_id = revision_id.clone();
            let result = ApplicationExecutor::read(
                "revision preview",
                move || -> Result<AuthorizedRevisionPreview, RevisionPreviewError> {
                    let authorization = notes
                        .revision_snapshot_resource(&note_id, &task_revision_id)
                        .map_err(RevisionPreviewError::from)?;
                    let snapshot = authorization.snapshot().clone();
                    let projections =
                        derive_editor_projections(snapshot.note.editor_state.as_str())
                            .map_err(RevisionPreviewError::from)?;
                    let html = render_markdown(&projections.markdown);
                    Ok(AuthorizedRevisionPreview {
                        preview: RevisionPreview { snapshot, html },
                        _authorization: authorization,
                    })
                },
            )
            .await;

            if *preview_request_generation.read() != request_generation {
                return;
            }

            match result {
                Ok(authorized_preview) => {
                    if !authorized_preview._authorization.is_publishable() {
                        return;
                    }
                    let Some(_publication_lease) = publication_token.begin_publication() else {
                        return;
                    };
                    let AuthorizedRevisionPreview {
                        preview,
                        _authorization,
                    } = authorized_preview;
                    preview_publication_token.set(Some(publication_token.clone()));
                    {
                        let mut cache = preview_cache.write();
                        cache.insert(revision_id.clone(), Arc::new(preview));
                    }
                    {
                        let mut order = preview_cache_order.write();
                        order.retain(|id| id != &revision_id);
                        order.push(revision_id.clone());
                        while order.len() > REVISION_PREVIEW_CACHE_SIZE {
                            let removed = order.remove(0);
                            preview_cache.write().remove(&removed);
                        }
                    }
                    if preview_error
                        .read()
                        .as_ref()
                        .is_some_and(|(id, _)| id == &revision_id)
                    {
                        preview_error.set(None);
                    }
                }
                Err(error) => preview_error.set(Some((revision_id.clone(), error.to_string()))),
            }
            if preview_loading_id.read().as_ref() == Some(&revision_id)
                && *preview_request_generation.read() == request_generation
            {
                preview_loading_id.set(None);
            }
        });
    });

    let selected_preview = if previews_are_current {
        selected_id
            .as_ref()
            .and_then(|id| preview_cache.read().get(id).cloned())
    } else {
        None
    };
    let selected_snapshot = selected_preview
        .as_ref()
        .map(|preview| &preview.snapshot.note);
    let selected_attachment_count = selected_preview
        .as_ref()
        .map(|preview| preview.snapshot.attachments.len())
        .unwrap_or_default();
    let selected_preview_html = selected_preview
        .as_ref()
        .map(|preview| preview.html.clone())
        .unwrap_or_default();
    let selected_snapshot_error = previews_are_current
        .then_some(selected_id.as_ref())
        .flatten()
        .and_then(|id| {
        preview_error
            .read()
            .as_ref()
            .filter(|(error_id, _)| error_id == id)
            .map(|(_, error)| error.clone())
    });
    let selected_loading = !previews_are_current
        || selected_id
            .as_ref()
            .is_some_and(|id| preview_loading_id.read().as_ref() == Some(id));
    let selected_title = selected_snapshot
        .as_ref()
        .map(|note| note.display_title().to_string())
        .unwrap_or_else(|| "Loading version…".to_string());
    let selected_tags = selected_snapshot
        .as_ref()
        .map(|note| note.tags_text())
        .unwrap_or_default();
    let selected_folder = selected_snapshot
        .as_ref()
        .and_then(|note| {
            notebooks
                .iter()
                .find(|notebook| notebook.id == note.notebook_id)
        })
        .map(|notebook| notebook.name.clone())
        .unwrap_or_else(|| "Notes".to_string());
    let selected_state = selected_snapshot
        .as_ref()
        .map(|note| {
            let mut states = Vec::new();
            if note.pinned {
                states.push("Pinned");
            }
            if note.archived {
                states.push("Archived");
            }
            if note.trashed {
                states.push("Trash");
            }
            if note.purge_after.is_some() {
                states.push("Deletion scheduled");
            }
            if states.is_empty() {
                "Active".to_string()
            } else {
                states.join(" · ")
            }
        })
        .unwrap_or_default();

    let total_revisions = revision_items.len();
    let max_window_start = total_revisions.saturating_sub(1);
    let window_start = (*revision_window_start.read()).min(max_window_start);
    let window_end = window_start
        .saturating_add(REVISION_WINDOW_SIZE)
        .min(total_revisions);
    let visible_revisions = revision_items[window_start..window_end].to_vec();
    let has_newer_window = window_start > 0;
    let has_older_window = window_end < total_revisions;
    let newer_window_start = window_start.saturating_sub(REVISION_WINDOW_SIZE);
    let newer_window_selection = revision_items
        .get(newer_window_start)
        .map(|revision| revision.id.clone());
    let older_window_start = window_end;
    let older_window_selection = revision_items
        .get(older_window_start)
        .map(|revision| revision.id.clone());
    rsx! {
        div {
            class: "history-content",
            aria_busy: if restoring || history_busy || selected_loading { "true" } else { "false" },
                nav { class: "revision-list",
                    if revision_items.is_empty() {
                        div { class: "history-empty",
                            strong { "No previous versions yet" }
                            span { "A version is created after meaningful edits and explicit saves." }
                        }
                    } else {
                        if has_newer_window {
                            button {
                                class: "revision-window-button",
                                onclick: move |_| {
                                    revision_window_start.set(newer_window_start);
                                    if let Some(revision_id) = newer_window_selection.clone() {
                                        selected_revision_id.set(Some(revision_id));
                                    }
                                },
                                "Show newer versions"
                            }
                        }
                        for revision in visible_revisions.iter() {
                            RevisionRow {
                                key: "{revision.id}",
                                revision: revision.clone(),
                                active: selected_revision
                                    .as_ref()
                                    .is_some_and(|selected| selected.id == revision.id),
                                onclick: {
                                    let revision_id = revision.id.clone();
                                    move |_| selected_revision_id.set(Some(revision_id.clone()))
                                },
                            }
                        }
                        if has_older_window {
                            button {
                                class: "revision-window-button",
                                onclick: move |_| {
                                    revision_window_start.set(older_window_start);
                                    if let Some(revision_id) = older_window_selection.clone() {
                                        selected_revision_id.set(Some(revision_id));
                                    }
                                },
                                "Show older versions"
                            }
                        }
                        if total_revisions > REVISION_WINDOW_SIZE {
                            small { class: "revision-window-status",
                                "Showing {window_start + 1}–{window_end} of {total_revisions}"
                            }
                        }
                    }
                }

                section { class: "revision-preview",
                    if let Some(revision) = selected_revision.as_ref() {
                        div { class: "revision-preview-header",
                            div {
                                span { class: "revision-number", "Version {revision.revision_no}" }
                                h3 { "{selected_title}" }
                                p {
                                    "{relative_time(revision.created_at)}"
                                    if !selected_tags.is_empty() {
                                        " · {selected_tags}"
                                    }
                                    if selected_snapshot.is_some() {
                                        " · {selected_attachment_count} attachment(s)"
                                        " · {selected_folder} · {selected_state}"
                                    }
                                }
                            }
                            button {
                                class: "restore-version-button",
                                disabled: read_only || restoring || history_busy || selected_snapshot.is_none(),
                                aria_busy: if restoring { "true" } else { "false" },
                                onclick: {
                                    let revision_id = revision.id.clone();
                                    let editor_state = selected_snapshot
                                        .as_ref()
                                        .map(|note| note.editor_state.clone())
                                        .unwrap_or_default();
                                    move |_| {
                                        if !read_only
                                            && !restoring
                                            && !history_busy
                                            && !editor_state.is_empty()
                                        {
                                            onrestore.call(NoteRevisionRestoreRequest {
                                                revision_id: revision_id.clone(),
                                                editor_state: editor_state.clone(),
                                            });
                                        }
                                    }
                                },
                                if read_only {
                                    "Recover note to restore"
                                } else if restoring {
                                    "Restoring…"
                                } else {
                                    "Restore note revision"
                                }
                            }
                        }
                        if selected_snapshot.is_some() {
                            p { class: "revision-restore-scope",
                                "Restores this note’s title, content, notebook, tags, state, and versioned attachments. Document links follow the restored content; hierarchy, sources, and other external relationships stay current."
                            }
                        }
                        if selected_loading {
                            div { class: "history-empty large",
                                strong { "Loading version…" }
                                span { "The snapshot and preview are being prepared in the background." }
                            }
                        } else if let Some(error) = selected_snapshot_error {
                            div { class: "history-empty large",
                                strong { "Could not load this revision" }
                                span { "{error}" }
                                button {
                                    class: "revision-window-button",
                                    onclick: move |_| preview_error.set(None),
                                    "Retry"
                                }
                            }
                        } else if selected_snapshot.is_some() {
                            article {
                                class: "revision-markdown",
                                dangerous_inner_html: selected_preview_html
                            }
                        } else {
                            div { class: "history-empty large",
                                strong { "Select a version" }
                                span { "Choose a saved version to inspect it before restoring." }
                            }
                        }
                    } else {
                        div { class: "history-empty large",
                            strong { "Select a version" }
                            span { "Choose a saved version to inspect it before restoring." }
                        }
                    }
                }
            }

            footer { class: "history-retention",
                div {
                    strong { "Retention" }
                    span {
                        if history_busy {
                            "Applying limits in the background…"
                        } else {
                            "Limits are applied immediately to every note."
                        }
                    }
                }
                label {
                    span { "Keep" }
                    select {
                        value: "{retention.read().max_revisions_per_note}",
                        disabled: restoring || history_busy || read_only,
                        onchange: {
                            let notes = notes.clone();
                            let note_id = note_id.clone();
                            move |event| {
                                let Ok(next_count) = event.value().parse::<i64>() else {
                                    return;
                                };
                                schedule_revision_limit_change(
                                    RevisionLimitChange::Count(next_count),
                                    RevisionLimitContext {
                                        notes: notes.clone(),
                                        note_id: note_id.clone(),
                                        retention,
                                        revisions,
                                        selected_revision_id,
                                        storage_usage,
                                        history_operation,
                                        background_coordinator: count_limit_background.clone(),
                                        publication_token: match privacy_epoch.capture_multi_note_publication_with_lifecycle(
                                            &count_limit_privacy,
                                            library_generation,
                                            note_generation,
                                        ) {
                                            Some(token) => token,
                                            None => return,
                                        },
                                        onstatus,
                                    },
                                );
                            }
                        },
                        option { value: "50", "50 versions" }
                        option { value: "100", "100 versions" }
                        option { value: "250", "250 versions" }
                        option { value: "500", "500 versions" }
                        option { value: "1000", "1000 versions" }
                    }
                }
                label {
                    span { "Age" }
                    select {
                        value: "{retention.read().max_age_days}",
                        disabled: restoring || history_busy || read_only,
                        onchange: {
                            let notes = notes.clone();
                            let note_id = note_id.clone();
                            move |event| {
                                let Ok(next_age) = event.value().parse::<i64>() else {
                                    return;
                                };
                                schedule_revision_limit_change(
                                    RevisionLimitChange::Age(next_age),
                                    RevisionLimitContext {
                                        notes: notes.clone(),
                                        note_id: note_id.clone(),
                                        retention,
                                        revisions,
                                        selected_revision_id,
                                        storage_usage,
                                        history_operation,
                                        background_coordinator: age_limit_background.clone(),
                                        publication_token: match privacy_epoch.capture_multi_note_publication_with_lifecycle(
                                            &age_limit_privacy,
                                            library_generation,
                                            note_generation,
                                        ) {
                                            Some(token) => token,
                                            None => return,
                                        },
                                        onstatus,
                                    },
                                );
                            }
                        },
                        option { value: "30", "30 days" }
                        option { value: "90", "90 days" }
                        option { value: "180", "180 days" }
                        option { value: "365", "1 year" }
                        option { value: "0", "Forever" }
                    }
                }
                label {
                    span { "History storage" }
                    select {
                        value: "{retention.read().max_total_bytes}",
                        disabled: restoring || history_busy || read_only,
                        onchange: {
                            let notes = notes.clone();
                            let note_id = note_id.clone();
                            move |event| {
                                let Ok(next_bytes) = event.value().parse::<i64>() else {
                                    return;
                                };
                                schedule_revision_limit_change(
                                    RevisionLimitChange::StorageBytes(next_bytes),
                                    RevisionLimitContext {
                                        notes: notes.clone(),
                                        note_id: note_id.clone(),
                                        retention,
                                        revisions,
                                        selected_revision_id,
                                        storage_usage,
                                        history_operation,
                                        background_coordinator: storage_limit_background.clone(),
                                        publication_token: match privacy_epoch.capture_multi_note_publication_with_lifecycle(
                                            &storage_limit_privacy,
                                            library_generation,
                                            note_generation,
                                        ) {
                                            Some(token) => token,
                                            None => return,
                                        },
                                        onstatus,
                                    },
                                );
                            }
                        },
                        option { value: "67108864", "64 MB" }
                        option { value: "134217728", "128 MB" }
                        option { value: "268435456", "256 MB" }
                        option { value: "536870912", "512 MB" }
                        option { value: "1073741824", "1 GB" }
                    }
                    small {
                        "Using {format_file_size(storage_usage.read().total_bytes)} across {storage_usage.read().revision_count} version(s)"
                    }
                }
            }
    }
}

fn schedule_revision_limit_change(
    change: RevisionLimitChange,
    context: RevisionLimitContext,
) {
    let RevisionLimitContext {
        notes,
        note_id,
        mut retention,
        mut revisions,
        mut selected_revision_id,
        mut storage_usage,
        mut history_operation,
        background_coordinator,
        publication_token,
        onstatus,
    } = context;
    let operation = ApplicationOperation::SaveHistoryLimits;
    if history_operation.read().is_some() {
        return;
    }
    history_operation.set(Some(operation));
    onstatus.call(operation.progress_label().to_string());
    let current = *retention.read();
    spawn(async move {
        let task_notes = notes.clone();
        let task_note_id = note_id.clone();
        let result = background_coordinator.execute(
            operation,
            move || -> Result<
                (RevisionRetention, usize, Vec<NoteRevisionSummary>, RevisionStorageUsage),
                super::task_error::DataTaskError,
            > {
                let (next_retention, removed) = match change {
                RevisionLimitChange::Count(count) => {
                    let removed = task_notes
                        .set_revision_retention(count, current.max_age_days)
                        .map_err(super::task_error::DataTaskError::from)?;
                    (
                        RevisionRetention {
                            max_revisions_per_note: count,
                            ..current
                        },
                        removed,
                    )
                }
                RevisionLimitChange::Age(age) => {
                    let removed = task_notes
                        .set_revision_retention(current.max_revisions_per_note, age)
                        .map_err(super::task_error::DataTaskError::from)?;
                    (
                        RevisionRetention {
                            max_age_days: age,
                            ..current
                        },
                        removed,
                    )
                }
                RevisionLimitChange::StorageBytes(bytes) => {
                    let removed = task_notes
                        .set_revision_storage_limit(bytes)
                        .map_err(super::task_error::DataTaskError::from)?;
                    (
                        RevisionRetention {
                            max_total_bytes: bytes,
                            ..current
                        },
                        removed,
                    )
                }
            };
            let refreshed = task_notes
                .revision_summaries(&task_note_id)
                .map_err(super::task_error::DataTaskError::from)?;
            let usage = task_notes
                .revision_storage_usage()
                .map_err(super::task_error::DataTaskError::from)?;
                Ok((next_retention, removed, refreshed, usage))
            },
        )
        .await;

        history_operation.set(None);
        match result {
            Ok((next_retention, removed, refreshed, usage)) => {
                publication_token.publish(|| {
                    let previous_selection = selected_revision_id.read().clone();
                    let next_selection = previous_selection
                        .filter(|selected| refreshed.iter().any(|revision| revision.id == *selected))
                        .or_else(|| refreshed.first().map(|revision| revision.id.clone()));
                    retention.set(next_retention);
                    revisions.set(refreshed);
                    selected_revision_id.set(next_selection);
                    storage_usage.set(usage);
                    onstatus.call(format!(
                        "Version-history limits applied; removed {removed} old version(s)"
                    ));
                });
            }
            Err(error) => {
                publication_token.publish(|| {
                    onstatus.call(format!("Could not apply version-history limits: {error}"));
                });
            }
        }
    });
}

#[component]
pub(super) fn RevisionRow(
    revision: NoteRevisionSummary,
    active: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: if active { "revision-row active" } else { "revision-row" },
            onclick: move |_| onclick.call(()),
            div { class: "revision-row-top",
                strong { "Version {revision.revision_no}" }
                span { "{relative_time(revision.created_at)}" }
            }
            span { class: "revision-row-title", "Saved version" }
            p { "{format_file_size(revision.byte_size)} snapshot" }
        }
    }
}

pub(super) fn format_file_size(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    if bytes < 1024.0 {
        return format!("{} B", bytes as u64);
    }
    if bytes < 1024.0 * 1024.0 {
        return format!("{:.1} KB", bytes / 1024.0);
    }
    if bytes < 1024.0 * 1024.0 * 1024.0 {
        return format!("{:.1} MB", bytes / (1024.0 * 1024.0));
    }
    format!("{:.1} GB", bytes / (1024.0 * 1024.0 * 1024.0))
}
