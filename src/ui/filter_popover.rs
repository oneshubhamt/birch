use dioxus::prelude::*;

use crate::domain::NoteFilters;
use crate::application::editor::EditorService;

use super::note_list_data::reload_note_list;
use super::synchronization::SynchronizationMode;
use super::{NotesListState, request_note_list_preferences_save};

#[derive(Clone, PartialEq)]
pub(super) struct FilterPopoverView {
    pub editor: EditorService,
    pub library_state: NotesListState,
    pub available_tags: Vec<String>,
    pub visible_filter_tags: Vec<(String, bool)>,
    pub tag_filter_query: Signal<String>,
}

#[component]
pub(super) fn FilterPopover(view: FilterPopoverView) -> Element {
    let FilterPopoverView {
        editor,
        library_state,
        available_tags,
        visible_filter_tags,
        mut tag_filter_query,
    } = view;
    let filters = library_state.filters;
    let pinned_editor = editor.clone();
    let attachments_editor = editor.clone();
    let without_tags_editor = editor.clone();
    let tag_editor = editor.clone();

    rsx! {
        section {
            id: "filter-popover",
            class: "list-popover filter-popover",
            role: "dialog",
            aria_label: "Note filters",
            "data-transient-surface": "true",
            "data-transient-kind": "popover",
            "data-dismiss-outside": "true",
            "data-return-focus": "filter-trigger",
            "data-fallback-focus": "note-list-main",
            header { class: "popover-header compact",
                div {
                    p { class: "eyebrow", "FILTER" }
                    h3 { "Show notes" }
                }
            }
            div { class: "popover-group",
                p { class: "popover-group-label", "PROPERTIES" }
                button {
                    class: if filters.read().pinned { "popover-option selected" } else { "popover-option" },
                    aria_pressed: "{filters.read().pinned}",
                    onclick: move |_| {
                        let mut next = filters.read().clone();
                        next.pinned = !next.pinned;
                        apply_note_filters(
                            pinned_editor.clone(),
                            next,
                            library_state,
                        );
                    },
                    span { class: "option-check", if filters.read().pinned { "✓" } else { "" } }
                    span { "Pinned" }
                }
                button {
                    class: if filters.read().with_attachments { "popover-option selected" } else { "popover-option" },
                    aria_pressed: "{filters.read().with_attachments}",
                    onclick: move |_| {
                        let mut next = filters.read().clone();
                        next.with_attachments = !next.with_attachments;
                        apply_note_filters(
                            attachments_editor.clone(),
                            next,
                            library_state,
                        );
                    },
                    span { class: "option-check", if filters.read().with_attachments { "✓" } else { "" } }
                    span { "With attachments" }
                }
                button {
                    class: if filters.read().without_tags { "popover-option selected" } else { "popover-option" },
                    aria_pressed: "{filters.read().without_tags}",
                    onclick: move |_| {
                        let mut next = filters.read().clone();
                        next.without_tags = !next.without_tags;
                        if next.without_tags {
                            next.tags.clear();
                        }
                        apply_note_filters(
                            without_tags_editor.clone(),
                            next,
                            library_state,
                        );
                    },
                    span { class: "option-check", if filters.read().without_tags { "✓" } else { "" } }
                    span { "Without tags" }
                }
            }
            if !available_tags.is_empty() {
                div { class: "popover-divider" }
                div { class: "popover-group tag-options",
                    p { class: "popover-group-label", "TAGS · MATCH ALL" }
                    label { class: "tag-filter-search",
                        span { aria_hidden: "true", "⌕" }
                        input {
                            value: "{tag_filter_query}",
                            placeholder: "Search tags",
                            aria_label: "Search available tags",
                            oninput: move |event| tag_filter_query.set(event.value()),
                        }
                    }
                    div { class: "tag-filter-results",
                        if visible_filter_tags.is_empty() {
                            p { class: "tag-filter-empty", "No matching tags" }
                        }
                        for (tag, selected) in visible_filter_tags.clone() {
                            button {
                                class: if selected { "popover-option selected" } else { "popover-option" },
                                aria_pressed: if selected { "true" } else { "false" },
                                onclick: {
                                    let tag_value = tag.clone();
                                    let editor = tag_editor.clone();
                                    move |_| {
                                        let mut next = filters.read().clone();
                                        if let Some(index) = next
                                            .tags
                                            .iter()
                                            .position(|value| value.eq_ignore_ascii_case(&tag_value))
                                        {
                                            next.tags.remove(index);
                                        } else {
                                            next.tags.push(tag_value.clone());
                                            next.without_tags = false;
                                        }
                                        apply_note_filters(
                                            editor.clone(),
                                            next,
                                            library_state,
                                        );
                                    }
                                },
                                span { class: "option-check", if selected { "✓" } else { "" } }
                                span { class: "tag-option-label", "#{tag}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ActiveFilterRowView {
    pub editor: EditorService,
    pub library_state: NotesListState,
}

#[component]
pub(super) fn ActiveFilterRow(view: ActiveFilterRowView) -> Element {
    let ActiveFilterRowView {
        editor,
        library_state,
    } = view;
    let filters = library_state.filters;
    let pinned_editor = editor.clone();
    let attachments_editor = editor.clone();
    let without_tags_editor = editor.clone();
    let tag_editor = editor.clone();
    let clear_editor = editor;

    rsx! {
        div { class: "active-filter-row",
            if filters.read().pinned {
                FilterChip {
                    label: "Pinned".to_string(),
                    title: "Remove pinned filter".to_string(),
                    onclick: move |_| {
                        let mut next = filters.read().clone();
                        next.pinned = false;
                        apply_note_filters(
                            pinned_editor.clone(),
                            next,
                            library_state,
                        );
                    }
                }
            }
            if filters.read().with_attachments {
                FilterChip {
                    label: "With attachments".to_string(),
                    title: "Remove attachment filter".to_string(),
                    onclick: move |_| {
                        let mut next = filters.read().clone();
                        next.with_attachments = false;
                        apply_note_filters(
                            attachments_editor.clone(),
                            next,
                            library_state,
                        );
                    }
                }
            }
            if filters.read().without_tags {
                FilterChip {
                    label: "Without tags".to_string(),
                    title: "Remove without-tags filter".to_string(),
                    onclick: move |_| {
                        let mut next = filters.read().clone();
                        next.without_tags = false;
                        apply_note_filters(
                            without_tags_editor.clone(),
                            next,
                            library_state,
                        );
                    }
                }
            }
            for tag in filters.read().tags.clone() {
                FilterChip {
                    key: "{tag}",
                    label: format!("#{tag}"),
                    title: format!("Remove {tag} tag filter"),
                    onclick: {
                        let tag_value = tag.clone();
                        let editor = tag_editor.clone();
                        move |_| {
                            let mut next = filters.read().clone();
                            next.tags.retain(|value| !value.eq_ignore_ascii_case(&tag_value));
                            apply_note_filters(
                                editor.clone(),
                                next,
                                library_state,
                            );
                        }
                    }
                }
            }
            button {
                class: "clear-filter-button",
                onclick: move |_| {
                    apply_note_filters(
                        clear_editor.clone(),
                        NoteFilters::default(),
                        library_state,
                    );
                },
                "Clear"
            }
        }
    }
}

#[component]
pub(super) fn FilterChip(
    label: String,
    title: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "filter-chip",
            title: "{title}",
            onclick: move |event| onclick.call(event),
            span { "{label}" }
            span { class: "chip-remove", aria_hidden: "true", "×" }
        }
    }
}

pub(super) fn apply_note_filters(
    editor: EditorService,
    next: NoteFilters,
    state: NotesListState,
) {
    let NotesListState {
        mut filters,
        mut status,
        ..
    } = state;
    let selection_generation = *state.note_selection_generation.peek();
    spawn(async move {
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
        let next = next.normalized();
        filters.set(next);
        reload_note_list(state);
        request_note_list_preferences_save(state);
    });
}
