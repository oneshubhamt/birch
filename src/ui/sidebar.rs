use std::collections::HashSet;

use dioxus::prelude::*;
use crate::application::{
    context::ApplicationServices, editor::EditorService, executor::ApplicationExecutor,
};

use crate::build_info::FULL_BUILD_IDENTIFIER;
use crate::domain::{DEFAULT_NOTEBOOK_ID, Notebook, Section};

use super::entity_actions::{ActionTarget, EntityAction};
use super::note_list_data::reload_note_list;
use super::notes_header::create_contextual_note;
use super::responsive::{focus_notebook_tree_item, NotebookTreeNavigation};
use super::synchronization::SynchronizationMode;
use super::task_error::DataTaskError;
use super::{CompactPane, NotesListState};

#[derive(Clone, Debug, PartialEq, Eq)]
enum NotebookCommand {
    Rename { notebook_id: String, name: String },
    Move { notebook_id: String, parent_id: Option<String> },
    Delete { notebook_id: String },
}

#[derive(Clone, PartialEq)]
struct NotebookTreeActions {
    onselect: EventHandler<String>,
    oncommand: EventHandler<NotebookCommand>,
    oncreatechild: EventHandler<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct SidebarView {
    pub services: Signal<ApplicationServices>,
    pub library_state: NotesListState,
    pub notebooks: Signal<Vec<Notebook>>,
    pub compact_pane: Signal<CompactPane>,
    pub sidebar_overlay_open: Signal<bool>,
    pub notebook_name: Signal<String>,
    pub show_notebook_form: Signal<bool>,
    pub show_settings: Signal<bool>,
    pub current_section: Section,
}

#[component]
pub(super) fn Sidebar(view: SidebarView) -> Element {
    let SidebarView {
        services,
        library_state,
        mut notebooks,
        mut compact_pane,
        mut sidebar_overlay_open,
        mut notebook_name,
        mut show_notebook_form,
        mut show_settings,
        current_section,
    } = view;
    let NotesListState {
        selected_notebook,
        mut status,
        ..
    } = library_state;
    let creation_notebook_name = selected_notebook
        .read()
        .as_ref()
        .map(|id| notebook_name_for(&notebooks.read(), id))
        .unwrap_or_else(|| notebook_name_for(&notebooks.read(), DEFAULT_NOTEBOOK_ID));
    let notebook_root_active =
        current_section == Section::Notes && selected_notebook.read().is_none();
    let notebook_add_label = if selected_notebook.read().is_some() {
        "Create a nested notebook"
    } else {
        "Create a top-level notebook"
    };
    let mut notebook_create_busy = use_signal(|| false);
    let mut notebook_create_parent = use_signal(|| None::<String>);

    rsx! {
        aside { id: "library-sidebar", class: "sidebar", aria_label: "Notebooks and library navigation",
            div { class: "brand-row",
                div { class: "brand-mark", "R" }
                div { class: "brand-copy",
                    h1 { "Rustnote" }
                    p { "Local-first notes" }
                }
                button {
                    class: "sidebar-close-button",
                    title: "Close notebooks",
                    aria_label: "Close notebooks panel",
                    onclick: move |_| sidebar_overlay_open.set(false),
                    "×"
                }
            }

            button {
                class: "new-note-button",
                title: "Create a note in {creation_notebook_name}",
                onclick: {
                    move |_| {
                        create_contextual_note(services.read().clone(), library_state);
                        compact_pane.set(CompactPane::Editor);
                        sidebar_overlay_open.set(false);
                    }
                },
                span { class: "button-symbol", "+" }
                span { "New note" }
                kbd { "⌘N" }
            }

            nav { class: "primary-nav", aria_label: "Library sections",
                SidebarButton {
                    label: "All notes",
                    symbol: "▤",
                    active: current_section == Section::Notes && selected_notebook.read().is_none(),
                    onclick: {
                        move |_| schedule_sidebar_navigation(
                            services.read().editor(),
                            library_state,
                            Section::Notes,
                            None,
                            sidebar_overlay_open,
                            compact_pane,
                        )
                    }
                }
                SidebarButton {
                    label: "Pinned",
                    symbol: "◆",
                    active: current_section == Section::Pinned,
                    onclick: {
                        move |_| schedule_sidebar_navigation(
                            services.read().editor(),
                            library_state,
                            Section::Pinned,
                            None,
                            sidebar_overlay_open,
                            compact_pane,
                        )
                    }
                }
                SidebarButton {
                    label: "Archive",
                    symbol: "▣",
                    active: current_section == Section::Archive,
                    onclick: {
                        move |_| schedule_sidebar_navigation(
                            services.read().editor(),
                            library_state,
                            Section::Archive,
                            None,
                            sidebar_overlay_open,
                            compact_pane,
                        )
                    }
                }
                SidebarButton {
                    label: "Trash",
                    symbol: "◇",
                    active: current_section == Section::Trash,
                    onclick: {
                        move |_| schedule_sidebar_navigation(
                            services.read().editor(),
                            library_state,
                            Section::Trash,
                            None,
                            sidebar_overlay_open,
                            compact_pane,
                        )
                    }
                }
            }

            div { class: "sidebar-heading",
                button {
                    class: if notebook_root_active {
                        "notebook-root-button active"
                    } else {
                        "notebook-root-button"
                    },
                    aria_current: if notebook_root_active { "page" } else { "false" },
                    aria_label: "Show all notebooks and create at the top level",
                    onclick: {
                        move |_| schedule_sidebar_navigation(
                            services.read().editor(),
                            library_state,
                            Section::Notes,
                            None,
                            sidebar_overlay_open,
                            compact_pane,
                        )
                    },
                    "NOTEBOOKS"
                }
                button {
                    class: "icon-button small",
                    "data-tooltip": "{notebook_add_label}",
                    aria_label: "{notebook_add_label}",
                    onclick: move |_| {
                        let next = !*show_notebook_form.read();
                        if next {
                            notebook_create_parent.set(selected_notebook.read().clone());
                            notebook_name.set(String::new());
                        }
                        show_notebook_form.set(next);
                    },
                    "+"
                }
            }

            if *show_notebook_form.read() {
                div { class: "notebook-form",
                    input {
                        id: "notebook-create-input",
                        value: "{notebook_name}",
                        placeholder: if notebook_create_parent.read().is_some() {
                            "New nested notebook"
                        } else {
                            "New top-level notebook"
                        },
                        oninput: move |event| notebook_name.set(event.value()),
                        disabled: *notebook_create_busy.read(),
                        aria_busy: *notebook_create_busy.read(),
                        onkeydown: {
                            move |event| {
                                if event.key() != Key::Enter || *notebook_create_busy.read() {
                                    return;
                                }
                                let name = notebook_name.read().clone();
                                let parent = notebook_create_parent.read().clone();
                                let task_library = services.read().library();
                                notebook_create_busy.set(true);
                                spawn(async move {
                                    let result = ApplicationExecutor::read("notebook creation", move || {
                                        task_library
                                            .create_notebook(&name, parent.as_deref())
                                            .map_err(super::task_error::DataTaskError::from)?;
                                        task_library
                                            .list_notebooks()
                                            .map_err(super::task_error::DataTaskError::from)
                                    })
                                    .await;
                                    notebook_create_busy.set(false);
                                    match result {
                                        Ok(next) => {
                                            notebooks.set(next);
                                            notebook_name.set(String::new());
                                            show_notebook_form.set(false);
                                            notebook_create_parent.set(None);
                                            status.set("Notebook created".to_string());
                                        }
                                        Err(error) => status.set(format!(
                                            "Could not create notebook: {error}"
                                        )),
                                    }
                                });
                            }
                        }
                    }
                }
            }

            div {
                id: "notebook-tree-scroll",
                class: "notebook-tree",
                role: "tree",
                tabindex: "0",
                aria_label: "Notebooks",
                onkeydown: move |event: KeyboardEvent| {
                    if event.is_composing() {
                        return;
                    }
                    let navigation = match event.key() {
                        Key::ArrowDown => Some(NotebookTreeNavigation::Next),
                        Key::ArrowUp => Some(NotebookTreeNavigation::Previous),
                        Key::Home => Some(NotebookTreeNavigation::First),
                        Key::End => Some(NotebookTreeNavigation::Last),
                        _ => None,
                    };
                    if let Some(navigation) = navigation {
                        event.prevent_default();
                        event.stop_propagation();
                        focus_notebook_tree_item(navigation);
                    }
                },
                NotebookTree {
                    notebooks: notebooks.read().clone(),
                    parent_id: None,
                    selected_id: selected_notebook.read().clone(),
                    actions: NotebookTreeActions {
                        onselect: EventHandler::new({
                            move |id: String| schedule_sidebar_navigation(
                                services.read().editor(),
                                library_state,
                                Section::Notes,
                                Some(id),
                                sidebar_overlay_open,
                                compact_pane,
                            )
                        }),
                        oncommand: EventHandler::new({
                            move |command: NotebookCommand| apply_notebook_command(
                                services.read().clone(),
                                command,
                                library_state,
                                notebooks,
                            )
                        }),
                        oncreatechild: EventHandler::new(move |parent_id: String| {
                            notebook_create_parent.set(Some(parent_id));
                            notebook_name.set(String::new());
                            show_notebook_form.set(true);
                            focus_notebook_create_input();
                        }),
                    },
                }
            }

            div { class: "sidebar-footer compact",
                button {
                    id: "settings-trigger",
                    class: "settings-entry",
                    title: "Open Rustnote settings",
                    aria_label: "Open Rustnote settings",
                    aria_controls: "settings-dialog",
                    aria_haspopup: "dialog",
                    aria_expanded: "{*show_settings.read()}",
                    "data-transient-trigger": "true",
                    onclick: move |_| {
                        sidebar_overlay_open.set(false);
                        show_settings.set(true);
                    },
                    span { class: "settings-entry-icon", aria_hidden: "true", "⚙" }
                    span { class: "settings-entry-copy",
                        strong { "Settings" }
                        small { "Appearance, backups, history, storage" }
                    }
                }
                span { class: "sidebar-version", "Rustnote v{FULL_BUILD_IDENTIFIER}" }
            }
        }
    }
}

fn schedule_sidebar_navigation(
    editor: EditorService,
    state: NotesListState,
    next_section: Section,
    notebook_id: Option<String>,
    mut sidebar_overlay_open: Signal<bool>,
    mut compact_pane: Signal<CompactPane>,
) {
    let NotesListState {
        mut section,
        mut selected_notebook,
        mut status,
        ..
    } = state;
    let selection_generation = state.advance_note_selection_generation();
    state.spawn(async move {
        let _publication_lease = match state
            .synchronize_then_begin_multi_note_publication_when_privacy_ready(
                editor,
                SynchronizationMode::Checkpoint,
                selection_generation,
            )
            .await
        {
            Ok(Some(lease)) => lease,
            Ok(None) => return,
            Err(error) => {
                status.set(format!("Could not finish saving: {error}"));
                return;
            }
        };
        section.set(next_section);
        selected_notebook.set(notebook_id.clone());
        reload_note_list(state);
        sidebar_overlay_open.set(false);
        compact_pane.set(CompactPane::Notes);
    });
}

fn focus_notebook_create_input() {
    spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let _ = document::eval(
            "const input = document.getElementById('notebook-create-input'); if (input) { input.focus(); input.select(); return true; } return false;",
        )
        .join::<bool>()
        .await;
    });
}

fn focus_and_select_element(element_id: String) {
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let script = format!(
            "const input = document.getElementById({element_id:?}); if (input) {{ input.focus(); input.select(); return true; }} return false;"
        );
        let _ = document::eval(&script).join::<bool>().await;
    });
}

fn apply_notebook_command(
    services: ApplicationServices,
    command: NotebookCommand,
    state: NotesListState,
    mut notebooks: Signal<Vec<Notebook>>,
) {
    let editor = services.editor();
    let library = services.library();
    let notes = services.notes();
    let NotesListState {
        mut selected_notebook,
        mut selected_note,
        note_selection_generation,
        mut status,
        ..
    } = state;

    spawn(async move {
        let deleting_notebook_id = match &command {
            NotebookCommand::Delete { notebook_id } => Some(notebook_id.clone()),
            NotebookCommand::Rename { .. } | NotebookCommand::Move { .. } => None,
        };
        let selected_before = selected_note.read().clone();
        let remount_selected_note = deleting_notebook_id.as_ref().is_some_and(|notebook_id| {
            selected_before
                .as_ref()
                .is_some_and(|note| &note.notebook_id == notebook_id)
        });
        let mut selected_after_sync = selected_before.clone();
        let remount_generation = if remount_selected_note {
            let selection_generation = *note_selection_generation.peek();
            match state
                .synchronize_current_editor_when_privacy_ready(
                    editor.clone(),
                    SynchronizationMode::Checkpoint,
                    selection_generation,
                )
                .await
            {
                Ok(Some(_source_token)) => {}
                Ok(None) => return,
                Err(error) => {
                    status.set(format!("Could not finish saving: {error}"));
                    return;
                }
            }
            selected_after_sync = selected_note.read().clone();
            Some(selection_generation)
        } else {
            None
        };

        let Some(publication_token) = state
            .wait_for_multi_note_publication_token(remount_generation)
            .await
        else {
            return;
        };
        let Some(_publication_lease) = publication_token.begin_publication() else {
            return;
        };
        let selected_note_id = selected_after_sync.as_ref().map(|note| note.id.clone());
        let deleted_id_for_ui = deleting_notebook_id.clone();
        let refresh_selected_note = remount_selected_note;
        let task_library = library.clone();
        let task_notes = notes.clone();

        let result = ApplicationExecutor::read("notebook action", move || {
            let (message, replacement_notebook) = match command {
                NotebookCommand::Rename { notebook_id, name } => {
                    task_library
                        .rename_notebook(&notebook_id, &name)
                        .map_err(DataTaskError::from)?;
                    ("Notebook renamed".to_string(), None)
                }
                NotebookCommand::Move {
                    notebook_id,
                    parent_id,
                } => {
                    task_library
                        .move_notebook(&notebook_id, parent_id.as_deref())
                        .map_err(DataTaskError::from)?;
                    ("Notebook moved".to_string(), None)
                }
                NotebookCommand::Delete { notebook_id } => {
                    let replacement = task_library
                        .delete_notebook_preserving_contents(&notebook_id)
                        .map_err(DataTaskError::from)?;
                    ("Notebook deleted; contents preserved".to_string(), Some(replacement))
                }
            };

            let refreshed_selected_note = if replacement_notebook.is_some() && refresh_selected_note {
                match selected_note_id.as_ref() {
                    Some(note_id) => task_notes
                        .get(note_id)
                        .map_err(DataTaskError::from)?,
                    None => None,
                }
            } else {
                None
            };
            let next_notebooks = task_library
                .list_notebooks()
                .map_err(DataTaskError::from)?;
            Ok::<_, DataTaskError>((
                message,
                replacement_notebook,
                next_notebooks,
                refreshed_selected_note,
            ))
        })
        .await;

        match result {
            Ok((message, replacement_notebook, next_notebooks, refreshed_selected_note)) => {
                notebooks.set(next_notebooks);
                if let (Some(deleted_id), Some(replacement)) =
                    (deleted_id_for_ui.as_ref(), replacement_notebook.as_ref())
                    && selected_notebook.read().as_ref() == Some(deleted_id)
                {
                    selected_notebook.set(Some(replacement.clone()));
                }
                if let Some(selection_generation) = remount_generation
                    && *note_selection_generation.peek() == selection_generation
                {
                    state.advance_note_view_generation();
                    selected_note.set(refreshed_selected_note);
                }
                if replacement_notebook.is_some() {
                    reload_note_list(state);
                }
                status.set(message);
            }
            Err(error) => {
                if let Some(selection_generation) = remount_generation
                    && *note_selection_generation.peek() == selection_generation
                {
                    selected_note.set(selected_after_sync);
                }
                status.set(format!("Could not update notebook: {error}"));
            }
        }
    });
}

#[component]
pub(super) fn SidebarButton(
    label: &'static str,
    symbol: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: if active { "sidebar-link active" } else { "sidebar-link" },
            aria_current: if active { "page" } else { "false" },
            onclick: move |event| onclick.call(event),
            span { class: "nav-symbol", "{symbol}" }
            span { "{label}" }
        }
    }
}

#[component]
fn NotebookTree(
    notebooks: Vec<Notebook>,
    parent_id: Option<String>,
    selected_id: Option<String>,
    actions: NotebookTreeActions,
) -> Element {
    let children = notebooks
        .iter()
        .filter(|notebook| notebook.parent_id == parent_id)
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        for notebook in children {
            NotebookTreeNode {
                key: "{notebook.id}",
                notebook,
                notebooks: notebooks.clone(),
                selected_id: selected_id.clone(),
                actions: actions.clone(),
            }
        }
    }
}

#[component]
fn NotebookTreeNode(
    notebook: Notebook,
    notebooks: Vec<Notebook>,
    selected_id: Option<String>,
    actions: NotebookTreeActions,
) -> Element {
    use super::context_menu::{
        ContextMenuController, EntityContextMenu, EntityContextMenuActions, EntityContextMenuView,
    };
    use super::entity_actions::{
        EntityActionContext, EntityActionRegistry, EntityActionState, EntityActionTargets,
        EntitySurface,
    };

    let NotebookTreeActions {
        onselect,
        oncommand,
        oncreatechild,
    } = actions;
    let mut context_menu = use_signal(ContextMenuController::default);
    let mut renaming = use_signal(|| false);
    let mut rename_value = use_signal(|| notebook.name.clone());
    let active = selected_id.as_ref() == Some(&notebook.id);
    let row_id = format!("notebook-row-{}", notebook.id);
    let trigger_id = format!("notebook-menu-trigger-{}", notebook.id);
    let menu_id = format!("notebook-menu-{}", notebook.id);
    let rename_input_id = format!("notebook-rename-{}", notebook.id);
    let action_context = EntityActionContext {
        surface: EntitySurface::Notebook,
        entity_id: notebook.id.clone(),
        state: EntityActionState {
            protected: notebook.id == DEFAULT_NOTEBOOK_ID,
            ..EntityActionState::default()
        },
    };
    let action_targets = EntityActionTargets {
        move_targets: valid_notebook_move_targets(&notebook, &notebooks),
    };
    let menu_entries = EntityActionRegistry::entries(&action_context, &action_targets);

    rsx! {
        div { class: "notebook-node",
            div { class: "notebook-row-shell",
                if *renaming.read() {
                    input {
                        id: "{rename_input_id}",
                        class: "notebook-rename-input",
                        value: "{rename_value}",
                        aria_label: "Rename notebook",
                        autofocus: true,
                        oninput: move |event| rename_value.set(event.value()),
                        onclick: move |event| event.stop_propagation(),
                        oncontextmenu: move |event| event.stop_propagation(),
                        onkeydown: {
                            let original_name = notebook.name.clone();
                            let notebook_id = notebook.id.clone();
                            move |event: KeyboardEvent| match event.key() {
                                Key::Enter => {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    let name = rename_value.read().clone();
                                    renaming.set(false);
                                    if name.trim() != original_name {
                                        oncommand.call(NotebookCommand::Rename {
                                            notebook_id: notebook_id.clone(),
                                            name,
                                        });
                                    }
                                }
                                Key::Escape => {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    rename_value.set(original_name.clone());
                                    renaming.set(false);
                                }
                                _ => {}
                            }
                        },
                        onblur: {
                            let original_name = notebook.name.clone();
                            let notebook_id = notebook.id.clone();
                            move |_| {
                                if !*renaming.peek() {
                                    return;
                                }
                                let name = rename_value.read().clone();
                                renaming.set(false);
                                if name.trim() != original_name {
                                    oncommand.call(NotebookCommand::Rename {
                                        notebook_id: notebook_id.clone(),
                                        name,
                                    });
                                }
                            }
                        },
                    }
                } else {
                    button {
                        id: "{row_id}",
                        role: "treeitem",
                        tabindex: "-1",
                        aria_selected: if active { "true" } else { "false" },
                        class: if active { "notebook-link active" } else { "notebook-link" },
                        "data-context-long-press": "true",
                        "aria-keyshortcuts": "Shift+F10",
                        onclick: {
                            let id = notebook.id.clone();
                            move |_| onselect.call(id.clone())
                        },
                        oncontextmenu: {
                            let row_id = row_id.clone();
                            move |event: MouseEvent| {
                                event.prevent_default();
                                event.stop_propagation();
                                let coordinates = event.client_coordinates();
                                let x = coordinates.x.round() as i32;
                                let y = coordinates.y.round() as i32;
                                if x == 0 && y == 0 {
                                    context_menu.write().open_anchor(row_id.clone());
                                } else {
                                    context_menu.write().open_pointer(x, y);
                                }
                            }
                        },
                        onkeydown: {
                            let row_id = row_id.clone();
                            move |event: KeyboardEvent| {
                                if !event.is_composing()
                                    && event.key() == Key::F10
                                    && event.modifiers().shift()
                                {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    context_menu.write().open_anchor(row_id.clone());
                                }
                            }
                        },
                        span { class: "folder-symbol", "▱" }
                        span { class: "notebook-label", "{notebook.name}" }
                    }
                }

                button {
                    id: "{trigger_id}",
                    class: if context_menu.read().is_open() {
                        "entity-action-trigger notebook-action-trigger active"
                    } else {
                        "entity-action-trigger notebook-action-trigger"
                    },
                    title: "Notebook actions",
                    aria_label: "Open actions for {notebook.name}",
                    aria_controls: "{menu_id}",
                    aria_haspopup: "menu",
                    aria_expanded: "{context_menu.read().is_open()}",
                    disabled: *renaming.read(),
                    "data-transient-trigger": "true",
                    oncontextmenu: {
                        let trigger_id = trigger_id.clone();
                        move |event: MouseEvent| {
                            event.prevent_default();
                            event.stop_propagation();
                            let coordinates = event.client_coordinates();
                            let x = coordinates.x.round() as i32;
                            let y = coordinates.y.round() as i32;
                            if x == 0 && y == 0 {
                                context_menu.write().open_anchor(trigger_id.clone());
                            } else {
                                context_menu.write().open_pointer(x, y);
                            }
                        }
                    },
                    onclick: {
                        let trigger_id = trigger_id.clone();
                        move |event| {
                            event.stop_propagation();
                            if context_menu.read().is_open() {
                                context_menu.write().close();
                            } else {
                                context_menu.write().open_anchor(trigger_id.clone());
                            }
                        }
                    },
                    "•••"
                }

                if context_menu.read().is_open() {
                    EntityContextMenu {
                        view: EntityContextMenuView {
                            id: menu_id.clone(),
                            label: "Notebook actions".to_string(),
                            return_focus_id: row_id.clone(),
                            fallback_focus_id: "notebook-tree-scroll".to_string(),
                            controller: context_menu,
                            entries: menu_entries,
                        },
                        actions: EntityContextMenuActions {
                            onaction: EventHandler::new({
                                let notebook_id = notebook.id.clone();
                                let notebook_name = notebook.name.clone();
                                let rename_input_id = rename_input_id.clone();
                                move |action: EntityAction| match action {
                                    EntityAction::Rename => {
                                        rename_value.set(notebook_name.clone());
                                        renaming.set(true);
                                        focus_and_select_element(rename_input_id.clone());
                                    }
                                    EntityAction::CreateChildNotebook => {
                                        oncreatechild.call(notebook_id.clone())
                                    }
                                    EntityAction::MoveNotebookTo(parent_id) => {
                                        oncommand.call(NotebookCommand::Move {
                                            notebook_id: notebook_id.clone(),
                                            parent_id,
                                        })
                                    }
                                    EntityAction::DeleteNotebook => {
                                        oncommand.call(NotebookCommand::Delete {
                                            notebook_id: notebook_id.clone(),
                                        })
                                    }
                                    EntityAction::Open
                                    | EntityAction::EncryptNote
                                    | EntityAction::UnlockNote
                                    | EntityAction::LockNote
                                    | EntityAction::RemoveNoteEncryption
                                    | EntityAction::TogglePin
                                    | EntityAction::MoveNoteTo(_)
                                    | EntityAction::ToggleArchive
                                    | EntityAction::MoveToTrash
                                    | EntityAction::RestoreFromTrash
                                    | EntityAction::DeletePermanently
                                    | EntityAction::EditTags
                                    | EntityAction::DuplicateNote
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
            }

            div { class: "notebook-children",
                NotebookTree {
                    notebooks: notebooks.clone(),
                    parent_id: Some(notebook.id.clone()),
                    selected_id: selected_id.clone(),
                    actions: NotebookTreeActions {
                        onselect,
                        oncommand,
                        oncreatechild,
                    },
                }
            }
        }
    }
}

fn valid_notebook_move_targets(notebook: &Notebook, notebooks: &[Notebook]) -> Vec<ActionTarget> {
    let descendants = notebook_descendant_ids(&notebook.id, notebooks);
    let mut targets = Vec::new();
    if notebook.parent_id.is_some() {
        targets.push(ActionTarget::root("Top level"));
    }
    targets.extend(
        notebooks
            .iter()
            .filter(|candidate| candidate.id != notebook.id)
            .filter(|candidate| !descendants.contains(&candidate.id))
            .filter(|candidate| notebook.parent_id.as_ref() != Some(&candidate.id))
            .map(|candidate| {
                ActionTarget::entity(
                    candidate.id.clone(),
                    indented_notebook_name(candidate, notebooks),
                )
            }),
    );
    targets
}

fn notebook_descendant_ids(notebook_id: &str, notebooks: &[Notebook]) -> HashSet<String> {
    let mut descendants = HashSet::new();
    let mut pending = vec![notebook_id.to_string()];
    while let Some(parent_id) = pending.pop() {
        for child in notebooks
            .iter()
            .filter(|candidate| candidate.parent_id.as_deref() == Some(parent_id.as_str()))
        {
            if descendants.insert(child.id.clone()) {
                pending.push(child.id.clone());
            }
        }
    }
    descendants
}

pub(super) fn notebook_name_for(notebooks: &[Notebook], id: &str) -> String {
    notebooks
        .iter()
        .find(|notebook| notebook.id == id)
        .map(|notebook| notebook.name.clone())
        .unwrap_or_else(|| "Notebook".to_string())
}

pub(super) fn indented_notebook_name(notebook: &Notebook, notebooks: &[Notebook]) -> String {
    let mut depth = 0;
    let mut parent = notebook.parent_id.as_ref();
    while let Some(parent_id) = parent {
        depth += 1;
        parent = notebooks
            .iter()
            .find(|candidate| &candidate.id == parent_id)
            .and_then(|candidate| candidate.parent_id.as_ref());
        if depth > 8 {
            break;
        }
    }
    format!("{}{}", "— ".repeat(depth), notebook.name)
}
