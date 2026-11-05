use std::time::Duration;

use dioxus::prelude::*;
use crate::application::{
    context::ApplicationServices, editor::EditorService, executor::ApplicationExecutor,
    library::LibraryService,
};

use crate::domain::{DEFAULT_NOTEBOOK_ID, Note, NoteFilters, NoteId, Notebook, Section};

use super::filter_popover::{
    ActiveFilterRow, ActiveFilterRowView, FilterPopover, FilterPopoverView, apply_note_filters,
};
use super::note_list_data::{
    NOTE_CARD_ROW_PX, NoteNavigation, load_note_page_for_viewport, navigate_to_note_index,
    navigation_target, reload_note_list, select_note, virtual_note_window, virtual_scroll_anchor,
};
use super::note_card::{
    NoteCard, NoteCardActions, NoteCardView, apply_note_card_action, apply_note_card_tag_action,
};
use super::responsive::{
    focus_editor_panel, focus_selected_note_surface, focus_title_input,
};
use super::synchronization::SynchronizationMode;
use super::search_help::SearchHelpPopover;
use super::task_error::DataTaskError;
use super::sidebar::notebook_name_for;
use super::{
    CompactPane, LIBRARY_SEARCH_DEBOUNCE_MS, ListPopover, NoteCardAction, NoteCardTagAction,
    NoteSort, NotesListState, request_note_list_preferences_save,
};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct NotesListPanelView {
    pub services: Signal<ApplicationServices>,
    pub library_state: NotesListState,
    pub notebooks: Signal<Vec<Notebook>>,
    pub tags: Signal<Vec<String>>,
    pub sidebar_overlay_open: Signal<bool>,
    pub compact_pane: Signal<CompactPane>,
    pub list_popover: Signal<Option<ListPopover>>,
    pub tag_filter_query: Signal<String>,
    pub show_shortcuts: Signal<bool>,
}

#[component]
pub(super) fn NotesListPanel(view: NotesListPanelView) -> Element {
    let NotesListPanelView {
        services,
        library_state,
        notebooks,
        tags,
        mut sidebar_overlay_open,
        mut compact_pane,
        mut list_popover,
        mut tag_filter_query,
        mut show_shortcuts,
    } = view;
    let NotesListState {
        section,
        selected_notebook,
        mut search,
        mut search_generation,
        sort_order,
        filters,
        note_cache,
        note_count,
        list_loading,
        mut list_scroll_top,
        mut list_viewport_height,
        selected_note,
        vault_transition,
        mut status,
        ..
    } = library_state;
    let (_, _, aggregate_revocation_active) =
        library_state.privacy_epoch.observe_multi_note();
    let active_filter_count = filters.read().active_count();
    let total_note_count = *note_count.read();
    let cache_snapshot = note_cache.read();
    let virtual_window = virtual_note_window(
        total_note_count,
        cache_snapshot.start_index(),
        cache_snapshot.len(),
        *list_scroll_top.read(),
        *list_viewport_height.read(),
    );
    let visible_notes = if aggregate_revocation_active {
        Vec::new()
    } else {
        cache_snapshot.cloned_local_range(virtual_window.start, virtual_window.end)
    };
    drop(cache_snapshot);
    let query_is_constrained = !search.read().trim().is_empty() || !filters.read().is_empty();
    let top_spacer_style = format!("height: {}px", virtual_window.top_spacer_px);
    let bottom_spacer_style = format!("height: {}px", virtual_window.bottom_spacer_px);
    let available_tags = if aggregate_revocation_active {
        Vec::new()
    } else {
        tags.read().clone()
    };
    let normalized_tag_query = tag_filter_query.read().trim().to_lowercase();
    let selected_filter_tags = filters.read().tags.clone();
    let visible_filter_tags = available_tags
        .iter()
        .filter(|tag| {
            normalized_tag_query.is_empty() || tag.to_lowercase().contains(&normalized_tag_query)
        })
        .map(|tag| {
            let selected = selected_filter_tags
                .iter()
                .any(|value| value.eq_ignore_ascii_case(tag));
            (tag.to_string(), selected)
        })
        .collect::<Vec<_>>();
    let selected_id = selected_note
        .read()
        .as_ref()
        .map(|note| note.id.clone())
        .or_else(|| vault_transition.read().as_ref().map(|transition| transition.note_id.clone()));
    let creation_notebook_name = selected_notebook
        .read()
        .as_ref()
        .map(|id| notebook_name_for(&notebooks.read(), id))
        .unwrap_or_else(|| notebook_name_for(&notebooks.read(), DEFAULT_NOTEBOOK_ID));

    rsx! {
        section { id: "note-list-main", class: "note-list-panel", tabindex: "-1", aria_label: "Note list",
            NotesHeader {
                view: NotesListProps {
                    title: if let Some(notebook_id) = selected_notebook.read().as_ref() {
                        notebook_name_for(&notebooks.read(), notebook_id)
                    } else {
                        section.read().label().to_string()
                    },
                    creation_notebook_name: creation_notebook_name.clone(),
                    note_count: total_note_count,
                    sidebar_open: *sidebar_overlay_open.read(),
                },
                actions: NotesListActions {
                    on_open_sidebar: EventHandler::new(move |_| sidebar_overlay_open.set(true)),
                    on_create_note: EventHandler::new({
                        move |_| {
                            create_contextual_note(
                                services.read().clone(),
                                library_state,
                            );
                            compact_pane.set(CompactPane::Editor);
                            focus_editor_panel();
                            sidebar_overlay_open.set(false);
                        }
                    }),
                },
            }

            div { class: "search-row",
                label { class: "search-box",
                    span { class: "sr-only", "Search notes" }
                    span { aria_hidden: "true", "⌕" }
                    input {
                        aria_label: "Search notes",
                        title: "Search library (⌘/Ctrl + Shift + F)",
                        id: "note-search-input",
                        value: "{search}",
                        placeholder: "Search notes",
                        onkeydown: move |event| {
                            event.stop_propagation();
                            if event.key() == Key::Escape {
                                event.prevent_default();
                                let current_generation = *search_generation.read();
                                search_generation.set(current_generation.wrapping_add(1));
                                search.set(String::new());
                                let task_editor = services.read().editor();
                                let selection_generation =
                                    *library_state.note_selection_generation.peek();
                                spawn(async move {
                                    match library_state
                                        .synchronize_then_begin_multi_note_publication_when_privacy_ready(
                                            task_editor,
                                            SynchronizationMode::Checkpoint,
                                            selection_generation,
                                        )
                                        .await
                                    {
                                        Ok(Some(_publication_lease)) => {
                                            reload_note_list(library_state);
                                        }
                                        Ok(None) => {}
                                        Err(error) => {
                                            status.set(format!(
                                                "Could not finish saving: {error}"
                                            ));
                                        }
                                    }
                                });
                            }
                        },
                        oninput: move |event| {
                            let query = event.value();
                            search.set(query.clone());

                            let current_generation = *search_generation.read();
                            let scheduled_generation = current_generation.wrapping_add(1);
                            search_generation.set(scheduled_generation);

                            let search_editor = services.read().editor();
                            let selection_generation =
                                *library_state.note_selection_generation.peek();
                            spawn(async move {
                                tokio::time::sleep(Duration::from_millis(
                                    LIBRARY_SEARCH_DEBOUNCE_MS,
                                ))
                                .await;
                                if *search_generation.read() != scheduled_generation {
                                    return;
                                }
                                let aggregate_lease = match library_state
                                    .synchronize_then_begin_multi_note_publication_when_privacy_ready(
                                        search_editor,
                                        SynchronizationMode::Checkpoint,
                                        selection_generation,
                                    )
                                    .await
                                {
                                    Ok(Some(lease)) => lease,
                                    Ok(None) => return,
                                    Err(error) => {
                                        status.set(format!(
                                            "Could not finish saving: {error}"
                                        ));
                                        return;
                                    }
                                };
                                if *search_generation.read() != scheduled_generation {
                                    return;
                                }
                                let _publication_lease = aggregate_lease;
                                reload_note_list(library_state);
                            });
                        }
                    }
                }
                div { class: "popover-anchor",
                    button {
                        id: "search-help-trigger",
                        class: if *list_popover.read() == Some(ListPopover::SearchHelp) {
                            "icon-button active"
                        } else {
                            "icon-button"
                        },
                        title: "Search help",
                        aria_label: "Open search help",
                        aria_controls: "search-help-popover",
                        aria_haspopup: "dialog",
                        aria_expanded: "{*list_popover.read() == Some(ListPopover::SearchHelp)}",
                        "data-transient-trigger": "true",
                        onclick: move |_| {
                            let next = if *list_popover.read() == Some(ListPopover::SearchHelp) {
                                None
                            } else {
                                Some(ListPopover::SearchHelp)
                            };
                            list_popover.set(next);
                        },
                        "?"
                    }
                    if *list_popover.read() == Some(ListPopover::SearchHelp) {
                        SearchHelpPopover {
                            onclose: move |_| list_popover.set(None),
                        }
                    }
                }
            }

            div { class: "notes-toolbar",
                div { class: "notes-toolbar-primary",
                    div { class: "popover-anchor",
                        button {
                            id: "filter-trigger",
                            class: if *list_popover.read() == Some(ListPopover::Filter) || active_filter_count > 0 {
                                "toolbar-menu-button active"
                            } else {
                                "toolbar-menu-button"
                            },
                            aria_label: "Filter notes",
                            aria_controls: "filter-popover",
                            aria_haspopup: "dialog",
                            aria_expanded: "{*list_popover.read() == Some(ListPopover::Filter)}",
                            "data-transient-trigger": "true",
                            onclick: move |_| {
                                let next = if *list_popover.read() == Some(ListPopover::Filter) {
                                    None
                                } else {
                                    tag_filter_query.set(String::new());
                                    Some(ListPopover::Filter)
                                };
                                list_popover.set(next);
                            },
                            span { class: "filter-funnel" }
                            span { "Filter" }
                            if active_filter_count > 0 {
                                span { class: "filter-count", "{active_filter_count}" }
                            }
                        }
                        if *list_popover.read() == Some(ListPopover::Filter) {
                            FilterPopover {
                                view: FilterPopoverView {
                                    editor: services.read().editor(),
                                    library_state,
                                    available_tags: available_tags.clone(),
                                    visible_filter_tags: visible_filter_tags.clone(),
                                    tag_filter_query,
                                }
                            }
                        }
                    }
                }
                div { class: "notes-toolbar-secondary",
                    div { class: "popover-anchor sort-anchor",
                        button {
                            id: "sort-trigger",
                            class: if *list_popover.read() == Some(ListPopover::Sort) {
                                "toolbar-menu-button active"
                            } else {
                                "toolbar-menu-button"
                            },
                            aria_label: "Sort notes",
                            aria_controls: "sort-popover",
                            aria_haspopup: "menu",
                            aria_expanded: "{*list_popover.read() == Some(ListPopover::Sort)}",
                            "data-transient-trigger": "true",
                            onclick: move |_| {
                                let next = if *list_popover.read() == Some(ListPopover::Sort) {
                                    None
                                } else {
                                    Some(ListPopover::Sort)
                                };
                                list_popover.set(next);
                            },
                            span { class: "sort-label", "{sort_order.read().label()}" }
                            span { class: "toolbar-chevron", "⌄" }
                        }
                        if *list_popover.read() == Some(ListPopover::Sort) {
                            SortPopover {
                                view: SortPopoverView {
                                    services,
                                    library_state,
                                    list_popover,
                                }
                            }
                        }
                    }
                    button {
                        id: "keyboard-shortcuts-trigger",
                        class: if *show_shortcuts.read() { "icon-button keyboard-help-button active" } else { "icon-button keyboard-help-button" },
                        title: "Keyboard shortcuts (⌘/Ctrl + /)",
                        aria_label: "Open keyboard shortcuts",
                        aria_controls: "shortcuts-dialog",
                        aria_haspopup: "dialog",
                        aria_expanded: "{*show_shortcuts.read()}",
                        "data-transient-trigger": "true",
                        onclick: move |_| {
                            list_popover.set(None);
                            let next = !*show_shortcuts.read();
                            show_shortcuts.set(next);
                        },
                        "⌘/"
                    }
                }
            }

            if active_filter_count > 0 && !aggregate_revocation_active {
                ActiveFilterRow {
                    view: ActiveFilterRowView { editor: services.read().editor(), library_state }
                }
            }

            div { class: "result-summary",
                if search.read().is_empty() && filters.read().is_empty() {
                    span { "{total_note_count} notes" }
                } else {
                    span { "{total_note_count} matching notes" }
                }
            }

            div {
                id: "note-list-scroll",
                class: "note-list",
                tabindex: "0",
                aria_label: "Notes",
                aria_busy: "{*list_loading.read()}",
                onkeydown: move |event: KeyboardEvent| {
                    if event.is_composing() {
                        return;
                    }
                    if aggregate_revocation_active {
                        return;
                    }
                    let total = *note_count.read();
                    if total == 0 {
                        return;
                    }
                    let cache = note_cache.read();
                    let current = selected_note
                        .read()
                        .as_ref()
                        .and_then(|note| cache.absolute_index_of(&note.id))
                        .unwrap_or_else(|| {
                            ((*list_scroll_top.read() / NOTE_CARD_ROW_PX).floor() as usize)
                                .min(total.saturating_sub(1))
                        });
                    let page_rows = ((*list_viewport_height.read()).max(NOTE_CARD_ROW_PX)
                        / NOTE_CARD_ROW_PX)
                        .floor()
                        .max(1.0) as usize;
                    let navigation = match event.key() {
                        Key::ArrowDown => Some(NoteNavigation::Next),
                        Key::ArrowUp => Some(NoteNavigation::Previous),
                        Key::PageDown => Some(NoteNavigation::PageNext),
                        Key::PageUp => Some(NoteNavigation::PagePrevious),
                        Key::Home => Some(NoteNavigation::First),
                        Key::End => Some(NoteNavigation::Last),
                        _ => None,
                    };
                    let target = navigation.and_then(|navigation| {
                        navigation_target(current, total, page_rows, navigation)
                    });
                    drop(cache);
                    if let Some(target) = target {
                        event.prevent_default();
                        event.stop_propagation();
                        navigate_to_note_index(
                            services.read().editor(),
                            target,
                            library_state,
                        );
                    }
                },
                onscroll: move |event: ScrollEvent| {
                    let scroll_top = event.data().scroll_top();
                    let viewport_height = f64::from(event.data().client_height());
                    let scroll_anchor = virtual_scroll_anchor(scroll_top);
                    if *list_scroll_top.peek() != scroll_anchor {
                        list_scroll_top.set(scroll_anchor);
                    }
                    if *list_viewport_height.peek() != viewport_height {
                        list_viewport_height.set(viewport_height);
                    }
                    load_note_page_for_viewport(library_state, scroll_top, viewport_height);
                },
                if total_note_count == 0 {
                    div { class: "empty-list",
                        div { class: "empty-icon", "□" }
                        h3 { "No notes here" }
                        p {
                            if query_is_constrained {
                                "Change the search or filter."
                            } else {
                                {format!("Create a note in {creation_notebook_name}.")}
                            }
                        }
                        if !query_is_constrained {
                            button {
                                class: "empty-create-button",
                                onclick: {
                                    move |_| {
                                        create_contextual_note(
                                            services.read().clone(),
                                            library_state,
                                        );
                                        compact_pane.set(CompactPane::Editor);
                                        sidebar_overlay_open.set(false);
                                    }
                                },
                                "+ New note"
                            }
                        }
                    }
                }
                if virtual_window.top_spacer_px > 0.0 {
                    div { class: "virtual-note-spacer", style: "{top_spacer_style}", aria_hidden: "true" }
                }
                for note in visible_notes.iter() {
                    NoteCard {
                        key: "{note.id}",
                        view: NoteCardView {
                            note: note.clone(),
                            notebook_name: notebook_name_for(&notebooks.read(), &note.notebook_id),
                            notebooks: notebooks.read().clone(),
                            query: search.read().clone(),
                            active: selected_id.as_ref() == Some(&note.id),
                        },
                        actions: NoteCardActions {
                            onaction: EventHandler::new(move |action: NoteCardAction| {
                                apply_note_card_action(
                                    services.read().clone(),
                                    action,
                                    library_state,
                                );
                            }),
                            onopen: EventHandler::new({
                                move |note_id: NoteId| {
                                    select_note(
                                        services.read().editor(),
                                        note_id,
                                        library_state,
                                        move |selected_id| {
                                            compact_pane.set(CompactPane::Editor);
                                            focus_selected_note_surface(selected_id);
                                        },
                                    );
                                }
                            }),
                            onopentag: EventHandler::new({
                                move |tag: String| {
                                    let mut next = library_state.filters.read().clone();
                                    next.tags = vec![tag];
                                    next.without_tags = false;
                                    apply_note_filters(
                                        services.read().editor(),
                                        next,
                                        library_state,
                                    );
                                }
                            }),
                            ontags: EventHandler::new({
                                move |note_id: NoteId| {
                                    select_note(
                                        services.read().editor(),
                                        note_id,
                                        library_state,
                                        move |selected_id| {
                                            compact_pane.set(CompactPane::Editor);
                                            let mut request = library_state.tag_editor_request;
                                            request.set(Some(selected_id.clone()));
                                            focus_selected_note_surface(selected_id);
                                        },
                                    );
                                }
                            }),
                            ontagaction: EventHandler::new(move |action: NoteCardTagAction| {
                                apply_note_card_tag_action(
                                    services.read().clone(),
                                    action,
                                    tags,
                                    library_state,
                                );
                            }),
                        },
                    }
                }
                if virtual_window.bottom_spacer_px > 0.0 {
                    div { class: "virtual-note-spacer", style: "{bottom_spacer_style}", aria_hidden: "true" }
                }
                if *list_loading.read() && !note_cache.read().is_empty() {
                    div {
                        class: "note-list-loading",
                        role: "status",
                        aria_live: "polite",
                        "Loading more notes…"
                    }
                }
            }
        }
    }
}

pub(super) fn create_contextual_note(
    services: ApplicationServices,
    state: NotesListState,
) {
    let editor = services.editor();
    let library = services.library();
    let NotesListState {
        selected_notebook,
        mut section,
        mut search,
        mut search_generation,
        sort_order,
        mut filters,
        mut selected_note,
        mut status,
        ..
    } = state;
    // Creating a note is also a selection request. This lets a later note click
    // supersede a create request that is merely waiting for a Vault transition.
    let selection_generation = state.advance_note_selection_generation();
    state.spawn(async move {
        if !state.selected_source_checkpointed_by_active_vault_transition() {
            match state
                .synchronize_current_editor_when_privacy_ready(
                    editor.clone(),
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
        }

        if !state.note_selection_is_current(selection_generation) {
            return;
        }
        let target_notebook = selected_notebook
            .read()
            .clone()
            .unwrap_or_else(|| DEFAULT_NOTEBOOK_ID.to_string());
        let sort = sort_order.read().value().to_string();
        let default_filters = NoteFilters::default();
        let filter = default_filters.to_storage();
        let result = ApplicationExecutor::read("new note", move || {
            create_contextual_note_blocking(library, target_notebook, sort, filter)
        })
        .await;

        match result {
            Ok((note, preference_warning)) => {
                // Creation has already committed exactly once. Publish the new
                // target only under fresh authority for C; never reuse the
                // source-note token that checkpointed A.
                let target_note_id = note.id.clone();
                let _target_lease = loop {
                    if !state.note_selection_is_current(selection_generation) {
                        return;
                    }
                    let Some(target_token) = state.capture_note_publication_token(
                        &target_note_id,
                        state.note_selection_generation,
                    ) else {
                        tokio::time::sleep(Duration::from_millis(25)).await;
                        continue;
                    };
                    if let Some(lease) = target_token.begin_publication() {
                        break lease;
                    }
                };
                selected_note.set(Some(note));
                drop(_target_lease);

                // C is already authorized and selected. Make its editor usable
                // immediately even if an unrelated Vault transition keeps the
                // aggregate list unavailable for a little longer.
                status.set(preference_warning.unwrap_or_else(|| "New note created".to_string()));
                focus_title_input();

                // Section/search/filter state is aggregate. Acquire a fresh
                // multi-note lease independently of target-note publication.
                let _aggregate_lease = loop {
                    let Some(aggregate_token) = state
                        .wait_for_multi_note_publication_token(Some(selection_generation))
                        .await
                    else {
                        return;
                    };
                    if let Some(lease) = aggregate_token.begin_publication() {
                        break lease;
                    }
                };
                section.set(Section::Notes);
                let current_generation = *search_generation.read();
                search_generation.set(current_generation.wrapping_add(1));
                search.set(String::new());
                filters.set(default_filters);
                reload_note_list(state);
            }
            Err(error) => status.set(format!("Could not create note: {error}")),
        }
    });
}

fn create_contextual_note_blocking(
    library: LibraryService,
    target_notebook: String,
    sort: String,
    filter: String,
) -> Result<(Note, Option<String>), DataTaskError> {
    let note = library.create_note(Some(&target_notebook))?;
    let preference_warning = library
        .set_note_list_preferences(&sort, &filter)
        .err()
        .map(|error| {
            format!("Note created, but list preference could not be saved: {error}")
        });
    Ok((note, preference_warning))
}

pub(super) fn switch_library_section(
    next_section: Section,
    editor: EditorService,
    state: NotesListState,
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
        selected_notebook.set(None);
        reload_note_list(state);
    });
}

pub(super) fn toggle_selected_pin(
    services: ApplicationServices,
    state: NotesListState,
) {
    let Some(note_id) = state
        .selected_note
        .read()
        .as_ref()
        .map(|note| note.id.clone())
    else {
        return;
    };
    apply_note_card_action(
        services,
        NoteCardAction::TogglePin(note_id),
        state,
    );
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct SortPopoverView {
    pub services: Signal<ApplicationServices>,
    pub library_state: NotesListState,
    pub list_popover: Signal<Option<ListPopover>>,
}

#[component]
pub(super) fn SortPopover(view: SortPopoverView) -> Element {
    let SortPopoverView {
        services,
        library_state,
        mut list_popover,
    } = view;
    let NotesListState {
        mut sort_order,
        mut status,
        ..
    } = library_state;

    rsx! {
        section {
            id: "sort-popover",
            class: "list-popover sort-popover",
            role: "menu",
            aria_label: "Sort notes",
            "data-transient-surface": "true",
            "data-transient-kind": "menu",
            "data-dismiss-outside": "true",
            "data-return-focus": "sort-trigger",
            "data-fallback-focus": "note-list-main",
            p { class: "popover-group-label", "SORT NOTES" }
            for sort in [
                NoteSort::UpdatedNewest,
                NoteSort::UpdatedOldest,
                NoteSort::CreatedNewest,
                NoteSort::TitleAscending,
                NoteSort::TitleDescending,
            ] {
                button {
                    class: if *sort_order.read() == sort {
                        "popover-option selected"
                    } else {
                        "popover-option"
                    },
                    role: "menuitemradio",
                    aria_checked: if *sort_order.read() == sort { "true" } else { "false" },
                    onclick: move |_| {
                        let task_editor = services.read().editor();
                        let selection_generation =
                            *library_state.note_selection_generation.peek();
                        spawn(async move {
                            let _publication_lease = match library_state
                                .synchronize_then_begin_multi_note_publication_when_privacy_ready(
                                    task_editor,
                                    SynchronizationMode::Checkpoint,
                                    selection_generation,
                                )
                                .await
                            {
                                Ok(Some(lease)) => lease,
                                Ok(None) => return,
                                Err(error) => {
                                    status.set(format!(
                                        "Could not finish saving: {error}"
                                    ));
                                    return;
                                }
                            };
                            sort_order.set(sort);
                            reload_note_list(library_state);
                            request_note_list_preferences_save(library_state);
                            list_popover.set(None);
                        });
                    },
                    span { class: "option-check", if *sort_order.read() == sort { "✓" } else { "" } }
                    span { "{sort.label()}" }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct NotesListProps {
    pub title: String,
    pub creation_notebook_name: String,
    pub note_count: usize,
    pub sidebar_open: bool,
}

#[derive(Clone, PartialEq)]
pub(super) struct NotesListActions {
    pub on_open_sidebar: EventHandler<()>,
    pub on_create_note: EventHandler<()>,
}

#[component]
pub(super) fn NotesHeader(view: NotesListProps, actions: NotesListActions) -> Element {
    let NotesListProps {
        title,
        creation_notebook_name,
        note_count,
        sidebar_open,
    } = view;
    let NotesListActions {
        on_open_sidebar,
        on_create_note,
    } = actions;

    rsx! {
        header { class: "list-header",
            div {
                p { class: "eyebrow", "LIBRARY" }
                h2 { "{title}" }
            }
            div { class: "list-header-actions",
                button {
                    class: "responsive-nav-button folders-toggle",
                    title: "Open notebooks",
                    aria_label: "Open notebooks and library navigation",
                    aria_controls: "library-sidebar",
                    aria_expanded: "{sidebar_open}",
                    onclick: move |_| on_open_sidebar.call(()),
                    span { aria_hidden: "true", "☰" }
                    span { class: "responsive-nav-label", "Notebooks" }
                }
                span {
                    class: "note-count",
                    aria_label: "{note_count} notes",
                    "{note_count}"
                }
                button {
                    class: "list-new-note-button",
                    title: "Create a note in {creation_notebook_name}",
                    aria_label: "Create note",
                    onclick: move |_| on_create_note.call(()),
                    "+"
                }
            }
        }
    }
}
