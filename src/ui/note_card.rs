use dioxus::core::use_drop;
use dioxus::prelude::*;
use crate::application::{
    context::ApplicationServices, editor::EditorService, errors::EditorError, executor::{ApplicationExecutionError, ApplicationExecutor, ExecutionFailure},
    notes::NoteService, vault::VaultProtectionService,
};

use crate::domain::{Note, NoteId, NoteSummary, Notebook, Section};
use crate::search::{ParsedSearch, SearchField, contextual_summary_preview, highlight_segments};
use crate::application::persistence::{NoteChangeScope, unix_timestamp};
use crate::application::errors::DataAccessError;
use crate::security::VaultError;
use crate::storage::StorageError;
use crate::vault_profile::{VaultProfileOperation, VaultProfileSpan, VaultProfileStage};

use super::note_list_data::{
    refresh_note_list, reload_note_list, remove_loaded_note, update_loaded_note,
};
use super::entity_commands::{TagCommand, TagCommandError, execute_tag_command_blocking};
use super::synchronization::{SynchronizationMode, synchronize_for_note_mutation};
use super::sidebar::indented_notebook_name;
use super::responsive::focus_selected_note_surface;
use super::{NoteCardAction, NoteCardTagAction, NotesListState};

#[derive(Clone, PartialEq)]
pub(super) struct NoteCardView {
    pub note: NoteSummary,
    pub notebook_name: String,
    pub notebooks: Vec<Notebook>,
    pub query: String,
    pub active: bool,
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteCardActions {
    pub onopen: EventHandler<NoteId>,
    pub onaction: EventHandler<NoteCardAction>,
    pub onopentag: EventHandler<String>,
    pub ontags: EventHandler<NoteId>,
    pub ontagaction: EventHandler<NoteCardTagAction>,
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteCardTagChipView {
    pub note_id: NoteId,
    pub tag: String,
    pub query: String,
    pub read_only: bool,
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteCardTagChipActions {
    pub onopen: EventHandler<String>,
    pub onaction: EventHandler<NoteCardTagAction>,
}

#[component]
fn NoteCardTagChip(view: NoteCardTagChipView, actions: NoteCardTagChipActions) -> Element {
    use super::context_menu::{
        ContextMenuController, EntityContextMenu, EntityContextMenuActions, EntityContextMenuView,
    };
    use super::entity_actions::{
        EntityAction, EntityActionContext, EntityActionRegistry, EntityActionState,
        EntityActionTargets, EntitySurface,
    };

    let NoteCardTagChipView {
        note_id,
        tag,
        query,
        read_only,
    } = view;
    let NoteCardTagChipActions { onopen, onaction } = actions;
    let mut context_menu = use_signal(ContextMenuController::default);
    let mut renaming = use_signal(|| false);
    let mut rename_value = use_signal(|| tag.clone());
    let chip_id = format!("note-card-tag-{}-{}", note_id, stable_dom_suffix(&tag));
    let menu_id = format!("{chip_id}-menu");
    let rename_id = format!("{chip_id}-rename");
    let entries = EntityActionRegistry::entries(
        &EntityActionContext {
            surface: EntitySurface::Tag,
            entity_id: tag.clone(),
            state: EntityActionState {
                read_only,
                ..EntityActionState::default()
            },
        },
        &EntityActionTargets::default(),
    );

    rsx! {
        span { class: "note-card-tag-shell",
            if *renaming.read() {
                input {
                    id: "{rename_id}",
                    class: "note-card-tag-rename",
                    value: "{rename_value}",
                    aria_label: "Rename tag {tag}",
                    autofocus: true,
                    oninput: move |event| rename_value.set(event.value()),
                    onclick: move |event| event.stop_propagation(),
                    oncontextmenu: move |event| event.stop_propagation(),
                    onkeydown: {
                        let tag = tag.clone();
                        let note_id = note_id.clone();
                        move |event: KeyboardEvent| match event.key() {
                            Key::Enter => {
                                event.prevent_default();
                                event.stop_propagation();
                                let next = rename_value.read().trim().to_string();
                                renaming.set(false);
                                if !next.is_empty() && next != tag {
                                    onaction.call(NoteCardTagAction::Rename {
                                        note_id: note_id.clone(),
                                        from: tag.clone(),
                                        to: next,
                                    });
                                }
                            }
                            Key::Escape => {
                                event.prevent_default();
                                event.stop_propagation();
                                rename_value.set(tag.clone());
                                renaming.set(false);
                            }
                            _ => {}
                        }
                    },
                    onblur: {
                        let tag = tag.clone();
                        let note_id = note_id.clone();
                        move |_| {
                            if !*renaming.peek() {
                                return;
                            }
                            let next = rename_value.read().trim().to_string();
                            renaming.set(false);
                            if !next.is_empty() && next != tag {
                                onaction.call(NoteCardTagAction::Rename {
                                    note_id: note_id.clone(),
                                    from: tag.clone(),
                                    to: next,
                                });
                            }
                        }
                    },
                }
            } else {
                button {
                    id: "{chip_id}",
                    class: "tag-pill note-card-tag-pill",
                    title: "Tag: {tag}",
                    aria_label: "Open tag {tag}",
                    aria_haspopup: "menu",
                    aria_controls: "{menu_id}",
                    aria_expanded: "{context_menu.read().is_open()}",
                    aria_keyshortcuts: "Shift+F10",
                    "data-context-long-press": "true",
                    onclick: {
                        let tag = tag.clone();
                        move |event: MouseEvent| {
                            event.prevent_default();
                            event.stop_propagation();
                            onopen.call(tag.clone());
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
                                && (event.key() == Key::ContextMenu
                                    || (event.key() == Key::F10 && event.modifiers().shift()))
                            {
                                event.prevent_default();
                                event.stop_propagation();
                                context_menu.write().open_anchor(chip_id.clone());
                            }
                        }
                    },
                    HighlightedText {
                        text: tag.clone(),
                        query: query.clone(),
                        field: SearchField::Tag,
                    }
                }
            }

            if context_menu.read().is_open() && !*renaming.read() {
                EntityContextMenu {
                    view: EntityContextMenuView {
                        id: menu_id.clone(),
                        label: format!("Actions for tag {tag}"),
                        return_focus_id: chip_id.clone(),
                        fallback_focus_id: "note-list-main".to_string(),
                        controller: context_menu,
                        entries,
                    },
                    actions: EntityContextMenuActions {
                        onaction: EventHandler::new({
                            let note_id = note_id.clone();
                            let tag = tag.clone();
                            let rename_id = rename_id.clone();
                            move |action: EntityAction| match action {
                                EntityAction::Open => onopen.call(tag.clone()),
                                EntityAction::Rename => {
                                    rename_value.set(tag.clone());
                                    renaming.set(true);
                                    let input_id = rename_id.clone();
                                    spawn(async move {
                                        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                                        let script = format!(
                                            "const input = document.getElementById({input_id:?}); if (input) {{ input.focus(); input.select(); return true; }} return false;"
                                        );
                                        let _ = document::eval(&script).join::<bool>().await;
                                    });
                                }
                                EntityAction::RemoveTagFromNote => onaction.call(NoteCardTagAction::Remove {
                                    note_id: note_id.clone(),
                                    name: tag.clone(),
                                }),
                                EntityAction::DeleteTag => onaction.call(NoteCardTagAction::Delete {
                                    note_id: note_id.clone(),
                                    name: tag.clone(),
                                }),
                                _ => {}
                            }
                        }),
                    },
                }
            }
        }
    }
}

fn stable_dom_suffix(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoteVaultPrompt {
    Encrypt,
    Unlock,
    RemoveEncryption,
}

impl NoteVaultPrompt {
    fn title(self) -> &'static str {
        match self {
            Self::Encrypt => "Encrypt note",
            Self::Unlock => "Unlock note",
            Self::RemoveEncryption => "Remove note encryption",
        }
    }

    fn submit_label(self) -> &'static str {
        match self {
            Self::Encrypt => "Encrypt",
            Self::Unlock => "Unlock",
            Self::RemoveEncryption => "Remove encryption",
        }
    }
}

#[component]
pub(super) fn NoteCard(view: NoteCardView, actions: NoteCardActions) -> Element {
    use super::context_menu::{
        ContextMenuController, EntityContextMenu, EntityContextMenuActions, EntityContextMenuView,
    };
    use super::entity_actions::{
        ActionTarget, EntityAction, EntityActionContext, EntityActionRegistry, EntityActionState,
        EntityActionTargets, EntitySurface,
    };

    let NoteCardView {
        note,
        notebook_name,
        notebooks,
        query,
        active,
    } = view;
    let NoteCardActions {
        onopen,
        onaction,
        onopentag,
        ontags,
        ontagaction,
    } = actions;
    let mut context_menu = use_signal(ContextMenuController::default);
    let mut renaming = use_signal(|| false);
    let mut rename_value = use_signal(|| note.title.clone());
    let mut vault_prompt = use_signal(|| None::<NoteVaultPrompt>);
    let mut vault_password = use_signal(String::new);
    let mut vault_confirm_password = use_signal(String::new);
    let mut vault_public_label = use_signal(String::new);
    let mut vault_error = use_signal(|| None::<String>);
    let mut vault_password_for_drop = vault_password;
    let mut vault_confirm_password_for_drop = vault_confirm_password;
    use_drop(move || {
        crate::security::clear_secret_input(&mut vault_password_for_drop.write());
        crate::security::clear_secret_input(&mut vault_confirm_password_for_drop.write());
    });
    let parsed_search = ParsedSearch::parse(&query);
    let preview = contextual_summary_preview(&note, &parsed_search);
    let attachment_match = preview.starts_with("Attachment: ");
    let title_placeholder = note.title.trim().is_empty();
    let preview_placeholder = note.preview_text.split_whitespace().next().is_none()
        && !attachment_match;
    let preview_field = if attachment_match {
        SearchField::Attachment
    } else {
        SearchField::Body
    };
    let hidden_tag_count = note.tags.len().saturating_sub(3);
    let attachment_count = note.attachment_names.len();
    let attachment_title = attachment_summary(&note.attachment_names);
    let accessible_label = format!(
        "{}, updated {}",
        note.display_title(),
        compact_relative_time(note.updated_at)
    );
    let card_class = if active {
        "note-card-shell active"
    } else {
        "note-card-shell"
    };
    let actions_label = format!("Open actions for {}", note.display_title());
    let card_id = format!("note-card-{}", note.id);
    let menu_trigger_id = format!("note-card-menu-trigger-{}", note.id);
    let menu_id = format!("note-card-menu-{}", note.id);
    let rename_input_id = format!("note-card-rename-{}", note.id);
    let action_context = EntityActionContext {
        surface: EntitySurface::Note,
        entity_id: note.id.to_string(),
        state: EntityActionState {
            pinned: note.pinned,
            archived: note.archived,
            trashed: note.trashed,
            protected: note.protection.is_protected(),
            unlocked: note.protection.is_unlocked(),
            read_only: note.protection.is_locked(),
            ..EntityActionState::default()
        },
    };
    let action_targets = EntityActionTargets {
        move_targets: notebooks
            .iter()
            .filter(|candidate| candidate.id != note.notebook_id)
            .map(|candidate| {
                ActionTarget::entity(
                    candidate.id.clone(),
                    indented_notebook_name(candidate, &notebooks),
                )
            })
            .collect(),
    };
    let menu_entries = EntityActionRegistry::entries(&action_context, &action_targets);

    rsx! {
        article { class: card_class,
            div {
                id: "{card_id}",
                class: if active { "note-card active" } else { "note-card" },
                tabindex: "0",
                aria_current: if active { "true" } else { "false" },
                aria_label: "{accessible_label}",
                "data-note-id": "{note.id}",
                "data-context-long-press": "true",
                "aria-keyshortcuts": "Shift+F10",
                onclick: {
                    let note_id = note.id.clone();
                    move |_| onopen.call(note_id.clone())
                },
                oncontextmenu: {
                    let card_id = card_id.clone();
                    move |event: MouseEvent| {
                        event.prevent_default();
                        event.stop_propagation();
                        let coordinates = event.client_coordinates();
                        let x = coordinates.x.round() as i32;
                        let y = coordinates.y.round() as i32;
                        if x == 0 && y == 0 {
                            context_menu.write().open_anchor(card_id.clone());
                        } else {
                            context_menu.write().open_pointer(x, y);
                        }
                    }
                },
                onkeydown: {
                    let card_id = card_id.clone();
                    let note_id = note.id.clone();
                    move |event: KeyboardEvent| {
                        if event.is_composing() {
                            return;
                        }
                        if event.key() == Key::ContextMenu
                            || (event.key() == Key::F10 && event.modifiers().shift())
                        {
                            event.prevent_default();
                            event.stop_propagation();
                            context_menu.write().open_anchor(card_id.clone());
                        } else {
                            let key = event.key();
                            if key == Key::Enter || matches!(key, Key::Character(value) if value == " ") {
                                event.prevent_default();
                                event.stop_propagation();
                                onopen.call(note_id.clone());
                            }
                        }
                    }
                },
                span { class: "note-card-selection-bar", aria_hidden: "true" }
                span { class: "note-card-content",
                    span { class: "note-card-title-row",
                        span { class: "note-card-title-group",
                            if note.pinned {
                                span { class: "pin-indicator", title: "Pinned note", aria_label: "Pinned", "◆" }
                            }
                            span {
                                class: if title_placeholder { "note-card-heading placeholder" } else { "note-card-heading" },
                                HighlightedText {
                                    text: note.display_title().to_string(),
                                    query: query.clone(),
                                    field: SearchField::Title,
                                }
                            }
                        }
                        time {
                            class: "note-card-time",
                            title: "Last updated {relative_time(note.updated_at)}",
                            "{compact_relative_time(note.updated_at)}"
                        }
                    }

                    span {
                        class: if attachment_match {
                            "note-preview attachment-preview"
                        } else if preview_placeholder {
                            "note-preview placeholder"
                        } else {
                            "note-preview"
                        },
                        if attachment_match {
                            span { class: "preview-kind", aria_hidden: "true", "↗" }
                        }
                        HighlightedText {
                            text: preview,
                            query: query.clone(),
                            field: preview_field,
                        }
                    }

                    span { class: "note-card-footer",
                        span { class: "note-card-taxonomy",
                            span { class: "notebook-pill", title: "Notebook: {notebook_name}", "{notebook_name}" }
                            for tag in note.tags.iter().take(3) {
                                NoteCardTagChip {
                                    key: "{note.id}-{tag}",
                                    view: NoteCardTagChipView {
                                        note_id: note.id.clone(),
                                        tag: tag.clone(),
                                        query: query.clone(),
                                        read_only: note.trashed || note.protection.is_locked(),
                                    },
                                    actions: NoteCardTagChipActions {
                                        onopen: onopentag,
                                        onaction: ontagaction,
                                    },
                                }
                            }
                            if hidden_tag_count > 0 {
                                span { class: "tag-overflow", title: "{hidden_tag_count} more tag(s)", "+{hidden_tag_count}" }
                            }
                        }
                        if attachment_count > 0 {
                            span {
                                class: "attachment-indicator",
                                title: "{attachment_title}",
                                aria_label: "{attachment_count} attachment(s)",
                                span { class: "attachment-glyph", aria_hidden: "true", "⌁" }
                                span { "{attachment_count}" }
                            }
                        }
                    }
                }
            }

            if *renaming.read() {
                input {
                    id: "{rename_input_id}",
                    class: "note-card-rename-input",
                    value: "{rename_value}",
                    aria_label: "Rename note",
                    autofocus: true,
                    oninput: move |event| rename_value.set(event.value()),
                    onclick: move |event| event.stop_propagation(),
                    oncontextmenu: move |event| event.stop_propagation(),
                    onkeydown: {
                        let original_title = note.title.clone();
                        let note_id = note.id.clone();
                        move |event: KeyboardEvent| match event.key() {
                            Key::Enter => {
                                event.prevent_default();
                                event.stop_propagation();
                                let title = rename_value.read().clone();
                                renaming.set(false);
                                if title != original_title {
                                    onaction.call(NoteCardAction::Rename {
                                        note_id: note_id.clone(),
                                        title,
                                    });
                                }
                            }
                            Key::Escape => {
                                event.prevent_default();
                                event.stop_propagation();
                                rename_value.set(original_title.clone());
                                renaming.set(false);
                            }
                            _ => {}
                        }
                    },
                    onblur: {
                        let original_title = note.title.clone();
                        let note_id = note.id.clone();
                        move |_| {
                            if !*renaming.peek() {
                                return;
                            }
                            let title = rename_value.read().clone();
                            renaming.set(false);
                            if title != original_title {
                                onaction.call(NoteCardAction::Rename {
                                    note_id: note_id.clone(),
                                    title,
                                });
                            }
                        }
                    },
                }
            }

            button {
                id: "{menu_trigger_id}",
                class: if context_menu.read().is_open() {
                    "entity-action-trigger note-card-action-trigger active"
                } else {
                    "entity-action-trigger note-card-action-trigger"
                },
                title: "Note actions",
                aria_label: "{actions_label}",
                aria_controls: "{menu_id}",
                aria_haspopup: "menu",
                aria_expanded: "{context_menu.read().is_open()}",
                disabled: *renaming.read(),
                "data-transient-trigger": "true",
                oncontextmenu: {
                    let menu_trigger_id = menu_trigger_id.clone();
                    move |event: MouseEvent| {
                        event.prevent_default();
                        event.stop_propagation();
                        let coordinates = event.client_coordinates();
                        let x = coordinates.x.round() as i32;
                        let y = coordinates.y.round() as i32;
                        if x == 0 && y == 0 {
                            context_menu.write().open_anchor(menu_trigger_id.clone());
                        } else {
                            context_menu.write().open_pointer(x, y);
                        }
                    }
                },
                onclick: {
                    let menu_trigger_id = menu_trigger_id.clone();
                    move |event| {
                        event.stop_propagation();
                        if context_menu.read().is_open() {
                            context_menu.write().close();
                        } else {
                            context_menu.write().open_anchor(menu_trigger_id.clone());
                        }
                    }
                },
                "•••"
            }

            if context_menu.read().is_open() {
                EntityContextMenu {
                    view: EntityContextMenuView {
                        id: menu_id.clone(),
                        label: "Note actions".to_string(),
                        return_focus_id: card_id.clone(),
                        fallback_focus_id: "note-list-main".to_string(),
                        controller: context_menu,
                        entries: menu_entries,
                    },
                    actions: EntityContextMenuActions {
                        onaction: EventHandler::new({
                            let note_id = note.id.clone();
                            let note_title = note.title.clone();
                            let rename_input_id = rename_input_id.clone();
                            move |action: EntityAction| match action {
                                EntityAction::Open => onopen.call(note_id.clone()),
                                EntityAction::Rename => {
                                    rename_value.set(note_title.clone());
                                    renaming.set(true);
                                    let input_id = rename_input_id.clone();
                                    spawn(async move {
                                        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                                        let script = format!(
                                            "const input = document.getElementById({input_id:?}); if (input) {{ input.focus(); input.select(); return true; }} return false;"
                                        );
                                        let _ = document::eval(&script).join::<bool>().await;
                                    });
                                }
                                EntityAction::EncryptNote => {
                                    crate::security::clear_secret_input(&mut vault_password.write());
                                    crate::security::clear_secret_input(&mut vault_confirm_password.write());
                                    vault_public_label.set(note_title.clone());
                                    if vault_public_label.read().trim().is_empty() {
                                        vault_public_label.set("Untitled note".to_string());
                                    }
                                    vault_error.set(None);
                                    vault_prompt.set(Some(NoteVaultPrompt::Encrypt));
                                }
                                EntityAction::UnlockNote => {
                                    crate::security::clear_secret_input(&mut vault_password.write());
                                    crate::security::clear_secret_input(&mut vault_confirm_password.write());
                                    vault_error.set(None);
                                    vault_prompt.set(Some(NoteVaultPrompt::Unlock));
                                }
                                EntityAction::LockNote => {
                                    onaction.call(NoteCardAction::LockEncrypted(note_id.clone()))
                                }
                                EntityAction::RemoveNoteEncryption => {
                                    crate::security::clear_secret_input(&mut vault_password.write());
                                    crate::security::clear_secret_input(&mut vault_confirm_password.write());
                                    vault_error.set(None);
                                    vault_prompt.set(Some(NoteVaultPrompt::RemoveEncryption));
                                }
                                EntityAction::TogglePin => {
                                    onaction.call(NoteCardAction::TogglePin(note_id.clone()))
                                }
                                EntityAction::EditTags => ontags.call(note_id.clone()),
                                EntityAction::DuplicateNote => {
                                    onaction.call(NoteCardAction::Duplicate(note_id.clone()))
                                }
                                EntityAction::MoveNoteTo(notebook_id) => {
                                    onaction.call(NoteCardAction::Move {
                                        note_id: note_id.clone(),
                                        notebook_id,
                                    })
                                }
                                EntityAction::ToggleArchive => {
                                    onaction.call(NoteCardAction::ToggleArchive(note_id.clone()))
                                }
                                EntityAction::MoveToTrash | EntityAction::RestoreFromTrash => {
                                    onaction.call(NoteCardAction::ToggleTrash(note_id.clone()))
                                }
                                EntityAction::DeletePermanently => {
                                    onaction.call(NoteCardAction::DeletePermanently(note_id.clone()))
                                }
                                EntityAction::CreateChildNotebook
                                | EntityAction::MoveNotebookTo(_)
                                | EntityAction::DeleteNotebook
                                | EntityAction::RemoveTagFromNote
                                | EntityAction::DeleteTag
                                | EntityAction::SaveAttachmentAs
                                | EntityAction::RevealAttachment
                                | EntityAction::RemoveAttachmentFromNote => {}
                            }
                        }),
                    },
                }
            }


            if let Some(prompt) = *vault_prompt.read() {
                div {
                    class: "vault-dialog-backdrop",
                    "data-transient-close": "note-vault-dialog",
                    onclick: move |_| {
                        crate::security::clear_secret_input(&mut vault_password.write());
                        crate::security::clear_secret_input(&mut vault_confirm_password.write());
                        vault_error.set(None);
                        vault_prompt.set(None);
                    },
                    section {
                        id: "note-vault-dialog",
                        class: "vault-dialog",
                        role: "dialog",
                        aria_modal: "true",
                        aria_label: "{prompt.title()}",
                        "data-transient-surface": "true",
                        "data-transient-kind": "dialog",
                        onclick: move |event| event.stop_propagation(),
                        h2 { "{prompt.title()}" }
                        p {
                            class: "vault-dialog-copy",
                            if prompt == NoteVaultPrompt::Encrypt {
                                "Use a long, unique passphrase or a password-manager-generated password. Rustnote does not store the password and cannot recover the note if you forget it."
                            } else if prompt == NoteVaultPrompt::Unlock {
                                "Enter the note password. The note stays unlocked only for this Rustnote session or until you lock it again."
                            } else {
                                "Enter the note password to restore this note to ordinary unencrypted storage."
                            }
                        }
                        if prompt == NoteVaultPrompt::Encrypt {
                            label { class: "vault-dialog-field",
                                span { "Locked-note label" }
                                input {
                                    r#type: "text",
                                    value: "{vault_public_label}",
                                    maxlength: "160",
                                    autocomplete: "off",
                                    oninput: move |event| {
                                        vault_public_label.set(event.value());
                                        vault_error.set(None);
                                    },
                                }
                                small { class: "vault-dialog-help",
                                    "Visible without the password. Use a neutral label if the current title is sensitive."
                                }
                            }
                        }
                        label { class: "vault-dialog-field",
                            span { "Password" }
                            input {
                                r#type: "password",
                                value: "{vault_password}",
                                autofocus: true,
                                autocomplete: "current-password",
                                oninput: move |event| {
                                    crate::security::replace_secret_input(&mut vault_password.write(), event.value());
                                    vault_error.set(None);
                                },
                            }
                        }
                        if prompt == NoteVaultPrompt::Encrypt {
                            label { class: "vault-dialog-field",
                                span { "Confirm password" }
                                input {
                                    r#type: "password",
                                    value: "{vault_confirm_password}",
                                    autocomplete: "new-password",
                                    oninput: move |event| {
                                        crate::security::replace_secret_input(&mut vault_confirm_password.write(), event.value());
                                        vault_error.set(None);
                                    },
                                }
                            }
                        }
                        if let Some(error) = vault_error.read().as_ref() {
                            p { class: "vault-dialog-error", role: "alert", "{error}" }
                        }
                        div { class: "vault-dialog-actions",
                            button {
                                class: "vault-dialog-button secondary",
                                onclick: move |_| {
                                    crate::security::clear_secret_input(&mut vault_password.write());
                                    crate::security::clear_secret_input(&mut vault_confirm_password.write());
                                    vault_error.set(None);
                                    vault_prompt.set(None);
                                },
                                "Cancel"
                            }
                            button {
                                class: "vault-dialog-button primary",
                                onclick: {
                                    let note_id = note.id.clone();
                                    move |_| {
                                        let password = crate::security::SecretString::take_from(
                                            &mut vault_password.write(),
                                        );
                                        if password.is_empty() {
                                            vault_error.set(Some("Enter a password.".to_string()));
                                            return;
                                        }
                                        if prompt == NoteVaultPrompt::Encrypt {
                                            let confirmation = crate::security::SecretString::take_from(
                                                &mut vault_confirm_password.write(),
                                            );
                                            if password.as_bytes() != confirmation.as_bytes() {
                                                vault_error.set(Some(
                                                    "The passwords do not match.".to_string(),
                                                ));
                                                return;
                                            }
                                        }
                                        let action = match prompt {
                                            NoteVaultPrompt::Encrypt => NoteCardAction::Encrypt {
                                                note_id: note_id.clone(),
                                                password,
                                                public_label: vault_public_label.read().clone(),
                                            },
                                            NoteVaultPrompt::Unlock => NoteCardAction::UnlockEncrypted {
                                                note_id: note_id.clone(),
                                                password,
                                            },
                                            NoteVaultPrompt::RemoveEncryption => NoteCardAction::RemoveEncryption {
                                                note_id: note_id.clone(),
                                                password,
                                            },
                                        };
                                        crate::security::clear_secret_input(&mut vault_password.write());
                                        crate::security::clear_secret_input(&mut vault_confirm_password.write());
                                        vault_error.set(None);
                                        vault_prompt.set(None);
                                        onaction.call(action);
                                    }
                                },
                                "{prompt.submit_label()}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn HighlightedText(text: String, query: String, field: SearchField) -> Element {
    let parsed = ParsedSearch::parse(&query);
    let terms = parsed.highlight_terms(field);
    let segments = highlight_segments(&text, &terms);
    rsx! {
        span {
            for (index, segment) in segments.into_iter().enumerate() {
                if segment.1 {
                    mark { key: "{index}", class: "search-highlight", "{segment.0}" }
                } else {
                    span { key: "{index}", "{segment.0}" }
                }
            }
        }
    }
}

fn vault_profile_operation(action: &NoteCardAction) -> Option<VaultProfileOperation> {
    match action {
        NoteCardAction::Encrypt { .. } => Some(VaultProfileOperation::Protect),
        NoteCardAction::UnlockEncrypted { .. } => Some(VaultProfileOperation::Unlock),
        NoteCardAction::LockEncrypted(_) => Some(VaultProfileOperation::Lock),
        NoteCardAction::RemoveEncryption { .. } => Some(VaultProfileOperation::RemoveEncryption),
        _ => None,
    }
}

fn vault_action_progress_message(action: &NoteCardAction) -> Option<&'static str> {
    match action {
        NoteCardAction::Encrypt { .. } => Some("Encrypting note…"),
        NoteCardAction::UnlockEncrypted { .. } => Some("Unlocking note…"),
        NoteCardAction::LockEncrypted(_) => Some("Locking note…"),
        NoteCardAction::RemoveEncryption { .. } => Some("Removing note encryption…"),
        _ => None,
    }
}

struct VaultActionBusyGuard {
    busy: Signal<Option<NoteId>>,
    note_id: NoteId,
}

impl VaultActionBusyGuard {
    fn begin(mut busy: Signal<Option<NoteId>>, note_id: &NoteId) -> Option<Self> {
        if busy.peek().is_some() {
            return None;
        }
        busy.set(Some(note_id.clone()));
        Some(Self { busy, note_id: note_id.clone() })
    }
}

impl Drop for VaultActionBusyGuard {
    fn drop(&mut self) {
        let mut busy = self.busy;
        if busy.peek().as_ref() == Some(&self.note_id) {
            busy.set(None);
        }
    }
}

fn should_republish_selected_vault_target(
    state: NotesListState,
    note_id: &NoteId,
    selected_target_at_revocation: bool,
    selection_generation_at_revocation: u64,
) -> bool {
    selected_target_at_revocation
        && state.note_selection_is_current(selection_generation_at_revocation)
        && state
            .vault_transition
            .peek()
            .as_ref()
            .is_some_and(|transition| &transition.note_id == note_id)
}

async fn read_canonical_note_for_vault_publication(
    notes: NoteService,
    state: NotesListState,
    note_id: NoteId,
) -> Result<(Option<Note>, super::privacy::PrivacyPublicationLease), NoteCardActionError> {
    loop {
        // Capture fresh B authority before the canonical reread. If another B or
        // library transition races the read, reject the snapshot and retry.
        let publication_token = state
            .wait_for_note_action_publication_token(&note_id)
            .await;
        let reread_notes = notes.clone();
        let reread_id = note_id.clone();
        let canonical = ApplicationExecutor::read("privacy action canonical reread", move || {
            reread_notes
                .get(&reread_id)
                .map_err(NoteCardActionError::from)
        })
        .await?;
        if !publication_token.is_publishable() {
            continue;
        }
        let Some(publication_lease) = publication_token.begin_publication() else {
            continue;
        };
        return Ok((canonical, publication_lease));
    }
}

fn note_card_action_note_id(action: &NoteCardAction) -> &NoteId {
    match action {
        NoteCardAction::Rename { note_id, .. }
        | NoteCardAction::Encrypt { note_id, .. }
        | NoteCardAction::UnlockEncrypted { note_id, .. }
        | NoteCardAction::RemoveEncryption { note_id, .. }
        | NoteCardAction::TogglePin(note_id)
        | NoteCardAction::ToggleArchive(note_id)
        | NoteCardAction::ToggleTrash(note_id)
        | NoteCardAction::Duplicate(note_id)
        | NoteCardAction::LockEncrypted(note_id)
        | NoteCardAction::DeletePermanently(note_id) => note_id,
        NoteCardAction::Move { note_id, .. } => note_id,
    }
}

pub(super) fn apply_note_card_action(
    services: ApplicationServices,
    action: NoteCardAction,
    state: NotesListState,
) {
    let notes = services.notes();
    let editor = services.editor();
    let vault = services.vault();
    let NotesListState {
        mut selected_note,
        mut vault_transition,
        mut status,
        editor_session,
        ..
    } = state;
    // Vault/note actions may intentionally remount the card/editor surface that
    // initiated them. Run the continuation in the stable `RunningApp` task scope:
    // it survives those child remounts, but is cancelled if the owning application
    // state itself is replaced. This keeps Dioxus Signal ownership sound.
    state.spawn(async move {
        let target_note_id = note_card_action_note_id(&action).clone();
        let profile_operation = vault_profile_operation(&action);
        let vault_profiled_action = profile_operation.is_some();
        let protecting_note = matches!(&action, NoteCardAction::Encrypt { .. });
        let unlocking_note = matches!(&action, NoteCardAction::UnlockEncrypted { .. });
        let locking_note = matches!(&action, NoteCardAction::LockEncrypted(_));
        let removing_encryption = matches!(&action, NoteCardAction::RemoveEncryption { .. });
        let vault_visibility_transition = matches!(
            &action,
            NoteCardAction::Encrypt { .. }
                | NoteCardAction::UnlockEncrypted { .. }
                | NoteCardAction::LockEncrypted(_)
                | NoteCardAction::RemoveEncryption { .. }
                | NoteCardAction::DeletePermanently(_)
        );
        let _vault_action_busy = if vault_visibility_transition {
            let Some(guard) = VaultActionBusyGuard::begin(state.vault_action_busy, &target_note_id) else {
                status.set("Another security operation is still finishing".to_string());
                return;
            };
            Some(guard)
        } else {
            None
        };
        if let Some(message) = vault_action_progress_message(&action) {
            status.set(message.to_string());
        }
        let needs_editor_sync = !matches!(
            &action,
            NoteCardAction::DeletePermanently(_) | NoteCardAction::UnlockEncrypted { .. }
        );
        let Some(pre_action_token) = state.capture_note_action_publication_token(&target_note_id) else {
            status.set("Note action blocked while privacy state is changing".to_string());
            return;
        };
        let _vault_profile_operation = profile_operation.map(VaultProfileSpan::operation);
        let mutation_synchronization = if needs_editor_sync {
            let _ui_checkpoint_profile = vault_profiled_action
                .then(|| VaultProfileSpan::stage(VaultProfileStage::UiCheckpoint));
            match synchronize_for_note_mutation(
                editor.clone(),
                selected_note,
                editor_session,
                state.note_selection_generation,
                target_note_id.clone(),
                pre_action_token.clone(),
            )
            .await
            {
                Ok(synchronized) => Some(synchronized),
                Err(error) => {
                    status.set(format!("Could not finish saving: {error}"));
                    return;
                }
            }
        } else {
            None
        };

        // For mutations that synchronized B, use the exact no-await selection
        // snapshot returned by that durability barrier. In particular, if an
        // action began on unselected B and B acquired a live editor while its
        // silent checkpoint was running, synchronization loops through the live
        // editor before this boundary. Unlock/Delete do not consume live B
        // plaintext and therefore need only the immediate boundary snapshot.
        let (revocation_selection_generation, selected_target) =
            if let Some(synchronized) = mutation_synchronization {
                (synchronized.selection_generation, synchronized.target_selected)
            } else {
                (
                    *state.note_selection_generation.peek(),
                    selected_note
                        .read()
                        .as_ref()
                        .is_some_and(|note| note.id == target_note_id),
                )
            };

        if selected_target && (protecting_note || locking_note || unlocking_note || removing_encryption) {
            let public_label = match &action {
                NoteCardAction::Encrypt { public_label, .. } => {
                    let label = public_label.trim();
                    if label.is_empty() {
                        "Encrypted note".to_string()
                    } else {
                        label.to_string()
                    }
                }
                // The selected unlocked note contains the private decrypted title,
                // not the user-approved public label. Never copy that private title
                // into transition UI that survives plaintext revocation.
                NoteCardAction::LockEncrypted(_) | NoteCardAction::UnlockEncrypted { .. } | NoteCardAction::RemoveEncryption { .. } => {
                    "Encrypted note".to_string()
                }
                _ => unreachable!("Vault transition shell is only used for Protect/Lock/Unlock/Remove Encryption"),
            };
            vault_transition.set(Some(super::VaultTransitionUiState {
                note_id: target_note_id.clone(),
                public_label,
                message: vault_action_progress_message(&action)
                    .unwrap_or("Securing note…")
                    .to_string(),
            }));
        }

        // Revoke B immediately at the UI boundary. A note revocation also
        // invalidates the multi-note generation because aggregate surfaces may
        // retain B-derived plaintext, while unrelated note A remains publishable.
        let privacy_revocation = vault_visibility_transition
            .then(|| state.privacy_epoch.begin_note_ui_revocation(target_note_id.clone()));

        if selected_target && vault_visibility_transition {
            // B was checkpointed before revocation. Remove its selected Note
            // object for the entire transition lifetime; the non-secret
            // VaultTransitionUiState above preserves logical selection for the
            // four Vault visibility actions without retaining decrypted state.
            // Aggregate caches are cleared centrally by the multi-note privacy
            // multi-note lifetime.
            selected_note.set(None);
        }

        let pin_filter_active = *state.section.read() == Section::Pinned
            || state.filters.read().pinned;
        let selected_notebook = state.selected_notebook.read().clone();
        // Retain a read capability for post-error reconciliation. Vault protection
        // can commit its canonical encryption transition before a later forensic
        // cleanup step reports a failure; the UI must then reflect the encrypted
        // state instead of continuing to display stale plaintext.
        let notes_after_action = notes.clone();
        let result = ApplicationExecutor::read("note action", move || {
            apply_note_card_action_blocking(
                notes,
                editor,
                vault,
                action,
                pin_filter_active,
                selected_notebook,
            )
        })
        .await;
        drop(privacy_revocation);
        match result {
            Ok((message, note_id, remove_immediately, updated_note)) => {
                if vault_visibility_transition {
                    // Storage has closed B's transition. Re-authorize B before
                    // rereading its canonical state; keep that B lease alive
                    // across any selected-editor handoff. Aggregate recovery is
                    // independently owned by the centralized multi-note lifetime.
                    let _canonical_publication_profile = vault_profiled_action
                        .then(|| VaultProfileSpan::stage(VaultProfileStage::CanonicalPublication));
                    match read_canonical_note_for_vault_publication(
                        notes_after_action.clone(),
                        state,
                        note_id.clone(),
                    )
                    .await
                    {
                        Ok((canonical, _note_lease)) => {
                            if let Some(current) = canonical.as_ref()
                                && should_republish_selected_vault_target(
                                    state,
                                    &note_id,
                                    selected_target,
                                    revocation_selection_generation,
                                )
                            {
                                republish_selected_note_after_external_mutation(
                                    state,
                                    current.clone(),
                                );
                                focus_selected_note_surface(&note_id);
                            }
                            status.set(message);
                        }
                        Err(error) => {
                            status.set(format!(
                                "{message}, but the canonical note could not be reloaded: {error}"
                            ));
                        }
                    }
                    // Dropping B's UI revocation above independently wakes the
                    // centralized multi-note lifetime, which repopulates
                    // list/tag aggregates under fresh multi-note authority.
                    clear_vault_transition_for_note(state, &note_id);
                    return;
                }

                // Non-Vault note mutation: note-local publication may update the
                // selected editor independently from aggregate list/cache refresh.
                if let Some(_note_lease) = pre_action_token.begin_publication()
                    && let Some(updated_note) = updated_note.as_ref()
                    && selected_note
                        .read()
                        .as_ref()
                        .is_some_and(|selected| selected.id == updated_note.id)
                {
                    republish_selected_note_after_external_mutation(
                        state,
                        updated_note.clone(),
                    );
                }

                if let Some(aggregate_token) = state.capture_multi_note_publication_token()
                    && let Some(_aggregate_lease) = aggregate_token.begin_publication()
                {
                    if remove_immediately {
                        remove_loaded_note(&note_id, state);
                    } else if let Some(updated_note) = updated_note {
                        update_loaded_note(&updated_note, state);
                    }
                    refresh_note_list(state);
                }
                status.set(message);
            }
            Err(error) => {
                // A Vault transition can commit canonical state before a later
                // cleanup error. Reconcile B under fresh B authority without
                // allowing the error path to resurrect B after navigation.
                if vault_visibility_transition {
                    let _canonical_publication_profile = vault_profiled_action
                        .then(|| VaultProfileSpan::stage(VaultProfileStage::CanonicalPublication));
                    if let Ok((canonical, _note_lease)) =
                        read_canonical_note_for_vault_publication(
                            notes_after_action.clone(),
                            state,
                            target_note_id.clone(),
                        )
                        .await
                        && let Some(current) = canonical.as_ref()
                        && should_republish_selected_vault_target(
                            state,
                            &target_note_id,
                            selected_target,
                            revocation_selection_generation,
                        )
                    {
                        republish_selected_note_after_external_mutation(
                            state,
                            current.clone(),
                        );
                    }
                }
                // Aggregate reconciliation is owned by the centralized multi-note
                // privacy lifetime triggered when B's revocation guard closes.
                clear_vault_transition_for_note(state, &target_note_id);
                status.set(note_action_error_message(
                    &error,
                    unlocking_note,
                    removing_encryption,
                ));
            }
        }
    });
}

pub(super) fn apply_note_card_tag_action(
    services: ApplicationServices,
    action: NoteCardTagAction,
    tags: Signal<Vec<String>>,
    state: NotesListState,
) {
    let notes = services.notes();
    let editor = services.editor();
    let library = services.library();
    let NotesListState {
        selected_note,
        mut filters,
        mut status,
        ..
    } = state;
    spawn(async move {
        let selection_generation = *state.note_selection_generation.peek();
        match state
            .synchronize_current_editor_when_privacy_ready(
                editor,
                SynchronizationMode::Checkpoint,
                selection_generation,
            )
            .await
        {
            Ok(Some(_)) => {}
            Ok(None) => return,
            Err(error) => {
                status.set(format!("Could not finish saving: {error}"));
                return;
            }
        }
        let Some(publication_token) = state
            .wait_for_multi_note_publication_token(Some(selection_generation))
            .await
        else {
            return;
        };

        let (note_id, command, success_message) = match &action {
            NoteCardTagAction::Rename { note_id, from, to } => (
                note_id.clone(),
                TagCommand::Rename {
                    from: from.clone(),
                    to: to.clone(),
                },
                "Tag renamed",
            ),
            NoteCardTagAction::Remove { note_id, name } => (
                note_id.clone(),
                TagCommand::RemoveFromNote { name: name.clone() },
                "Tag removed from note",
            ),
            NoteCardTagAction::Delete { note_id, name } => (
                note_id.clone(),
                TagCommand::Delete { name: name.clone() },
                "Tag deleted",
            ),
        };
        let selected_id = selected_note.read().as_ref().map(|note| note.id.clone());
        let worker_notes = notes.clone();
        let worker_selected_id = selected_id.clone();
        let result = ApplicationExecutor::read("tag action", move || {
            execute_tag_command_blocking(&worker_notes, &note_id, &command)?;
            let selected = worker_selected_id
                .as_ref()
                .map(|selected_id| worker_notes.get(selected_id))
                .transpose()
                .map_err(TagCommandError::from)?
                .flatten();
            Ok::<_, TagCommandError>(selected)
        })
        .await;

        match result {
            Ok(fresh_selected) => {
                let Some(_publication_lease) = publication_token.begin_publication() else {
                    return;
                };
                let mut next_filters = filters.read().clone();
                let mut filter_changed = false;
                match &action {
                    NoteCardTagAction::Rename { from, to, .. } => {
                        let canonical = to.trim();
                        if !canonical.is_empty() {
                            for tag in &mut next_filters.tags {
                                if tag.eq_ignore_ascii_case(from) && tag != canonical {
                                    *tag = canonical.to_string();
                                    filter_changed = true;
                                }
                            }
                        }
                    }
                    NoteCardTagAction::Delete { name, .. } => {
                        let before = next_filters.tags.len();
                        next_filters.tags.retain(|tag| !tag.eq_ignore_ascii_case(name));
                        filter_changed = next_filters.tags.len() != before;
                    }
                    NoteCardTagAction::Remove { .. } => {}
                }
                if filter_changed {
                    filters.set(next_filters.normalized());
                    super::request_note_list_preferences_save(state);
                }
                if let Some(fresh_selected) = fresh_selected
                    && selected_id.as_ref().is_some_and(|id| id == &fresh_selected.id)
                    && selected_note
                        .read()
                        .as_ref()
                        .is_some_and(|note| note.id == fresh_selected.id)
                {
                    republish_selected_note_after_external_mutation(
                        state,
                        fresh_selected,
                    );
                }
                if !super::refresh_tags(&library, tags, status) {
                    return;
                }
                reload_note_list(state);
                status.set(success_message.to_string());
            }
            Err(error) => status.set(format!("Could not update tag: {error}")),
        }
    });
}

fn clear_vault_transition_for_note(mut state: NotesListState, note_id: &NoteId) {
    if state
        .vault_transition
        .peek()
        .as_ref()
        .is_some_and(|transition| &transition.note_id == note_id)
    {
        state.vault_transition.set(None);
    }
}

fn republish_selected_note_after_external_mutation(
    mut state: NotesListState,
    note: Note,
) {
    // Every caller holds the authoritative PrivacyPublicationLease for this UI
    // handoff. Do not reacquire a nested lease here: remount identity is local
    // UI state and must not be allowed to invalidate publication authorization.
    state.advance_note_view_generation();
    let note_id = note.id.clone();
    state.selected_note.set(Some(note));
    clear_vault_transition_for_note(state, &note_id);
}

type NoteCardActionOutcome = (String, NoteId, bool, Option<Note>);

#[derive(Debug)]
enum NoteCardActionError {
    Data(DataAccessError),
    Editor(EditorError),
    NoteMissing,
    Execution(ApplicationExecutionError),
}

impl std::fmt::Display for NoteCardActionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Data(error) => std::fmt::Display::fmt(error, formatter),
            Self::Editor(error) => std::fmt::Display::fmt(error, formatter),
            Self::NoteMissing => formatter.write_str("The note no longer exists"),
            Self::Execution(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for NoteCardActionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Data(error) => Some(error),
            Self::Editor(error) => Some(error),
            Self::Execution(error) => Some(error),
            Self::NoteMissing => None,
        }
    }
}

impl NoteCardActionError {
    fn is_vault_authentication_failure(&self) -> bool {
        matches!(
            self,
            Self::Data(DataAccessError::Storage(StorageError::Vault(
                VaultError::AuthenticationFailed
            )))
        )
    }
}

fn note_action_error_message(
    error: &NoteCardActionError,
    unlocking_note: bool,
    removing_encryption: bool,
) -> String {
    if error.is_vault_authentication_failure() {
        if unlocking_note {
            return "Incorrect password. The note remains locked.".to_string();
        }
        if removing_encryption {
            return "Incorrect password. Encryption was not removed.".to_string();
        }
        return "Incorrect password.".to_string();
    }
    format!("Could not update note: {error}")
}

impl From<DataAccessError> for NoteCardActionError {
    fn from(error: DataAccessError) -> Self { Self::Data(error) }
}

impl From<EditorError> for NoteCardActionError {
    fn from(error: EditorError) -> Self { Self::Editor(error) }
}

impl ExecutionFailure for NoteCardActionError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self { Self::Execution(error) }
}

fn apply_note_card_action_blocking(
    notes: NoteService,
    editor: EditorService,
    vault: VaultProtectionService,
    action: NoteCardAction,
    pin_filter_active: bool,
    selected_notebook: Option<String>,
) -> Result<NoteCardActionOutcome, NoteCardActionError> {
    match action {
        NoteCardAction::Rename { note_id, title } => {
            let mut current = notes.get(&note_id)?.ok_or(NoteCardActionError::NoteMissing)?;
            current.title = title;
            current.updated_at = unix_timestamp();
            let current_id = current.id.clone();
            let updated = notes.checkpoint(current, NoteChangeScope::Title)?.0;
            Ok(("Note renamed".to_string(), current_id, false, Some(updated)))
        }
        NoteCardAction::Encrypt {
            note_id,
            password,
            public_label,
        } => {
            let outcome = vault.protect_note(&note_id, password.as_bytes(), &public_label)?;
            Ok((
                protect_note_status_message(outcome.pending_backup_privacy_resets),
                note_id,
                false,
                None,
            ))
        }
        NoteCardAction::UnlockEncrypted { note_id, password } => {
            vault.unlock_note(&note_id, password.as_bytes())?;
            Ok(("Note unlocked".to_string(), note_id, false, None))
        }
        NoteCardAction::LockEncrypted(note_id) => {
            vault.lock_note(&note_id)?;
            Ok(("Note locked".to_string(), note_id, false, None))
        }
        NoteCardAction::RemoveEncryption { note_id, password } => {
            vault.remove_note_protection(&note_id, password.as_bytes())?;
            Ok((
                "Note encryption removed".to_string(),
                note_id,
                false,
                None,
            ))
        }
        NoteCardAction::TogglePin(note_id) => {
            let mut current = notes.get(&note_id)?.ok_or(NoteCardActionError::NoteMissing)?;
            let next = !current.pinned;
            let remove_immediately = !next && pin_filter_active;
            current.pinned = next;
            current.updated_at = unix_timestamp();
            let current_id = current.id.clone();
            let updated = notes.checkpoint(current, NoteChangeScope::State)?.0;
            Ok((
                if next { "Note pinned" } else { "Note unpinned" }.to_string(),
                current_id,
                remove_immediately,
                Some(updated),
            ))
        }
        NoteCardAction::Move { note_id, notebook_id } => {
            let remove_immediately = selected_notebook
                .as_ref()
                .is_some_and(|selected| selected != &notebook_id);
            let mut current = notes.get(&note_id)?.ok_or(NoteCardActionError::NoteMissing)?;
            current.notebook_id = notebook_id;
            current.updated_at = unix_timestamp();
            let current_id = current.id.clone();
            let saved = notes.checkpoint(current, NoteChangeScope::State)?.0;
            Ok((
                "Note moved".to_string(),
                current_id,
                remove_immediately,
                Some(saved),
            ))
        }
        NoteCardAction::ToggleArchive(note_id) => {
            let mut current = notes.get(&note_id)?.ok_or(NoteCardActionError::NoteMissing)?;
            let next = !current.archived;
            current.archived = next;
            current.trashed = false;
            current.purge_after = None;
            current.updated_at = unix_timestamp();
            let current_id = current.id.clone();
            notes.checkpoint(current, NoteChangeScope::State)?;
            Ok((
                if next { "Note archived" } else { "Note restored from archive" }.to_string(),
                current_id,
                true,
                None,
            ))
        }
        NoteCardAction::ToggleTrash(note_id) => {
            let mut current = notes.get(&note_id)?.ok_or(NoteCardActionError::NoteMissing)?;
            let trashed = !current.trashed;
            current.archived = false;
            current.trashed = trashed;
            current.purge_after = if trashed {
                notes.trash_deletion_deadline()?
            } else {
                None
            };
            current.updated_at = unix_timestamp();
            let current_id = current.id.clone();
            notes.checkpoint(current, NoteChangeScope::State)?;
            Ok((
                if trashed { "Note moved to trash" } else { "Note recovered from trash" }
                    .to_string(),
                current_id,
                true,
                None,
            ))
        }
        NoteCardAction::Duplicate(note_id) => {
            let duplicate = notes.duplicate(&note_id)?;
            Ok(("Note duplicated".to_string(), duplicate.id, false, None))
        }
        NoteCardAction::DeletePermanently(note_id) => {
            editor.discard_pending_note(&note_id)?;
            notes.permanently_delete_trashed(&note_id)?;
            Ok(("Note permanently deleted".to_string(), note_id, true, None))
        }
    }
}

fn protect_note_status_message(pending_backup_privacy_resets: Option<usize>) -> String {
    match pending_backup_privacy_resets {
        Some(0) => "Note encrypted and unlocked for this session".to_string(),
        Some(1) => "Note encrypted locally and unlocked for this session. 1 backup location still requires privacy cleanup and may contain an earlier plaintext copy; Rustnote will clean it before using that repository again.".to_string(),
        Some(count) => format!(
            "Note encrypted locally and unlocked for this session. {count} backup locations still require privacy cleanup and may contain earlier plaintext copies; Rustnote will clean them before using those repositories again."
        ),
        None => "Note encrypted locally and unlocked for this session. Backup privacy status could not be refreshed; check Settings > Backups before assuming older backup copies are clean.".to_string(),
    }
}

pub(super) fn attachment_summary(names: &[String]) -> String {
    match names {
        [] => "No attachments".to_string(),
        [name] => format!("Attachment: {name}"),
        _ => {
            let shown = names.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            let remaining = names.len().saturating_sub(3);
            if remaining == 0 {
                format!("Attachments: {shown}")
            } else {
                format!("Attachments: {shown}, and {remaining} more")
            }
        }
    }
}

pub(super) fn compact_relative_time(timestamp: i64) -> String {
    let seconds = unix_timestamp().saturating_sub(timestamp);
    match seconds {
        0..=59 => "Now".to_string(),
        60..=3_599 => format!("{}m", seconds / 60),
        3_600..=86_399 => format!("{}h", seconds / 3_600),
        86_400..=604_799 => format!("{}d", seconds / 86_400),
        _ => format!("{}w", seconds / 604_800),
    }
}

pub(super) fn relative_time(timestamp: i64) -> String {
    let seconds = unix_timestamp().saturating_sub(timestamp);
    match seconds {
        0..=59 => "Just now".to_string(),
        60..=3_599 => format!("{}m ago", seconds / 60),
        3_600..=86_399 => format!("{}h ago", seconds / 3_600),
        86_400..=604_799 => format!("{}d ago", seconds / 86_400),
        _ => format!("{}w ago", seconds / 604_800),
    }
}

#[cfg(test)]
mod vault_privacy_message_tests {
    use super::protect_note_status_message;

    #[test]
    fn protect_success_discloses_pending_backup_plaintext_risk() {
        let message = protect_note_status_message(Some(1));
        assert!(message.contains("encrypted locally"));
        assert!(message.contains("earlier plaintext copy"));
        assert!(message.contains("privacy cleanup"));
    }

    #[test]
    fn protect_success_is_conservative_when_backup_status_refresh_fails() {
        let message = protect_note_status_message(None);
        assert!(message.contains("Backup privacy status could not be refreshed"));
        assert!(message.contains("Settings > Backups"));
    }
}
