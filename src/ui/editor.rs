use std::sync::atomic::{AtomicU64, Ordering};

use crate::application::attachments::{AttachmentFileCommand, AttachmentFileCommandOutcome};
use crate::application::context::ApplicationServices;
use crate::application::editor::EditorService;
use crate::application::editor_workflows::MediaCompatibilityWorkflowAction;
use crate::application::persistence::{NoteChangeScope, unix_timestamp};
use crate::application::settings::EditorPreferences;
use crate::domain::{Note, NoteId, NoteLinkTarget, NoteTitleStyle, Notebook};
use crate::editor_bridge::{
    EditorAttachmentCommand, EditorAttachmentFileAction, EditorDocumentUpdate,
    EditorNoteLinkCommand,
};
use crate::search::{ParsedSearch, SearchField};
use dioxus::core::use_drop;
use dioxus::prelude::*;

use super::editor_toolbar::{EditorToolbar, EditorToolbarActions, EditorToolbarView};
use super::editor_toolbar_support::{
    FindPanel, FindPanelView, TextStatistics, TitleTypographyCommand,
};
use super::editor_workflows::{
    AttachmentWorkflowPresentation, ImageImportFlow, RevisionRestoreFlow, reject_image_edit,
    reject_image_import, restore_revision_flow, run_attachment_import, run_image_edit,
    run_image_import, run_media_compatibility, run_pdf_export, run_pending_attachment_open,
    run_pending_attachment_recovery,
};
use super::filter_popover::apply_note_filters;
use super::live_editor::{
    LiveEditorActions, LiveEditorView, NoteDocumentEditor,
    live_editor_complete_note_link_picker_script, live_editor_dispose_script,
    live_editor_fail_attachment_picker_script, live_editor_fail_note_link_picker_script,
    live_editor_find_script, live_editor_set_attachment_registry_script,
    live_editor_set_editable_script, live_editor_set_note_link_registry_script,
};
use super::note_hierarchy::{
    NoteHierarchyPanel, NoteHierarchyPanelActions, NoteHierarchyPanelView,
};
use super::note_links::{
    BacklinksPanel, BacklinksPanelActions, BacklinksPanelView, NoteLinkPicker,
    NoteLinkPickerActions, NoteLinkPickerRequest, NoteLinkPickerView,
};
use super::note_list_data::{current_note_list_query, refresh_note_list, select_note};
use super::note_metadata::{
    NoteRevisionRestoreRequest, RevisionHistoryPanel, RevisionHistoryPanelActions,
    RevisionHistoryPanelView,
};
use super::note_properties::{
    NotePropertiesPanel, NotePropertiesPanelActions, NotePropertiesPanelView,
};
use super::responsive::{focus_editor_panel, focus_note_find_input, focus_note_list_panel};
use super::right_panel::{
    AttachmentsPanel, AttachmentsPanelActions, AttachmentsPanelView, DEFAULT_RIGHT_PANEL_WIDTH,
    OutlinePanel, RightPanelMode, RightPanelShell, clamp_right_panel_width, focus_live_editor,
    persist_right_panel_width,
};
use super::sources::{SourcesPanel, SourcesPanelActions, SourcesPanelView};
use super::synchronization::{
    EditorSession, ProjectedEditorDocumentUpdate, SynchronizationMode, apply_editor_update,
    begin_draft_flush_scoped, complete_draft_flush, project_editor_update_async,
};
use crate::application::executor::{
    ApplicationExecutionStatus, ApplicationExecutor, ApplicationOperation,
};

use super::toast::status_bar_class;
use super::{
    CompactPane, EditorPopover, FormatAction, NoteCardAction, NotesListState, TagCatalogMutation,
    refresh_tags, request_note_list_preferences_save, storage_or_report,
};

static NEXT_EDITOR_GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_editor_generation() -> u64 {
    NEXT_EDITOR_GENERATION.fetch_add(1, Ordering::Relaxed)
}

fn redact_note_for_privacy_transition(note: &Note) -> Note {
    let mut redacted = note.clone();
    redacted.protection = crate::domain::NoteProtectionState::Locked;
    redacted.title = "Encrypted note".to_string();
    redacted.title_style = NoteTitleStyle::default();
    redacted.document = Default::default();
    redacted.tags.clear();
    redacted.attachment_names.clear();
    redacted
}

fn refresh_editor_aggregate_surfaces_when_privacy_ready(state: NotesListState) {
    state.spawn(async move {
        let Some(publication_token) = state.wait_for_multi_note_publication_token(None).await
        else {
            return;
        };
        let Some(_publication_lease) = publication_token.begin_publication() else {
            return;
        };
        let library = state.services.peek().library();
        let _ = refresh_tags(&library, state.tags, state.status);
        refresh_note_list(state);
    });
}

fn title_style_css(style: &NoteTitleStyle) -> String {
    let font_family = match style.font_family.as_str() {
        "serif" => "Georgia, 'Times New Roman', serif",
        "rounded" => "ui-rounded, 'SF Pro Rounded', system-ui, sans-serif",
        "mono" => "ui-monospace, 'SFMono-Regular', Menlo, Consolas, monospace",
        _ => "Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
    };
    let color = match style.text_color.as_str() {
        "grey" => "var(--editor-text-grey)",
        "red" => "var(--editor-text-red)",
        "orange" => "var(--editor-text-orange)",
        "green" => "var(--editor-text-green)",
        "blue" => "var(--editor-text-blue)",
        "purple" => "var(--editor-text-purple)",
        _ => "var(--text)",
    };
    let mut decorations = Vec::with_capacity(2);
    if style.underline {
        decorations.push("underline");
    }
    if style.strikethrough {
        decorations.push("line-through");
    }
    let decoration = if decorations.is_empty() {
        "none".to_string()
    } else {
        decorations.join(" ")
    };
    let size = style
        .font_size
        .map(|size| format!("font-size:{size}pt;"))
        .unwrap_or_default();
    format!(
        "font-family:{font_family};font-weight:{};font-style:{};color:{color};-webkit-text-fill-color:{color};text-decoration-line:{decoration};{size}",
        if style.bold { 750 } else { 400 },
        if style.italic { "italic" } else { "normal" },
    )
}

fn apply_title_typography(style: &mut NoteTitleStyle, command: TitleTypographyCommand) {
    match command {
        TitleTypographyCommand::Toggle(FormatAction::Bold) => style.bold = !style.bold,
        TitleTypographyCommand::Toggle(FormatAction::Italic) => style.italic = !style.italic,
        TitleTypographyCommand::Toggle(FormatAction::Underline) => {
            style.underline = !style.underline
        }
        TitleTypographyCommand::Toggle(FormatAction::Strikethrough) => {
            style.strikethrough = !style.strikethrough;
        }
        TitleTypographyCommand::Toggle(_) => return,
        TitleTypographyCommand::SetFontFamily(font_family) => style.font_family = font_family,
        TitleTypographyCommand::SetFontSize(size) => style.font_size = Some(size),
        TitleTypographyCommand::AdjustFontSize(delta) => {
            let current = style.font_size.unwrap_or(28);
            style.font_size = Some(
                i16::from(current)
                    .saturating_add(i16::from(delta))
                    .clamp(8, 72) as u8,
            );
        }
        TitleTypographyCommand::SetTextColor(text_color) => style.text_color = text_color,
        TitleTypographyCommand::Clear => *style = NoteTitleStyle::default(),
    }
    *style = std::mem::take(style).normalized();
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct EditorPaneView {
    pub services: Signal<ApplicationServices>,
    pub notebooks: Signal<Vec<Notebook>>,
    pub library_state: NotesListState,
    pub compact_pane: Signal<CompactPane>,
    pub note_find_request: Signal<u64>,
    pub save_status: Signal<String>,
    pub persistence_generation: Signal<u64>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
}

#[derive(Clone, PartialEq)]
struct LockedNotePaneView {
    note: Note,
    services: ApplicationServices,
    library_state: NotesListState,
}

#[component]
fn VaultTransitionPane(transition: super::VaultTransitionUiState) -> Element {
    rsx! {
        section {
            class: "locked-note-pane",
            aria_label: "Secure note transition",
            aria_busy: "true",
            div { class: "locked-note-card",
                div { class: "locked-note-icon", aria_hidden: "true", "🔒" }
                h2 { "{transition.public_label}" }
                p { "{transition.message}" }
                p { "The private note content has been removed from the interface while Rustnote finishes the secure transition." }
            }
        }
    }
}

#[component]
fn LockedNotePane(view: LockedNotePaneView) -> Element {
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut password_for_drop = password;
    use_drop(move || {
        crate::security::clear_secret_input(&mut password_for_drop.write());
    });
    let note_id = view.note.id.clone();
    let locked_label = view.note.display_title().to_string();
    let services = view.services.clone();
    let library_state = view.library_state;

    rsx! {
        section {
            class: "locked-note-pane",
            aria_label: "Locked encrypted note",
            div { class: "locked-note-card",
                div { class: "locked-note-icon", aria_hidden: "true", "🔒" }
                h2 { "{locked_label}" }
                p { "This note is encrypted. Enter its password to unlock it for this Rustnote session." }
                label { class: "vault-dialog-field",
                    span { "Password" }
                    input {
                        r#type: "password",
                        value: "{password}",
                        autofocus: true,
                        autocomplete: "current-password",
                        oninput: move |event| {
                            crate::security::replace_secret_input(
                                &mut password.write(),
                                event.value(),
                            );
                            error.set(None);
                        },
                        onkeydown: {
                            let note_id = note_id.clone();
                            let services = services.clone();
                            move |event: KeyboardEvent| {
                                if event.key() != Key::Enter {
                                    return;
                                }
                                event.prevent_default();
                                let entered =
                                    crate::security::SecretString::take_from(&mut password.write());
                                if entered.is_empty() {
                                    error.set(Some("Enter the note password.".to_string()));
                                    return;
                                }
                                super::note_card::apply_note_card_action(
                                    services.clone(),
                                    NoteCardAction::UnlockEncrypted {
                                        note_id: note_id.clone(),
                                        password: entered,
                                    },
                                    library_state,
                                );
                            }
                        },
                    }
                }
                if let Some(message) = error.read().as_ref() {
                    p { class: "vault-dialog-error", role: "alert", "{message}" }
                }
                button {
                    class: "vault-dialog-button primary",
                    onclick: {
                        let note_id = note_id.clone();
                        let services = services.clone();
                        move |_| {
                            let entered =
                                crate::security::SecretString::take_from(&mut password.write());
                            if entered.is_empty() {
                                error.set(Some("Enter the note password.".to_string()));
                                return;
                            }
                            super::note_card::apply_note_card_action(
                                services.clone(),
                                NoteCardAction::UnlockEncrypted {
                                    note_id: note_id.clone(),
                                    password: entered,
                                },
                                library_state,
                            );
                        }
                    },
                    "Unlock note"
                }
            }
        }
    }
}

#[component]
pub(super) fn EditorPane(view: EditorPaneView) -> Element {
    let EditorPaneView {
        services,
        notebooks,
        mut library_state,
        mut compact_pane,
        note_find_request,
        save_status,
        persistence_generation,
        privacy_epoch,
    } = view;
    let NotesListState {
        search,
        mut selected_note,
        vault_transition,
        note_view_generation,
        tag_editor_request,
        mut status,
        editor_session,
        ..
    } = library_state;
    let view_generation = *note_view_generation.read();
    let status_text = save_status.read().clone();
    let status_class = status_bar_class(&status_text);
    let mut right_panel_mode = use_signal(|| None::<RightPanelMode>);
    let right_panel_last_mode = use_signal(|| RightPanelMode::Outline);
    let right_panel_settings = services.read().settings();
    let initial_right_panel_status = EventHandler::new(move |message: String| status.set(message));
    let right_panel_width = use_signal(move || {
        storage_or_report(
            right_panel_settings.right_panel_width(),
            "Could not load right-panel width",
            initial_right_panel_status,
        )
        .map(clamp_right_panel_width)
        .unwrap_or(DEFAULT_RIGHT_PANEL_WIDTH)
    });
    let _empty_editor_cleanup = use_effect(move || {
        if selected_note.read().is_none() {
            right_panel_mode.set(None);
            let _ = document::eval(&live_editor_dispose_script(false));
        }
    });

    rsx! {
        main { id: "note-editor-main", class: "editor-panel", tabindex: "-1", aria_label: "Note editor",
            if let Some(note) = selected_note.read().clone() {
                if note.protection.is_locked() {
                    LockedNotePane {
                        key: "locked-{note.id}-{view_generation}",
                        view: LockedNotePaneView {
                            note,
                            services: services.read().clone(),
                            library_state,
                        },
                    }
                } else {
                NoteEditor {
                    key: "note-{note.id}-{view_generation}",
                    view: EditorProps {
                        note,
                        selected_note,
                        notebooks: notebooks.read().clone(),
                        services: services.read().clone(),
                        global_query: search.read().clone(),
                        find_request: *note_find_request.read(),
                        save_status: status_text.clone(),
                        save_status_class: status_class,
                        right_panel_mode,
                        right_panel_last_mode,
                        right_panel_width,
                        editor_session,
                        tag_editor_request,
                        library_generation: library_state.list_generation,
                        status_note_generation: library_state.note_selection_generation,
                        persistence_generation,
                        privacy_epoch,
                    },
                    actions: EditorActions {
                        onback: EventHandler::new(move |_| {
                            compact_pane.set(CompactPane::Notes);
                            focus_note_list_panel();
                        }),
                        onchanged: EventHandler::new(move |updated_note: Note| {
                            selected_note.set(Some(updated_note));
                            refresh_editor_aggregate_surfaces_when_privacy_ready(library_state);
                        }),
                        onopentag: EventHandler::new(move |tag: String| {
                            let mut next = library_state.filters.read().clone();
                            next.tags = vec![tag];
                            next.without_tags = false;
                            apply_note_filters(
                                services.read().editor(),
                                next,
                                library_state,
                            );
                            compact_pane.set(CompactPane::Notes);
                            focus_note_list_panel();
                        }),
                        ontagcatalogchanged: EventHandler::new(move |mutation: TagCatalogMutation| {
                            let mut next = library_state.filters.read().clone();
                            let before = next.clone();
                            match mutation {
                                TagCatalogMutation::Rename { from, to } => {
                                    for tag in &mut next.tags {
                                        if tag.eq_ignore_ascii_case(&from) {
                                            *tag = to.clone();
                                        }
                                    }
                                }
                                TagCatalogMutation::Delete { name } => {
                                    next.tags.retain(|tag| !tag.eq_ignore_ascii_case(&name));
                                }
                            }
                            next = next.normalized();
                            if next != before {
                                library_state.filters.set(next);
                                request_note_list_preferences_save(library_state);
                            }
                        }),
                        onrestored: EventHandler::new(move |restored: Note| {
                            let active_query = current_note_list_query(library_state);
                            let restored_matches = services
                                .read()
                                .library()
                                .note_matches_list_query(&restored.id, &active_query);
                            match restored_matches {
                                Ok(true) => selected_note.set(Some(restored)),
                                Ok(false) => selected_note.set(None),
                                Err(error) => {
                                    selected_note.set(None);
                                    status.set(format!(
                                        "The version was restored, but its list membership could not be checked: {error}"
                                    ));
                                }
                            }
                            // Keep the active section, notebook, filters, and FTS query
                            // unchanged. The replacement-page worker repeats the same
                            // canonical SQLite membership check and rejects stale results
                            // by list generation before selecting a fallback note.
                            refresh_editor_aggregate_surfaces_when_privacy_ready(library_state);
                        }),
                        onopennote: EventHandler::new(move |note_id: NoteId| {
                            select_note(
                                services.read().editor(),
                                note_id,
                                library_state,
                                move |_| {
                                    compact_pane.set(CompactPane::Editor);
                                    focus_editor_panel();
                                },
                            );
                        }),
                        onstatus: EventHandler::new(move |message: String| status.set(message)),
                    },
                }
                }
            } else if let Some(transition) = vault_transition.read().clone() {
                VaultTransitionPane {
                    key: "vault-transition-{transition.note_id}-{view_generation}",
                    transition,
                }
            } else {
                div { class: "empty-editor",
                    div { class: "empty-editor-mark", "R" }
                    h2 { "Select a note" }
                    p { "Choose a note from the list or create a new one." }
                }
            }
        }

    }
}

#[derive(Clone, PartialEq)]
pub(super) struct EditorProps {
    pub note: Note,
    pub selected_note: Signal<Option<Note>>,
    pub notebooks: Vec<Notebook>,
    pub services: ApplicationServices,
    pub global_query: String,
    pub find_request: u64,
    pub save_status: String,
    pub save_status_class: &'static str,
    pub right_panel_mode: Signal<Option<RightPanelMode>>,
    pub right_panel_last_mode: Signal<RightPanelMode>,
    pub right_panel_width: Signal<i32>,
    pub editor_session: Signal<Option<EditorSession>>,
    pub tag_editor_request: Signal<Option<NoteId>>,
    pub library_generation: Signal<u64>,
    pub status_note_generation: Signal<u64>,
    pub persistence_generation: Signal<u64>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
}

#[derive(Clone, PartialEq)]
pub(super) struct EditorActions {
    pub onback: EventHandler<()>,
    pub onchanged: EventHandler<Note>,
    pub onopentag: EventHandler<String>,
    pub ontagcatalogchanged: EventHandler<TagCatalogMutation>,
    pub onrestored: EventHandler<Note>,
    pub onopennote: EventHandler<NoteId>,
    pub onstatus: EventHandler<String>,
}

fn statistics_for_editor_update(
    note: &Note,
    change: &ProjectedEditorDocumentUpdate,
) -> Option<TextStatistics> {
    (note.searchable_body() != change.plain_text.as_str())
        .then(|| TextStatistics::from_text(&change.plain_text))
}

fn register_editor_projection_request(
    change: &EditorDocumentUpdate,
    mut latest_revision: Signal<u64>,
    mut request_generation: Signal<u64>,
) -> Option<u64> {
    if change.revision < *latest_revision.peek() {
        return None;
    }
    latest_revision.set(change.revision);
    let request = (*request_generation.peek()).wrapping_add(1);
    request_generation.set(request);
    Some(request)
}

fn editor_projection_request_is_current(
    editor_generation: Signal<u64>,
    latest_revision: Signal<u64>,
    request_generation: Signal<u64>,
    generation: u64,
    revision: u64,
    request: u64,
) -> bool {
    *editor_generation.peek() == generation
        && *latest_revision.peek() == revision
        && *request_generation.peek() == request
}

struct RightPanelRequestContext {
    editor: EditorService,
    draft: Signal<Note>,
    panel_mode: Signal<Option<RightPanelMode>>,
    last_mode: Signal<RightPanelMode>,
    transition_busy: Signal<bool>,
    editor_popover: Signal<Option<EditorPopover>>,
    onstatus: EventHandler<String>,
}

fn request_right_panel_mode(requested: RightPanelMode, context: RightPanelRequestContext) {
    let RightPanelRequestContext {
        editor,
        draft,
        mut panel_mode,
        mut last_mode,
        mut transition_busy,
        mut editor_popover,
        onstatus,
    } = context;
    if *panel_mode.peek() == Some(requested) || *transition_busy.peek() {
        return;
    }
    editor_popover.set(None);
    if requested != RightPanelMode::Revisions {
        last_mode.set(requested);
        panel_mode.set(Some(requested));
        return;
    }

    let pending_flush = begin_draft_flush_scoped(
        &editor,
        draft.peek().clone(),
        SynchronizationMode::Checkpoint,
        NoteChangeScope::None,
    );
    transition_busy.set(true);
    spawn(async move {
        let result = complete_draft_flush(pending_flush).await;
        transition_busy.set(false);
        match result {
            Ok(result) => {
                last_mode.set(RightPanelMode::Revisions);
                panel_mode.set(Some(RightPanelMode::Revisions));
                onstatus.call(result.completion_message("Saved locally"));
            }
            Err(error) => onstatus.call(format!("Save failed: {error}")),
        }
    });
}

#[component]
pub(super) fn NoteEditor(view: EditorProps, actions: EditorActions) -> Element {
    let EditorProps {
        note,
        selected_note,
        notebooks,
        services,
        global_query,
        find_request,
        save_status,
        save_status_class,
        mut right_panel_mode,
        mut right_panel_last_mode,
        right_panel_width,
        mut editor_session,
        mut tag_editor_request,
        library_generation,
        status_note_generation,
        persistence_generation,
        privacy_epoch,
    } = view;
    let EditorActions {
        onback,
        onchanged,
        onopentag,
        ontagcatalogchanged,
        onrestored,
        onopennote,
        onstatus: outer_onstatus,
    } = actions;
    let background_operations = use_context::<Signal<ApplicationExecutionStatus>>();
    let background_coordinator = use_context::<ApplicationExecutor>();
    let attachment_service = services.attachments();
    let editor_service = services.editor();
    let editor_preferences = use_context::<Signal<EditorPreferences>>();
    let workflow_service = services.editor_workflows(background_coordinator.clone());
    let desktop = dioxus_desktop::use_window();
    let protected_status_publisher = use_context::<super::toast::ProtectedStatusPublisher>();
    let protected_status_scope = note.protection.is_protected().then(|| {
        privacy_epoch.note_publication_scope(
            services.privacy(),
            note.id.clone(),
            status_note_generation,
        )
    });
    let onstatus = EventHandler::new(move |message: String| {
        let Some(scope) = protected_status_scope.as_ref() else {
            outer_onstatus.call(message);
            return;
        };
        let Some(token) = scope.capture() else {
            return;
        };
        let _ = protected_status_publisher.publish(token, message);
    });
    let initial_attachment_service = attachment_service.clone();
    let attachment_note_id = note.id.clone();
    let initial_attachment_status = onstatus;
    let mut attachments =
        use_signal(
            move || match initial_attachment_service.list(&attachment_note_id) {
                Ok(attachments) => attachments,
                Err(error) => {
                    initial_attachment_status.call(format!("Could not load attachments: {error}"));
                    Vec::new()
                }
            },
        );
    // Note-link targets are an aggregate registry: a target may expose another
    // note's title/metadata. Start empty and populate only under multi-note
    // publication authority below.
    let mut note_link_targets = use_signal(Vec::<NoteLinkTarget>::new);
    let multi_note_privacy = use_memo(move || privacy_epoch.observe_multi_note());
    let mut note_link_picker = use_signal(|| None::<NoteLinkPickerRequest>);

    let title_flush_busy = use_signal(|| false);
    let mut title_focused = use_signal(|| false);
    let initial_text_statistics = TextStatistics::from_text(note.searchable_body());
    let mut text_statistics = use_signal(move || initial_text_statistics);
    let mut editor_document_revision = use_signal(|| 0_u64);
    let mut draft = use_signal(|| note.clone());
    let mut tags_text = use_signal(|| note.tags_text());
    let mut editor_generation = use_signal(next_editor_generation);
    let mut latest_editor_revision = use_signal(|| 0_u64);
    let mut editor_projection_request_generation = use_signal(|| 0_u64);
    let mut editor_mount_epoch = use_signal(|| 0_u64);
    let session_note_id = note.id.clone();
    let session_note_id_for_drop = session_note_id.clone();
    let _editor_session_effect = use_effect(move || {
        editor_session.set(Some(EditorSession {
            note_id: session_note_id.clone(),
            generation: *editor_generation.read(),
        }));
    });
    use_drop(move || {
        // E3: dispose the browser editor immediately when this note view is
        // unmounted (including when an unlocked encrypted note is re-locked),
        // so the previous document is not deliberately retained in the live
        // WebView editor instance.
        let _ = document::eval(&live_editor_dispose_script(false));
        if editor_session
            .read()
            .as_ref()
            .is_some_and(|session| session.note_id == session_note_id_for_drop)
        {
            editor_session.set(None);
        }
    });
    let mut restoring_revision = use_signal(|| false);
    let editing_available = background_operations.read().editing_available;
    let note_read_only = draft.read().trashed || draft.read().protection.is_locked();
    let editor_editable = editing_available && !*restoring_revision.read() && !note_read_only;
    let _editor_editability_effect = use_effect(move || {
        let editable = background_operations.read().editing_available
            && !*restoring_revision.read()
            && !draft.read().trashed
            && !draft.read().protection.is_locked();
        let script = live_editor_set_editable_script(editable);
        spawn(async move {
            let _ = document::eval(&script).join::<bool>().await;
        });
    });
    let attachment_registry_service = attachment_service.clone();
    let attachment_registry_privacy = services.privacy();
    let attachment_registry_note_id = note.id.clone();
    let _attachment_registry_effect = use_effect(move || {
        let current_attachments = attachments.read().clone();
        let Some(publication_token) = privacy_epoch.capture_note_publication(
            &attachment_registry_privacy,
            &attachment_registry_note_id,
            editor_generation,
        ) else {
            return;
        };
        let script = live_editor_set_attachment_registry_script(
            &attachment_registry_service,
            &current_attachments,
        );
        let clear_service = attachment_registry_service.clone();
        spawn(async move {
            let Some(_publication_lease) = publication_token.begin_publication() else {
                return;
            };
            let _ = document::eval(&script).join::<bool>().await;
            drop(_publication_lease);
            if !publication_token.is_publishable() {
                let clear_script = live_editor_set_attachment_registry_script(&clear_service, &[]);
                let _ = document::eval(&clear_script).join::<bool>().await;
            }
        });
    });
    let mut target_refresh_generation = use_signal(|| 0_u64);
    let target_refresh_service = editor_service.clone();
    let target_refresh_note_id = note.id.clone();
    let target_refresh_privacy = services.privacy();
    let _note_link_target_refresh_effect = use_effect(move || {
        let _multi_note_privacy = *multi_note_privacy.read();
        let _library_generation = *library_generation.read();
        let Some(publication_token) = privacy_epoch.capture_multi_note_publication_with_lifecycle(
            &target_refresh_privacy,
            library_generation,
            editor_generation,
        ) else {
            note_link_targets.set(Vec::new());
            return;
        };
        note_link_targets.set(Vec::new());
        let request_generation = (*target_refresh_generation.peek()).wrapping_add(1);
        target_refresh_generation.set(request_generation);
        let service = target_refresh_service.clone();
        let note_id = target_refresh_note_id.clone();
        spawn(async move {
            let result = ApplicationExecutor::read("note-link targets", move || {
                service.note_link_targets(&note_id)
            })
            .await;
            if *target_refresh_generation.peek() != request_generation {
                return;
            }
            let Some(_publication_lease) = publication_token.begin_publication() else {
                return;
            };
            match result {
                Ok(targets) => {
                    let mut merged = note_link_targets.peek().clone();
                    for target in targets {
                        if let Some(existing) = merged
                            .iter_mut()
                            .find(|existing| existing.note_id == target.note_id)
                        {
                            *existing = target;
                        } else {
                            merged.push(target);
                        }
                    }
                    note_link_targets.set(merged);
                }
                Err(error) => onstatus.call(format!("Could not refresh note links: {error}")),
            }
        });
    });
    let note_link_registry_privacy = services.privacy();
    let _note_link_registry_effect = use_effect(move || {
        let current_targets = note_link_targets.read().clone();
        let Some(publication_token) = privacy_epoch.capture_multi_note_publication_with_lifecycle(
            &note_link_registry_privacy,
            library_generation,
            editor_generation,
        ) else {
            let clear_script = live_editor_set_note_link_registry_script(&[]);
            spawn(async move {
                let _ = document::eval(&clear_script).join::<bool>().await;
            });
            return;
        };
        let script = live_editor_set_note_link_registry_script(&current_targets);
        spawn(async move {
            let Some(_publication_lease) = publication_token.begin_publication() else {
                return;
            };
            let _ = document::eval(&script).join::<bool>().await;
            drop(_publication_lease);
            if !publication_token.is_publishable() {
                let clear_script = live_editor_set_note_link_registry_script(&[]);
                let _ = document::eval(&clear_script).join::<bool>().await;
            }
        });
    });
    let revision_panel_operation = use_signal(|| None::<ApplicationOperation>);
    let right_panel_transition_busy = use_signal(|| false);
    let mut editor_popover = use_signal(|| None::<EditorPopover>);
    let _tag_editor_request_effect = use_effect(move || {
        let target_note_id = tag_editor_request.read().clone();
        let Some(target_note_id) = target_note_id else {
            return;
        };
        if draft.read().id.as_str() != target_note_id.as_str() {
            return;
        }
        if draft.read().trashed
            || draft.read().protection.is_locked()
            || !background_operations.read().editing_available
            || *restoring_revision.read()
        {
            return;
        }
        editor_popover.set(Some(EditorPopover::Tags));
        tag_editor_request.set(None);
    });
    let link_url = use_signal(String::new);
    let link_label = use_signal(|| "link text".to_string());
    let link_error = use_signal(|| None::<String>);
    let editor_note_privacy = super::privacy::use_note_privacy(privacy_epoch, note.id.clone());
    let editor_privacy_note_id = note.id.clone();
    let mut privacy_draft = draft;
    let mut privacy_tags_text = tags_text;
    let mut privacy_attachments = attachments;
    let mut privacy_note_link_targets = note_link_targets;
    let mut privacy_note_link_picker = note_link_picker;
    let mut privacy_text_statistics = text_statistics;
    let mut privacy_editor_popover = editor_popover;
    let mut privacy_right_panel_mode = right_panel_mode;
    let mut privacy_link_url = link_url;
    let mut privacy_link_label = link_label;
    let mut privacy_link_error = link_error;
    let _note_privacy_redaction_effect = use_effect(move || {
        let (_, _, revocation_active) = *editor_note_privacy.read();
        if !revocation_active || privacy_draft.peek().id != editor_privacy_note_id {
            return;
        }
        let current = privacy_draft.peek().clone();
        let redacted = redact_note_for_privacy_transition(&current);
        privacy_draft.set(redacted);
        privacy_tags_text.set(String::new());
        privacy_attachments.set(Vec::new());
        privacy_note_link_targets.set(Vec::new());
        privacy_note_link_picker.set(None);
        privacy_text_statistics.set(TextStatistics::default());
        privacy_editor_popover.set(None);
        privacy_right_panel_mode.set(None);
        privacy_link_url.set(String::new());
        privacy_link_label.set(String::new());
        privacy_link_error.set(None);
        let dispose_script = live_editor_dispose_script(false);
        spawn(async move {
            let _ = document::eval(&dispose_script).join::<bool>().await;
        });
    });
    let initial_find_query = ParsedSearch::parse(&global_query)
        .first_highlight_term(SearchField::Body)
        .unwrap_or_default();
    let initial_find_query_for_signal = initial_find_query.clone();
    let mut find_query = use_signal(move || initial_find_query_for_signal);
    let mut last_global_find_query = use_signal(move || initial_find_query);
    let mut find_index = use_signal(|| 0_usize);
    let mut find_count = use_signal(|| 0_usize);
    let mut find_request_generation = use_signal(|| 0_u64);
    let mut last_scheduled_find_query = use_signal(String::new);
    let mut last_find_request = use_signal(move || find_request);
    let _find_request_effect = use_effect(move || {
        if find_request != *last_find_request.read() {
            last_find_request.set(find_request);
            if !*right_panel_transition_busy.read()
                && revision_panel_operation.read().is_none()
                && !*restoring_revision.read()
            {
                editor_popover.set(None);
                right_panel_last_mode.set(RightPanelMode::Find);
                right_panel_mode.set(Some(RightPanelMode::Find));
            }
        }
    });
    let _find_visibility_effect = use_effect(move || {
        if *right_panel_mode.read() == Some(RightPanelMode::Find) {
            focus_note_find_input();
        }
    });
    let _global_find_sync = use_effect(move || {
        let next = ParsedSearch::parse(&global_query)
            .first_highlight_term(SearchField::Body)
            .unwrap_or_default();
        if next != *last_global_find_query.read() {
            last_global_find_query.set(next.clone());
            find_query.set(next);
            find_index.set(0);
        }
    });
    let _find_effect = use_effect(move || {
        let _document_revision = *editor_document_revision.read();
        let query = if *right_panel_mode.read() == Some(RightPanelMode::Find) {
            find_query.read().clone()
        } else {
            String::new()
        };
        let query_changed = query != *last_scheduled_find_query.peek();
        if query_changed {
            last_scheduled_find_query.set(query.clone());
            find_count.set(0);
        }
        let request_generation = (*find_request_generation.peek()).wrapping_add(1);
        find_request_generation.set(request_generation);
        let debounce_query_change = query_changed && !query.is_empty();
        schedule_note_find(
            query,
            *find_index.read(),
            find_count,
            find_index,
            request_generation,
            find_request_generation,
            debounce_query_change,
        );
    });
    let current_text_statistics = *text_statistics.read();
    let footer_word_count = current_text_statistics.words;
    let current_panel_mode = *right_panel_mode.read();
    let outline_editor_state = if current_panel_mode == Some(RightPanelMode::Outline) {
        let _outline_revision = *editor_document_revision.read();
        Some(draft.peek().document.editor_state.clone())
    } else {
        None
    };
    let right_panel_locked = *restoring_revision.read()
        || note_link_picker.read().is_some()
        || revision_panel_operation.read().is_some()
        || *right_panel_transition_busy.read();

    let note_title_style = if draft.read().title.trim().is_empty() {
        "font-weight:400;".to_string()
    } else {
        title_style_css(&draft.read().title_style)
    };
    let editor_key = format!("{}:{}", draft.read().id, *editor_mount_epoch.read());

    rsx! {
        div {
            class: "editor-shell",
            onkeydown: move |event| {
                if !background_operations.read().editing_available {
                    return;
                }
                if event.key() != Key::Escape {
                    return;
                }
                if editor_popover.read().is_some() {
                    event.stop_propagation();
                    editor_popover.set(None);
                } else if right_panel_mode.read().is_some()
                    && !*restoring_revision.read()
                    && revision_panel_operation.read().is_none()
                    && !*right_panel_transition_busy.read()
                {
                    event.stop_propagation();
                    right_panel_mode.set(None);
                    focus_live_editor();
                }
            },
            div { class: "editor-workspace",
            if draft.read().protection.is_protected() {
                div { class: "encrypted-note-banner", role: "status",
                    span { aria_hidden: "true", "🔒" }
                    if draft.read().protection.is_locked() {
                        span { "Encrypted note — locked." }
                    } else {
                        span { "Encrypted note — unlocked for this session." }
                    }
                }
            }
            EditorToolbar {
                view: EditorToolbarView {
                    notebooks: notebooks.clone(),
                    services: services.clone(),
                    selected_note,
                    editor_session,
                    privacy_epoch,
                    library_generation,
                    note_generation: editor_generation,
                    draft,
                    tags_text,
                    right_panel_mode,
                    right_panel_last_mode,
                    right_panel_locked,
                    editing_available,
                    editor_popover,
                    link_url,
                    link_label,
                    link_error,
                    editor_preferences,
                    title_focused,
                },
                actions: EditorToolbarActions {
                    onback,
                    onpanelrequest: {
                        let editor = editor_service.clone();
                        EventHandler::new(move |requested| {
                            request_right_panel_mode(
                                requested,
                                RightPanelRequestContext {
                                    editor: editor.clone(),
                                    draft,
                                    panel_mode: right_panel_mode,
                                    last_mode: right_panel_last_mode,
                                    transition_busy: right_panel_transition_busy,
                                    editor_popover,
                                    onstatus,
                                },
                            );
                        })
                    },
                    onchanged,
                    onopentag,
                    ontagcatalogchanged,
                    ontitletypography: {
                        let editor = editor_service.clone();
                        EventHandler::new(move |command: TitleTypographyCommand| {
                            if !background_operations.read().editing_available
                                || *restoring_revision.read()
                                || draft.read().trashed
                                || draft.read().protection.is_locked()
                            {
                                return;
                            }
                            let mut updated = draft.read().clone();
                            let previous = updated.title_style.clone();
                            apply_title_typography(&mut updated.title_style, command);
                            if updated.title_style == previous {
                                return;
                            }
                            updated.updated_at = unix_timestamp();
                            draft.set(updated.clone());
                            schedule_draft_scoped(
                                &editor,
                                updated,
                                NoteChangeScope::Title,
                                onstatus,
                            );
                        })
                    },
                    onstatus,
                },
            }

            h1 { class: "note-print-title", "{draft.read().display_title()}" }

            input {
                id: "note-title-input",
                class: if draft.read().title.trim().is_empty() {
                    "note-title-input placeholder"
                } else {
                    "note-title-input"
                },
                value: "{draft.read().title}",
                style: "{note_title_style}",
                placeholder: "Untitled note",
                disabled: !editing_available
                    || *restoring_revision.read()
                    || draft.read().trashed
                    || draft.read().protection.is_locked()
                    || *title_flush_busy.read(),
                onfocus: move |_| title_focused.set(true),
                oninput: {
                    let editor = editor_service.clone();
                    move |event| {
                        if !background_operations.read().editing_available
                            || *restoring_revision.read()
                            || draft.read().trashed
                            || draft.read().protection.is_locked()
                        {
                            return;
                        }
                        let mut updated = draft.read().clone();
                        updated.title = event.value();
                        updated.updated_at = unix_timestamp();
                        draft.set(updated.clone());
                        schedule_draft_scoped(&editor, updated, NoteChangeScope::Title, onstatus);
                    }
                },
                onkeydown: move |event: KeyboardEvent| {
                    if event.is_composing() {
                        return;
                    }
                    if matches!(event.key(), Key::Enter | Key::ArrowDown) {
                        event.prevent_default();
                        event.stop_propagation();
                        focus_live_editor();
                    }
                },
                onblur: {
                    let editor = editor_service.clone();
                    move |_| {
                        title_focused.set(false);
                        if !background_operations.read().editing_available
                            || *restoring_revision.read()
                            || draft.read().trashed
                            || draft.read().protection.is_locked()
                            || *title_flush_busy.read()
                        {
                            return;
                        }
                        let updated = draft.read().clone();
                        let pending_flush = begin_draft_flush_scoped(
                            &editor,
                            updated,
                            SynchronizationMode::Persist,
                            NoteChangeScope::Title,
                        );
                        let mut task_busy = title_flush_busy;
                        task_busy.set(true);
                        spawn(async move {
                            let result = complete_draft_flush(pending_flush).await;
                            task_busy.set(false);
                            match result {
                                Ok(result) => {
                                    onstatus.call(result.completion_message("Saved locally"));
                                }
                                Err(error) => onstatus.call(format!("Save failed: {error}")),
                            }
                        });
                    }
                }
            }

            if let Some(editor_publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
            {
                NoteDocumentEditor {
                    key: "{editor_key}",
                    view: LiveEditorView {
                        note_id: draft.read().id.clone(),
                        document: draft.read().document.clone(),
                        generation: *editor_generation.read(),
                        editable: editor_editable,
                        attachment_service: attachment_service.clone(),
                        attachments: attachments.read().clone(),
                        note_link_targets: note_link_targets.read().clone(),
                        editor_preferences: *editor_preferences.read(),
                        publication_token: editor_publication_token.clone(),
                    },
                actions: LiveEditorActions {
                    onchange: {
                        let editor = editor_service.clone();
                        let publication_token = editor_publication_token.clone();
                        EventHandler::new(move |change: EditorDocumentUpdate| {
                            if !publication_token.is_publishable()
                                || !background_operations.read().editing_available
                                || *restoring_revision.read()
                                || change.generation != *editor_generation.read()
                            {
                                return;
                            }
                            let Some(request) = register_editor_projection_request(
                                &change,
                                latest_editor_revision,
                                editor_projection_request_generation,
                            ) else {
                                return;
                            };
                            let generation = change.generation;
                            let revision = change.revision;
                            let editor = editor.clone();
                            let publication_token = publication_token.clone();
                            spawn(async move {
                                let projected = match project_editor_update_async(change).await {
                                    Ok(projected) => projected,
                                    Err(error) => {
                                        if editor_projection_request_is_current(
                                            editor_generation,
                                            latest_editor_revision,
                                            editor_projection_request_generation,
                                            generation,
                                            revision,
                                            request,
                                        ) && let Some(_publication_lease) = publication_token.begin_publication()
                                        {
                                            onstatus.call(format!(
                                                "Could not process the editor update: {error}"
                                            ));
                                        }
                                        return;
                                    }
                                };
                                if !editor_projection_request_is_current(
                                    editor_generation,
                                    latest_editor_revision,
                                    editor_projection_request_generation,
                                    generation,
                                    revision,
                                    request,
                                ) || *restoring_revision.read() {
                                    return;
                                }
                                let Some(_publication_lease) = publication_token.begin_publication() else {
                                    return;
                                };
                                let mut updated = draft.read().clone();
                                let next_statistics =
                                    statistics_for_editor_update(&updated, &projected);
                                if !apply_editor_update(&mut updated, projected) {
                                    return;
                                }
                                updated.updated_at = unix_timestamp();
                                draft.set(updated.clone());
                                if let Some(statistics) = next_statistics {
                                    text_statistics.set(statistics);
                                }
                                let next_revision =
                                    (*editor_document_revision.read()).wrapping_add(1);
                                editor_document_revision.set(next_revision);
                                schedule_draft_scoped(
                                    &editor,
                                    updated,
                                    NoteChangeScope::Document,
                                    onstatus,
                                );
                            });
                        })
                    },
                    oncommit: {
                        let editor = editor_service.clone();
                        let publication_token = editor_publication_token.clone();
                        EventHandler::new(move |change: EditorDocumentUpdate| {
                            if !publication_token.is_publishable()
                                || !background_operations.read().editing_available
                                || *restoring_revision.read()
                                || change.generation != *editor_generation.read()
                            {
                                return;
                            }
                            let Some(request) = register_editor_projection_request(
                                &change,
                                latest_editor_revision,
                                editor_projection_request_generation,
                            ) else {
                                return;
                            };
                            let generation = change.generation;
                            let revision = change.revision;
                            let editor = editor.clone();
                            let publication_token = publication_token.clone();
                            spawn(async move {
                                let mut updated = draft.read().clone();
                                if updated.editor_state() != change.editor_state.as_str() {
                                    let projected = match project_editor_update_async(change).await {
                                        Ok(projected) => projected,
                                        Err(error) => {
                                            if editor_projection_request_is_current(
                                                editor_generation,
                                                latest_editor_revision,
                                                editor_projection_request_generation,
                                                generation,
                                                revision,
                                                request,
                                            ) && let Some(_publication_lease) = publication_token.begin_publication()
                                            {
                                                onstatus.call(format!(
                                                    "Save failed while processing the editor state: {error}"
                                                ));
                                            }
                                            return;
                                        }
                                    };
                                    if !editor_projection_request_is_current(
                                        editor_generation,
                                        latest_editor_revision,
                                        editor_projection_request_generation,
                                        generation,
                                        revision,
                                        request,
                                    ) || *restoring_revision.read() {
                                        return;
                                    }
                                    let Some(_publication_lease) = publication_token.begin_publication() else {
                                        return;
                                    };
                                    let next_statistics =
                                        statistics_for_editor_update(&updated, &projected);
                                    if apply_editor_update(&mut updated, projected) {
                                        updated.updated_at = unix_timestamp();
                                        draft.set(updated.clone());
                                        if let Some(statistics) = next_statistics {
                                            text_statistics.set(statistics);
                                        }
                                        let next_revision =
                                            (*editor_document_revision.read()).wrapping_add(1);
                                        editor_document_revision.set(next_revision);
                                    }
                                } else if !publication_token.is_publishable()
                                    || !editor_projection_request_is_current(
                                        editor_generation,
                                        latest_editor_revision,
                                        editor_projection_request_generation,
                                        generation,
                                        revision,
                                        request,
                                    )
                                    || *restoring_revision.read()
                                {
                                    return;
                                }
                                let pending_flush = begin_draft_flush_scoped(
                                    &editor,
                                    updated,
                                    SynchronizationMode::Persist,
                                    NoteChangeScope::Document,
                                );
                                match complete_draft_flush(pending_flush).await {
                                    Ok(result) if publication_token.is_publishable() => {
                                        onstatus.call(result.completion_message("Saved locally"));
                                    }
                                    Ok(_) => {}
                                    Err(error) if publication_token.is_publishable() => {
                                        onstatus.call(format!("Save failed: {error}"));
                                    }
                                    Err(_) => {}
                                }
                            });
                        })
                    },
                    onattachmentcommand: {
                        let attachment_service = attachment_service.clone();
                        let workflow_service = workflow_service.clone();
                        let pdf_window = desktop.clone();
                        let publication_token = editor_publication_token.clone();
                        let services = services.clone();
                        EventHandler::new(move |command: EditorAttachmentCommand| {
                            if !publication_token.is_publishable() {
                                return;
                            }
                            match command {
                            EditorAttachmentCommand::Status { message } => {
                                onstatus.call(message);
                            }
                            EditorAttachmentCommand::ToggleWindowFullscreen => {
                                let fullscreen = if pdf_window.window.fullscreen().is_some() {
                                    None
                                } else {
                                    Some(dioxus_desktop::tao::window::Fullscreen::Borderless(None))
                                };
                                pdf_window.window.set_fullscreen(fullscreen);
                            }
                            EditorAttachmentCommand::PickAttachments { request_id, picker, editor_update } => {
                                if editor_update.generation != *editor_generation.read() {
                                    let script = live_editor_fail_attachment_picker_script(&request_id);
                                    spawn(async move {
                                        let _ = document::eval(&script).join::<bool>().await;
                                    });
                                    return;
                                }
                                if draft.read().trashed || draft.read().protection.is_locked() || !background_operations.read().editing_available {
                                    let script = live_editor_fail_attachment_picker_script(&request_id);
                                    spawn(async move {
                                        let _ = document::eval(&script).join::<bool>().await;
                                    });
                                    onstatus.call("Attachments cannot be inserted while this note is read-only".to_string());
                                    return;
                                }
                                if workflow_service.attachment_operation_active() {
                                    let script = live_editor_fail_attachment_picker_script(&request_id);
                                    spawn(async move {
                                        let _ = document::eval(&script).join::<bool>().await;
                                    });
                                    onstatus.call("Another attachment import is already in progress".to_string());
                                    return;
                                }
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                let completion_service = attachment_service.clone();
                                spawn(async move {
                                    let presentation = AttachmentWorkflowPresentation::new(
                                        completion_service,
                                        draft,
                                        attachments,
                                        onchanged,
                                        onstatus,
                                    );
                                    run_attachment_import(
                                        workflows,
                                        request_id,
                                        picker,
                                        editor_update,
                                        publication_token,
                                        presentation,
                                    )
                                    .await;
                                });
                            }
                            EditorAttachmentCommand::ProbeMediaCompatibility { attachment_id, profile } => {
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                spawn(async move {
                                    run_media_compatibility(
                                        workflows,
                                        attachment_id,
                                        profile,
                                        MediaCompatibilityWorkflowAction::Probe,
                                        publication_token,
                                    )
                                    .await;
                                });
                            }
                            EditorAttachmentCommand::PrepareMediaCompatibility { attachment_id, profile } => {
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                spawn(async move {
                                    run_media_compatibility(
                                        workflows,
                                        attachment_id,
                                        profile,
                                        MediaCompatibilityWorkflowAction::Prepare,
                                        publication_token,
                                    )
                                    .await;
                                });
                            }
                            EditorAttachmentCommand::InvalidateMediaCompatibility { attachment_id, profile } => {
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                spawn(async move {
                                    run_media_compatibility(
                                        workflows,
                                        attachment_id,
                                        profile,
                                        MediaCompatibilityWorkflowAction::Invalidate,
                                        publication_token,
                                    )
                                    .await;
                                });
                            }
                            EditorAttachmentCommand::FileAction { attachment_id, action } => {
                                let service = attachment_service.clone();
                                let note_id = draft.read().id.clone();
                                let command = match action {
                                    EditorAttachmentFileAction::Open => AttachmentFileCommand::Open,
                                    EditorAttachmentFileAction::SaveAs => AttachmentFileCommand::SaveAs,
                                    EditorAttachmentFileAction::Reveal => AttachmentFileCommand::Reveal,
                                };
                                spawn(async move {
                                    match service
                                        .execute_file_command_by_id(note_id, attachment_id, command)
                                    .await
                                    {
                                        Ok(AttachmentFileCommandOutcome::Completed) => {
                                            onstatus.call(command.success_message().to_string())
                                        }
                                        Ok(AttachmentFileCommandOutcome::Cancelled) => {}
                                        Err(error) => onstatus.call(format!(
                                            "Could not {} attachment: {error}",
                                            command.failure_verb()
                                        )),
                                    }
                                });
                            }
                            EditorAttachmentCommand::OpenPendingAttachment { ingest_id } => {
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                spawn(async move {
                                    run_pending_attachment_open(
                                        workflows,
                                        ingest_id,
                                        publication_token,
                                        onstatus,
                                    )
                                    .await;
                                });
                            }
                            EditorAttachmentCommand::RecoverPendingAttachments { generation } => {
                                if generation != *editor_generation.read() {
                                    return;
                                }
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                let recovery_service = attachment_service.clone();
                                let note_id = draft.read().id.clone();
                                spawn(async move {
                                    let presentation = AttachmentWorkflowPresentation::new(
                                        recovery_service,
                                        draft,
                                        attachments,
                                        onchanged,
                                        onstatus,
                                    );
                                    run_pending_attachment_recovery(
                                        workflows,
                                        note_id,
                                        generation,
                                        editor_generation,
                                        publication_token,
                                        presentation,
                                    )
                                    .await;
                                });
                            }
                            EditorAttachmentCommand::SavePdfExport {
                                request_id,
                                attachment_id,
                                export_token,
                            } => {
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                let note_id = draft.read().id.clone();
                                spawn(async move {
                                    run_pdf_export(
                                        workflows,
                                        note_id,
                                        request_id,
                                        attachment_id,
                                        export_token,
                                        publication_token,
                                        onstatus,
                                    )
                                    .await;
                                });
                            }
                            EditorAttachmentCommand::CommitImageEdit(payload) => {
                                let request_id = payload.request_id.clone();
                                let stage_token = payload.stage_token.clone();
                                if draft.read().trashed || draft.read().protection.is_locked() || !background_operations.read().editing_available {
                                    reject_image_edit(
                                        &workflow_service,
                                        &request_id,
                                        &stage_token,
                                        "Image editing is unavailable while this note is read-only",
                                        onstatus,
                                    );
                                    return;
                                }
                                if workflow_service.attachment_operation_active() {
                                    reject_image_edit(
                                        &workflow_service,
                                        &request_id,
                                        &stage_token,
                                        "Another attachment operation is already in progress",
                                        onstatus,
                                    );
                                    return;
                                }
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                let completion_service = attachment_service.clone();
                                let note_id = draft.read().id.clone();
                                spawn(async move {
                                    run_image_edit(
                                        workflows,
                                        completion_service,
                                        note_id,
                                        payload,
                                        publication_token,
                                        attachments,
                                        onstatus,
                                    )
                                    .await;
                                });
                            }
                            EditorAttachmentCommand::ImportImages { request_id, images } => {
                                if draft.read().trashed || draft.read().protection.is_locked() || !background_operations.read().editing_available {
                                    reject_image_import(
                                        &workflow_service,
                                        &request_id,
                                        &images,
                                        "Images cannot be inserted while this note is read-only",
                                        onstatus,
                                    );
                                    return;
                                }
                                if workflow_service.attachment_operation_active() {
                                    reject_image_import(
                                        &workflow_service,
                                        &request_id,
                                        &images,
                                        "Another attachment import is already in progress",
                                        onstatus,
                                    );
                                    return;
                                }
                                let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                                else {
                                    return;
                                };
                                let workflows = workflow_service.clone();
                                let completion_service = attachment_service.clone();
                                let note_id = draft.read().id.clone();
                                spawn(async move {
                                    run_image_import(ImageImportFlow {
                                        workflows,
                                        attachment_service: completion_service,
                                        note_id,
                                        request_id,
                                        images,
                                        publication_token,
                                        attachments,
                                        onstatus,
                                    })
                                    .await;
                                });
                            }
                        }
                        })
                    },
                    onnotelinkcommand: {
                        let publication_token = editor_publication_token.clone();
                        EventHandler::new(move |command: EditorNoteLinkCommand| {
                            if !publication_token.is_publishable() {
                                return;
                            }
                            match command {
                            EditorNoteLinkCommand::PickNoteLink { request_id, initial_query } => {
                                if draft.read().trashed
                                    || draft.read().protection.is_locked()
                                    || !background_operations.read().editing_available
                                    || *restoring_revision.read()
                                {
                                    let script = live_editor_fail_note_link_picker_script(&request_id);
                                    spawn(async move {
                                        let _ = document::eval(&script).join::<bool>().await;
                                    });
                                    onstatus.call(
                                        "Note links cannot be inserted while this note is read-only".to_string(),
                                    );
                                    return;
                                }
                                editor_popover.set(None);
                                note_link_picker.set(Some(NoteLinkPickerRequest {
                                    request_id,
                                    initial_query,
                                }));
                            }
                            EditorNoteLinkCommand::OpenNote { note_id } => {
                                note_link_picker.set(None);
                                onopennote.call(note_id);
                            }
                        }
                        })
                    },
                    onstatus,
                },
            }

            footer { class: "editor-footer",
                span { class: "editor-footer-word-count", "{footer_word_count} words" }
                span {
                    class: "editor-footer-save-status {save_status_class}",
                    role: "status",
                    aria_live: "polite",
                    span { class: "status-dot", aria_hidden: "true" }
                    span { class: "status-message", "{save_status}" }
                }
            }
            }

            if let Some(active_mode) = current_panel_mode {
                RightPanelShell {
                    mode: active_mode,
                    panel_mode: right_panel_mode,
                    width: right_panel_width,
                    locked: right_panel_locked,
                    onmodechange: {
                        let editor = editor_service.clone();
                        move |requested| {
                            request_right_panel_mode(
                                requested,
                                RightPanelRequestContext {
                                    editor: editor.clone(),
                                    draft,
                                    panel_mode: right_panel_mode,
                                    last_mode: right_panel_last_mode,
                                    transition_busy: right_panel_transition_busy,
                                    editor_popover,
                                    onstatus,
                                },
                            );
                        }
                    },
                    onwidthcommit: {
                        let settings = services.settings();
                        move |next_width| {
                            persist_right_panel_width(settings.clone(), next_width, onstatus);
                        }
                    },
                    if active_mode == RightPanelMode::Find {
                        FindPanel {
                            view: FindPanelView {
                                find_query,
                                find_index,
                                find_count,
                            },
                        }
                    } else if active_mode == RightPanelMode::Outline {
                        OutlinePanel {
                            editor_state: outline_editor_state
                                .clone()
                                .unwrap_or_else(|| draft.peek().document.editor_state.clone()),
                        }
                    } else if active_mode == RightPanelMode::Backlinks {
                        BacklinksPanel {
                            view: BacklinksPanelView {
                                notes: services.notes(),
                                note_id: draft.peek().id.clone(),
                                document_revision: editor_document_revision,
                                privacy_epoch,
                                library_generation,
                                note_generation: editor_generation,
                            },
                            actions: BacklinksPanelActions {
                                onopen: onopennote,
                                onstatus,
                            },
                        }
                    } else if active_mode == RightPanelMode::Hierarchy {
                        NoteHierarchyPanel {
                            view: NoteHierarchyPanelView {
                                notes: services.notes(),
                                library: services.library(),
                                note_id: draft.peek().id.clone(),
                                read_only: draft.peek().trashed || draft.peek().protection.is_locked(),
                                library_generation,
                                note_generation: editor_generation,
                                privacy_epoch,
                            },
                            actions: NoteHierarchyPanelActions {
                                onopen: onopennote,
                                onstatus,
                            },
                        }
                    } else if active_mode == RightPanelMode::Properties {
                        NotePropertiesPanel {
                            view: NotePropertiesPanelView {
                                notes: services.notes(),
                                note: draft.peek().clone(),
                                notebooks: notebooks.clone(),
                                persistence_generation,
                                privacy_epoch,
                                note_generation: editor_generation,
                            },
                            actions: NotePropertiesPanelActions {
                                onopen: onopennote,
                                onstatus,
                            },
                        }
                    } else if active_mode == RightPanelMode::Sources {
                        SourcesPanel {
                            view: SourcesPanelView {
                                notes: services.notes(),
                                note_id: draft.peek().id.clone(),
                                read_only: draft.peek().trashed || draft.peek().protection.is_locked(),
                                library_generation,
                                note_generation: editor_generation,
                                privacy_epoch,
                            },
                            actions: SourcesPanelActions { onstatus },
                        }
                    } else if active_mode == RightPanelMode::Attachments {
                        AttachmentsPanel {
                            view: AttachmentsPanelView {
                                attachment_service: attachment_service.clone(),
                                draft,
                                attachments,
                                attachment_busy: workflow_service.attachment_operation_active(),
                                editing_available,
                                privacy_epoch,
                                privacy: services.privacy(),
                                note_generation: editor_generation,
                            },
                            actions: AttachmentsPanelActions { onstatus },
                        }
                    } else {
                        RevisionHistoryPanel {
                            key: "{draft.read().id}",
                            view: RevisionHistoryPanelView {
                                note_id: draft.read().id.clone(),
                                notebooks: notebooks.clone(),
                                notes: services.notes(),
                                restoring: *restoring_revision.read(),
                                // Unlocked protected notes use the same revision UX; storage
                                // routes restore through immutable encrypted revision objects.
                                read_only: draft.read().trashed,
                                history_operation: revision_panel_operation,
                                privacy_epoch,
                                library_generation,
                                note_generation: editor_generation,
                            },
                            actions: RevisionHistoryPanelActions {
                                onrestore: EventHandler::new({
                                    let attachment_service = attachment_service.clone();
                                    let workflow_service = workflow_service.clone();
                                    let services = services.clone();
                                    move |request: NoteRevisionRestoreRequest| {
                                        if *restoring_revision.read()
                                            || draft.read().trashed
                                            || draft.read().protection.is_locked()
                                        {
                                            return;
                                        }
                                        let editing_pause = workflow_service.pause_editing();
                                        restoring_revision.set(true);
                                        onstatus.call(ApplicationOperation::RestoreNoteRevision.progress_label().to_string());
                                        let current = draft.read().clone();
                                        let current_generation = *editor_generation.read();
                                        let restore_publication_scope = privacy_epoch
                                            .note_publication_scope(
                                                services.privacy(),
                                                current.id.clone(),
                                                editor_generation,
                                            );
                                        let Some(restore_publication_token) =
                                            restore_publication_scope.capture()
                                        else {
                                            restoring_revision.set(false);
                                            return;
                                        };
                                        let attachment_service = attachment_service.clone();
                                        let restore_workflows = workflow_service.clone();
                                        spawn(async move {
                                            let _editing_pause = editing_pause;
                                            match restore_revision_flow(
                                                RevisionRestoreFlow {
                                                    attachment_service,
                                                    workflows: restore_workflows,
                                                    current,
                                                    current_generation,
                                                    request,
                                                    editor_preferences: *editor_preferences.read(),
                                                    pre_restore_publication_token:
                                                        restore_publication_token.clone(),
                                                    publication_scope:
                                                        restore_publication_scope.clone(),
                                                },
                                                next_editor_generation,
                                            )
                                            .await
                                            {
                                                Ok(result) => {
                                                    let Some(_publication_lease) =
                                                        result.publication_token.begin_publication()
                                                    else {
                                                        restoring_revision.set(false);
                                                        return;
                                                    };
                                                    editor_generation.set(result.generation);
                                                    latest_editor_revision.set(0);
                                                    let next_projection_request =
                                                        (*editor_projection_request_generation.read())
                                                            .wrapping_add(1);
                                                    editor_projection_request_generation
                                                        .set(next_projection_request);
                                                    tags_text.set(result.note.tags_text());
                                                    attachments.set(result.attachments);
                                                    note_link_targets.set(result.note_link_targets);
                                                    text_statistics.set(TextStatistics::from_text(
                                                        result.note.searchable_body(),
                                                    ));
                                                    let next_revision =
                                                        (*editor_document_revision.read()).wrapping_add(1);
                                                    editor_document_revision.set(next_revision);
                                                    draft.set(result.note.clone());
                                                    if result.requires_remount {
                                                        let next = (*editor_mount_epoch.read()).wrapping_add(1);
                                                        editor_mount_epoch.set(next);
                                                    }
                                                    editor_popover.set(None);
                                                    right_panel_mode.set(None);
                                                    restoring_revision.set(false);
                                                    onrestored.call(result.note);
                                                    let message = if result.warnings.is_empty() {
                                                        "Note revision restored; the replaced state remains in history".to_string()
                                                    } else {
                                                        format!(
                                                            "Note revision restored with {} warning(s): {}",
                                                            result.warnings.len(),
                                                            result.warnings.join(" ")
                                                        )
                                                    };
                                                    onstatus.call(message);
                                                    focus_live_editor();
                                                }
                                                Err(error) => {
                                                    restoring_revision.set(false);
                                                    if let Some(post_restore_publication_token) =
                                                        restore_publication_scope.capture()
                                                        && let Some(_publication_lease) =
                                                            post_restore_publication_token
                                                                .begin_publication()
                                                    {
                                                        onstatus.call(format!(
                                                            "Could not restore version: {error}"
                                                        ));
                                                    }
                                                }
                                            }
                                        });
                                    }
                                }),
                                onstatus,
                            },
                        }
                    }
                }
            }
            }
            if let Some(request) = note_link_picker.read().clone() {
                NoteLinkPicker {
                    key: "{request.request_id}",
                    view: NoteLinkPickerView {
                        library: services.library(),
                        request: request.clone(),
                        current_note_id: draft.read().id.clone(),
                        privacy_epoch,
                        library_generation,
                        note_generation: editor_generation,
                    },
                    actions: NoteLinkPickerActions {
                        onselect: EventHandler::new(move |candidate| {
                            let Some(active_request) = note_link_picker.peek().clone() else {
                                return;
                            };
                            let Some(publication_token) = privacy_epoch.capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)
                            else {
                                note_link_picker.set(None);
                                return;
                            };
                            let script = live_editor_complete_note_link_picker_script(
                                &active_request.request_id,
                                &candidate,
                            );
                            note_link_picker.set(None);
                            let candidate_target = NoteLinkTarget {
                                note_id: candidate.id.clone(),
                                title: Some(candidate.display_title().to_string()),
                                state: if candidate.archived {
                                    crate::domain::NoteLinkTargetState::Archived
                                } else {
                                    crate::domain::NoteLinkTargetState::Active
                                },
                            };
                            let Some(_publication_lease) = publication_token.begin_publication() else {
                                note_link_picker.set(None);
                                return;
                            };
                            let mut targets = note_link_targets.peek().clone();
                            if let Some(existing) = targets
                                .iter_mut()
                                .find(|target| target.note_id == candidate_target.note_id)
                            {
                                *existing = candidate_target;
                            } else {
                                targets.push(candidate_target);
                            }
                            note_link_targets.set(targets);
                            spawn(async move {
                                let Some(_publication_lease) = publication_token.begin_publication() else {
                                    return;
                                };
                                match document::eval(&script).join::<bool>().await {
                                    Ok(true) => {}
                                    _ if publication_token.is_publishable() => {
                                        onstatus.call("Could not insert the note link".to_string())
                                    }
                                    _ => {}
                                }
                            });
                        }),
                        oncancel: EventHandler::new(move |_| {
                            let Some(active_request) = note_link_picker.peek().clone() else {
                                return;
                            };
                            note_link_picker.set(None);
                            let script =
                                live_editor_fail_note_link_picker_script(&active_request.request_id);
                            spawn(async move {
                                let _ = document::eval(&script).join::<bool>().await;
                                focus_live_editor();
                            });
                        }),
                        onstatus,
                    },
                }
            }
        }
    }
}

pub(super) fn schedule_note_find(
    query: String,
    requested_index: usize,
    mut find_count: Signal<usize>,
    mut find_index: Signal<usize>,
    request_generation: u64,
    find_request_generation: Signal<u64>,
    debounce_query_change: bool,
) {
    let script = live_editor_find_script(&query, requested_index);
    spawn(async move {
        if debounce_query_change {
            tokio::time::sleep(std::time::Duration::from_millis(125)).await;
        }
        if *find_request_generation.peek() != request_generation {
            return;
        }
        if let Ok(total) = document::eval(&script).join::<usize>().await {
            if *find_request_generation.peek() != request_generation {
                return;
            }
            find_count.set(total);
            if total == 0 {
                if *find_index.read() != 0 {
                    find_index.set(0);
                }
            } else if *find_index.read() >= total {
                find_index.set(total - 1);
            }
        }
    });
}

pub(super) fn schedule_draft_scoped(
    editor: &EditorService,
    updated: Note,
    scope: NoteChangeScope,
    onstatus: EventHandler<String>,
) {
    match editor.schedule_note(updated, scope) {
        Ok(()) => onstatus.call("Unsaved changes".to_string()),
        Err(error) => onstatus.call(format!("Could not schedule autosave: {error}")),
    }
}
