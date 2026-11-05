//! Dioxus interface, organized by surface and responsibility.

use std::{future::Future, time::Duration};

use dioxus::core::{Runtime, ScopeId};
use dioxus::prelude::*;

use crate::application::{context::ApplicationServices, library::LibraryService};
use crate::{
    domain::{Note, NoteFilters, NoteId, NoteListCursor, Notebook, Section},
    security::SecretString,
};

mod app;
mod background;
mod context_menu;
mod editor;
mod editor_preferences;
mod editor_workflows;
mod editor_toolbar;
mod editor_toolbar_support;
mod entity_actions;
mod entity_commands;
mod eval_bridge;
mod filter_popover;
mod live_editor;
mod note_card;
mod note_hierarchy;
mod note_links;
mod note_list_data;
mod note_metadata;
mod note_picker;
mod note_properties;
mod portability;
mod privacy;
mod notes_header;
mod responsive;
mod right_panel;
mod search_help;
mod settings;
mod settings_support;
mod shortcuts_help;
mod shutdown;
mod sidebar;
mod startup;
mod sources;
mod synchronization;
mod task_error;
mod toast;

pub use responsive::{
    COMPACT_LAYOUT_BREAKPOINT, DEFAULT_NOTE_LIST_WIDTH, DEFAULT_SIDEBAR_WIDTH,
    KEYBOARD_RESIZE_STEP, MAX_NOTE_LIST_WIDTH, MAX_SIDEBAR_WIDTH, MIN_EDITOR_WIDTH,
    MIN_NOTE_LIST_WIDTH, MIN_SIDEBAR_WIDTH, ResizeState, SIDEBAR_OVERLAY_BREAKPOINT,
    SPLITTER_WIDTH, clamp_note_list_width, clamp_sidebar_width, effective_layout_widths,
    layout_widths_for_resize, note_list_width_for_viewport, note_list_width_from_pointer,
    reset_note_list_width, reset_sidebar_width, resize_note_list_with_keyboard,
    resize_sidebar_with_keyboard, sidebar_width_for_viewport, sidebar_width_from_pointer,
};
pub use startup::App;
const LIBRARY_SEARCH_DEBOUNCE_MS: u64 = 300;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CompactPane {
    #[default]
    Notes,
    Editor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListPopover {
    Filter,
    Sort,
    SearchHelp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SettingsSection {
    #[default]
    Appearance,
    Editor,
    ImportExport,
    Backups,
    History,
    Storage,
    Shortcuts,
    Diagnostics,
}

impl SettingsSection {
    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Editor => "Editor",
            Self::ImportExport => "Import & export",
            Self::Backups => "Backups",
            Self::History => "Version history",
            Self::Storage => "Storage",
            Self::Shortcuts => "Keyboard shortcuts",
            Self::Diagnostics => "Diagnostics",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Appearance => "◐",
            Self::Editor => "¶",
            Self::ImportExport => "⇄",
            Self::Backups => "⇩",
            Self::History => "↶",
            Self::Storage => "▤",
            Self::Shortcuts => "⌨",
            Self::Diagnostics => "◉",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorPopover {
    Folder,
    Tags,
    Export,
    Typography,
    ParagraphLayout,
    BlockStyle,
    ListStyle,
    HighlightPalette,
    Link,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToastKind {
    Info,
    Error,
}

impl ToastKind {
    fn class_name(self) -> &'static str {
        match self {
            Self::Info => "toast",
            Self::Error => "toast error",
        }
    }

    fn timeout(self) -> Duration {
        match self {
            Self::Info => Duration::from_secs(4),
            Self::Error => Duration::from_secs(8),
        }
    }
}

#[derive(Clone, Debug)]
struct ToastMessage {
    id: u64,
    text: String,
    kind: ToastKind,
    privacy_token: Option<privacy::PrivacyPublicationToken>,
}

impl ToastMessage {
    fn is_publishable(&self) -> bool {
        self.privacy_token
            .as_ref()
            .is_none_or(privacy::PrivacyPublicationToken::observe_publishable)
    }
}

impl PartialEq for ToastMessage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.text == other.text && self.kind == other.kind
    }
}

impl Eq for ToastMessage {}

pub(super) use crate::domain::NoteSort;

#[derive(Clone, Debug, PartialEq, Eq)]
enum TagCatalogMutation {
    Rename { from: String, to: String },
    Delete { name: String },
}

enum NoteCardAction {
    Rename { note_id: NoteId, title: String },
    Encrypt { note_id: NoteId, password: SecretString, public_label: String },
    UnlockEncrypted { note_id: NoteId, password: SecretString },
    LockEncrypted(NoteId),
    RemoveEncryption { note_id: NoteId, password: SecretString },
    TogglePin(NoteId),
    Move { note_id: NoteId, notebook_id: String },
    ToggleArchive(NoteId),
    ToggleTrash(NoteId),
    Duplicate(NoteId),
    DeletePermanently(NoteId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VaultTransitionUiState {
    note_id: NoteId,
    /// Non-secret label only. Never carry the decrypted/private title here.
    public_label: String,
    message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NoteCardTagAction {
    Rename { note_id: NoteId, from: String, to: String },
    Remove { note_id: NoteId, name: String },
    Delete { note_id: NoteId, name: String },
}

/// Signals owned by the note-list workflow. This is deliberately scoped to
/// library navigation and mutations rather than acting as a global app context.
#[derive(Clone, Copy, PartialEq)]
struct NotesListState {
    task_scope: ScopeId,
    services: Signal<ApplicationServices>,
    section: Signal<Section>,
    selected_notebook: Signal<Option<String>>,
    search: Signal<String>,
    search_generation: Signal<u64>,
    list_generation: Signal<u64>,
    note_selection_generation: Signal<u64>,
    note_view_generation: Signal<u64>,
    privacy_epoch: privacy::UiPrivacyEpoch,
    list_request_sequence: Signal<u64>,
    list_page_request: Signal<Option<note_list_data::NotePageRequest>>,
    sort_order: Signal<NoteSort>,
    filters: Signal<NoteFilters>,
    tags: Signal<Vec<String>>,
    preference_save_generation: Signal<u64>,
    note_cache: Signal<note_list_data::NoteListCache>,
    note_count: Signal<usize>,
    page_start_cursors: Signal<Vec<Option<NoteListCursor>>>,
    list_loading: Signal<bool>,
    list_scroll_top: Signal<f64>,
    list_viewport_height: Signal<f64>,
    selected_note: Signal<Option<Note>>,
    vault_transition: Signal<Option<VaultTransitionUiState>>,
    vault_action_busy: Signal<Option<NoteId>>,
    tag_editor_request: Signal<Option<NoteId>>,
    status: Signal<String>,
    editor_session: Signal<Option<synchronization::EditorSession>>,
}

impl NotesListState {
    /// Spawn a short-lived UI workflow in the `RunningApp` scope that owns all
    /// Signals carried by this state. This deliberately survives child/card/editor
    /// remounts without hoisting component-owned Signals into Dioxus's root scope.
    fn spawn(self, future: impl Future<Output = ()> + 'static) {
        Runtime::current().spawn(self.task_scope, future);
    }

    fn advance_note_view_generation(mut self) -> u64 {
        let generation = (*self.note_view_generation.peek()).wrapping_add(1);
        self.note_view_generation.set(generation);
        generation
    }

    fn advance_note_selection_generation(mut self) -> u64 {
        let generation = (*self.note_selection_generation.peek()).wrapping_add(1);
        self.note_selection_generation.set(generation);
        generation
    }

    fn note_selection_is_current(self, expected: u64) -> bool {
        *self.note_selection_generation.peek() == expected
    }

    /// A selected Vault action checkpoints its source note before opening the
    /// fail-closed UI revocation window. While that window is active the
    /// plaintext `selected_note` is intentionally absent, but navigation/create
    /// must not fall back to a multi-note flush that would wait on the very
    /// revocation being escaped. This marker is valid only while the matching
    /// application-scoped Vault action guard is still active.
    fn selected_source_checkpointed_by_active_vault_transition(self) -> bool {
        let transition = self.vault_transition.peek();
        let busy = self.vault_action_busy.peek();
        self.selected_note.peek().is_none()
            && transition.as_ref().is_some_and(|transition| {
                busy.as_ref() == Some(&transition.note_id)
            })
    }

    fn capture_multi_note_publication_token(self) -> Option<privacy::PrivacyPublicationToken> {
        self.privacy_epoch.capture_multi_note_publication(
            &self.services.peek().privacy(),
            self.list_generation,
        )
    }

    fn capture_note_publication_token(
        self,
        note_id: &NoteId,
        lifecycle_generation: Signal<u64>,
    ) -> Option<privacy::PrivacyPublicationToken> {
        self.privacy_epoch.capture_note_publication(
            &self.services.peek().privacy(),
            note_id,
            lifecycle_generation,
        )
    }

    fn capture_note_action_publication_token(
        self,
        note_id: &NoteId,
    ) -> Option<privacy::PrivacyPublicationToken> {
        self.privacy_epoch.capture_note_publication_without_lifecycle(
            &self.services.peek().privacy(),
            note_id,
        )
    }

    async fn wait_for_note_action_publication_token(
        self,
        note_id: &NoteId,
    ) -> privacy::PrivacyPublicationToken {
        loop {
            if let Some(token) = self.capture_note_action_publication_token(note_id) {
                return token;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn wait_for_multi_note_publication_token(
        self,
        selection_generation: Option<u64>,
    ) -> Option<privacy::PrivacyPublicationToken> {
        loop {
            if selection_generation
                .is_some_and(|expected| !self.note_selection_is_current(expected))
            {
                return None;
            }
            if let Some(token) = self.capture_multi_note_publication_token() {
                return Some(token);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Synchronize only the current source editor under source-note authority.
    /// The returned token never authorizes publication of a different target note.
    async fn synchronize_current_editor_when_privacy_ready(
        self,
        editor: crate::application::editor::EditorService,
        mode: synchronization::SynchronizationMode,
        selection_generation: u64,
    ) -> Result<Option<privacy::PrivacyPublicationToken>, synchronization::SynchronizationError> {
        loop {
            if !self.note_selection_is_current(selection_generation) {
                return Ok(None);
            }
            let source_note_id = self
                .selected_note
                .peek()
                .as_ref()
                .map(|note| note.id.clone());
            let publication_token = match source_note_id.as_ref() {
                Some(note_id) => self.capture_note_publication_token(
                    note_id,
                    self.note_selection_generation,
                ),
                None => self.privacy_epoch.capture_multi_note_publication_with_lifecycle(
                    &self.services.peek().privacy(),
                    self.list_generation,
                    self.note_selection_generation,
                ),
            };
            let Some(publication_token) = publication_token else {
                tokio::time::sleep(Duration::from_millis(25)).await;
                continue;
            };
            let result = synchronization::synchronize_current_editor(
                editor.clone(),
                self.selected_note,
                self.editor_session,
                mode,
                publication_token.clone(),
            )
            .await;
            if !self.note_selection_is_current(selection_generation) {
                return Ok(None);
            }
            if !publication_token.is_publishable() {
                continue;
            }
            result?;
            return Ok(Some(publication_token));
        }
    }


    async fn synchronize_then_begin_multi_note_publication_when_privacy_ready(
        self,
        editor: crate::application::editor::EditorService,
        mode: synchronization::SynchronizationMode,
        selection_generation: u64,
    ) -> Result<Option<privacy::PrivacyPublicationLease>, synchronization::SynchronizationError> {
        let Some(_source_token) = self
            .synchronize_current_editor_when_privacy_ready(
                editor,
                mode,
                selection_generation,
            )
            .await?
        else {
            return Ok(None);
        };
        loop {
            let Some(token) = self
                .wait_for_multi_note_publication_token(Some(selection_generation))
                .await
            else {
                return Ok(None);
            };
            if let Some(lease) = token.begin_publication() {
                return Ok(Some(lease));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatAction {
    Undo,
    Redo,
    Paragraph,
    Heading1,
    Heading2,
    Heading3,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Superscript,
    Subscript,
    Highlight,
    BulletList,
    NumberedList,
    Checklist,
    Outdent,
    Indent,
    BulletStyleDisc,
    BulletStyleDash,
    OrderedStyleAuto,
    OrderedStyleDecimal,
    OrderedStyleLowerRoman,
    OrderedStyleLowerAlpha,
    OrderedRestart,
    OrderedContinue,
    Quote,
    HorizontalRule,
}

impl FormatAction {
    fn script_name(self) -> &'static str {
        match self {
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Paragraph => "paragraph",
            Self::Heading1 => "heading1",
            Self::Heading2 => "heading2",
            Self::Heading3 => "heading3",
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Underline => "underline",
            Self::Strikethrough => "strikethrough",
            Self::Superscript => "superscript",
            Self::Subscript => "subscript",
            Self::Highlight => "highlight",
            Self::BulletList => "bullet",
            Self::NumberedList => "numbered",
            Self::Checklist => "checklist",
            Self::Outdent => "outdent",
            Self::Indent => "indent",
            Self::BulletStyleDisc => "bullet_style_disc",
            Self::BulletStyleDash => "bullet_style_dash",
            Self::OrderedStyleAuto => "list_style_auto",
            Self::OrderedStyleDecimal => "list_style_decimal",
            Self::OrderedStyleLowerRoman => "list_style_lower_roman",
            Self::OrderedStyleLowerAlpha => "list_style_lower_alpha",
            Self::OrderedRestart => "list_restart",
            Self::OrderedContinue => "list_continue",
            Self::Quote => "quote",
            Self::HorizontalRule => "horizontal_rule",
        }
    }
}


fn request_note_list_preferences_save(state: NotesListState) {
    let mut generation = state.preference_save_generation;
    let next_generation = (*generation.read()).wrapping_add(1);
    generation.set(next_generation);
}

fn refresh_tags(
    library: &LibraryService,
    mut tags: Signal<Vec<String>>,
    mut status: Signal<String>,
) -> bool {
    match library.list_tags() {
        Ok(next) => {
            tags.set(next);
            true
        }
        Err(error) => {
            status.set(format!("Could not refresh tags: {error}"));
            false
        }
    }
}

fn refresh_notebooks(
    library: &LibraryService,
    mut notebooks: Signal<Vec<Notebook>>,
    mut status: Signal<String>,
) -> bool {
    match library.list_notebooks() {
        Ok(next) => {
            notebooks.set(next);
            true
        }
        Err(error) => {
            status.set(format!("Could not refresh notebooks: {error}"));
            false
        }
    }
}

fn storage_or_report<T, E>(
    result: Result<T, E>,
    action: &str,
    onstatus: EventHandler<String>,
) -> Option<T>
where
    E: std::fmt::Display,
{
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            onstatus.call(format!("{action}: {error}"));
            None
        }
    }
}
