use std::time::Duration;

use dioxus::prelude::*;
use crate::application::{executor::ApplicationExecutor, library::LibraryService, privacy::PrivacyPublicationService};

use crate::domain::{NoteCandidate, NoteId};


const NOTE_PICKER_SEARCH_DEBOUNCE_MS: u64 = 90;

fn short_note_id(note_id: &NoteId) -> String {
    let mut suffix = note_id.as_str().chars().rev().take(8).collect::<Vec<_>>();
    suffix.reverse();
    suffix.into_iter().collect()
}

#[derive(Clone, PartialEq)]
pub(super) struct NotePickerView {
    pub library: LibraryService,
    pub current_note_id: NoteId,
    pub initial_query: String,
    pub exclude_current: bool,
    pub eyebrow: &'static str,
    pub title: &'static str,
    pub aria_label: &'static str,
    pub footer: &'static str,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub library_generation: Signal<u64>,
    pub note_generation: Signal<u64>,
}

#[derive(Clone, PartialEq)]
pub(super) struct NotePickerActions {
    pub onselect: EventHandler<NoteCandidate>,
    pub oncancel: EventHandler<()>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn NotePicker(view: NotePickerView, actions: NotePickerActions) -> Element {
    let NotePickerView {
        library,
        current_note_id,
        initial_query,
        exclude_current,
        eyebrow,
        title,
        aria_label,
        footer,
        privacy_epoch,
        library_generation,
        note_generation,
    } = view;
    let NotePickerActions {
        onselect,
        oncancel,
        onstatus,
    } = actions;
    let mut query = use_signal(move || initial_query);
    let mut results = use_signal(Vec::<NoteCandidate>::new);
    let mut active_index = use_signal(|| 0_usize);
    let mut search_generation = use_signal(|| 0_u64);
    let mut loading = use_signal(|| true);
    let mut results_publication_token =
        use_signal(|| None::<super::privacy::PrivacyPublicationToken>);
    let privacy = use_context::<PrivacyPublicationService>();
    let multi_note_privacy = use_memo(move || privacy_epoch.observe_multi_note());
    let results_are_current = results_publication_token
        .read()
        .as_ref()
        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable);
    let search_current_note_id = current_note_id.clone();

    let _search_effect = use_effect(move || {
        let _multi_note_privacy = *multi_note_privacy.read();
        let requested_query = query.read().clone();
        let Some(publication_token) = privacy_epoch.capture_multi_note_publication_with_lifecycle(
            &privacy,
            library_generation,
            note_generation,
        ) else {
            results.set(Vec::new());
            results_publication_token.set(None);
            loading.set(false);
            return;
        };
        results.set(Vec::new());
        active_index.set(0);
        let next_generation = (*search_generation.peek()).wrapping_add(1);
        search_generation.set(next_generation);
        loading.set(true);
        let library = library.clone();
        let current_note_id = search_current_note_id.clone();
        spawn(async move {
            if !requested_query.trim().is_empty() {
                tokio::time::sleep(Duration::from_millis(NOTE_PICKER_SEARCH_DEBOUNCE_MS)).await;
            }
            if *search_generation.peek() != next_generation
                || !publication_token.is_publishable()
            {
                return;
            }
            let search_query = requested_query.clone();
            let result = ApplicationExecutor::read("note picker search", move || {
                library
                    .search_note_candidates(&search_query)
                    .map(|mut candidates| {
                        if exclude_current {
                            candidates.retain(|candidate| candidate.id != current_note_id);
                        }
                        candidates
                    })
                    .map_err(super::task_error::DataTaskError::from)
            })
            .await;
            if *search_generation.peek() != next_generation {
                return;
            }
            let _ = publication_token.publish(|| {
                loading.set(false);
                match result {
                    Ok(candidates) => {
                        active_index.set(0);
                        results_publication_token.set(Some(publication_token.clone()));
                        results.set(candidates);
                    }
                    Err(error) => {
                        results.set(Vec::new());
                        onstatus.call(format!("Could not search notes: {error}"));
                    }
                }
            });
        });
    });

    rsx! {
        div {
            class: "note-picker-backdrop",
            role: "presentation",
            onclick: move |_| oncancel.call(()),
            section {
                class: "note-picker",
                role: "dialog",
                aria_modal: "true",
                aria_label: "{aria_label}",
                onclick: move |event| event.stop_propagation(),
                header { class: "note-picker-header",
                    div {
                        p { class: "eyebrow", "{eyebrow}" }
                        h3 { "{title}" }
                    }
                    button {
                        class: "note-picker-close",
                        aria_label: "Cancel note selection",
                        title: "Cancel",
                        onclick: move |_| oncancel.call(()),
                        "×"
                    }
                }
                label { class: "note-picker-search",
                    span { class: "sr-only", "Search notes by title" }
                    span { aria_hidden: "true", "⌕" }
                    input {
                        autofocus: true,
                        value: "{query}",
                        placeholder: "Search note titles",
                        autocomplete: "off",
                        oninput: move |event| {
                            query.set(event.value());
                            active_index.set(0);
                        },
                        onkeydown: move |event| {
                            let count = if results_are_current { results.read().len() } else { 0 };
                            match event.key() {
                                Key::Escape => {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    oncancel.call(());
                                }
                                Key::ArrowDown if count > 0 => {
                                    event.prevent_default();
                                    let next = (*active_index.peek() + 1) % count;
                                    active_index.set(next);
                                }
                                Key::ArrowUp if count > 0 => {
                                    event.prevent_default();
                                    let current = *active_index.peek();
                                    active_index.set(current.checked_sub(1).unwrap_or(count - 1));
                                }
                                Key::Enter if count > 0 => {
                                    event.prevent_default();
                                    let index = (*active_index.peek()).min(count - 1);
                                    let candidate = results.read()[index].clone();
                                    if results_publication_token
                                        .peek()
                                        .as_ref()
                                        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable)
                                    {
                                        onselect.call(candidate);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                div {
                    class: "note-picker-results",
                    role: "listbox",
                    aria_label: "Matching notes",
                    if *loading.read() || !results_are_current {
                        div { class: "note-picker-empty", "Searching…" }
                    } else if results.read().is_empty() {
                        div { class: "note-picker-empty", "No matching notes" }
                    } else {
                        for (index, candidate) in results.read().clone().into_iter().enumerate() {
                            {
                                let candidate_id = candidate.id.as_str().to_string();
                                let title = candidate.display_title().to_string();
                                let duplicate_same_context = results.read().iter().any(|other| {
                                    other.id != candidate.id
                                        && other.display_title() == candidate.display_title()
                                        && other.notebook_name == candidate.notebook_name
                                });
                                let mut context = candidate.notebook_name.clone();
                                if candidate.id == current_note_id {
                                    context.push_str(" · Current note");
                                } else if candidate.archived {
                                    context.push_str(" · Archived");
                                }
                                if duplicate_same_context {
                                    let short_id = short_note_id(&candidate.id);
                                    context.push_str(&format!(" · ID {short_id}"));
                                }
                                let selected = index == *active_index.read();
                                rsx! {
                                    button {
                                        key: "{candidate_id}",
                                        class: if selected { "note-picker-result active" } else { "note-picker-result" },
                                        role: "option",
                                        aria_selected: if selected { "true" } else { "false" },
                                        onmouseenter: move |_| active_index.set(index),
                                        onclick: move |_| {
                                            if results_publication_token
                                                .peek()
                                                .as_ref()
                                                .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable)
                                            {
                                                onselect.call(candidate.clone());
                                            }
                                        },
                                        strong { "{title}" }
                                        span { "{context}" }
                                    }
                                }
                            }
                        }
                    }
                }
                footer { class: "note-picker-footer",
                    span { "↑↓ select · Enter choose · Esc cancel" }
                    span { "{footer}" }
                }
            }
        }
    }
}
