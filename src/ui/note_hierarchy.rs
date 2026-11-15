use dioxus::prelude::*;

use crate::application::{
    executor::ApplicationExecutor, library::LibraryService, notes::NoteService,
    privacy::PrivacyPublicationService,
};
use crate::domain::{HierarchyNoteSummary, NoteCandidate, NoteHierarchy, NoteId};

use super::note_picker::{NotePicker, NotePickerActions, NotePickerView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HierarchyPickerMode {
    SetParent,
    AddChild,
}

impl HierarchyPickerMode {
    fn title(self) -> &'static str {
        match self {
            Self::SetParent => "Set parent note",
            Self::AddChild => "Add child note",
        }
    }

    fn eyebrow(self) -> &'static str {
        match self {
            Self::SetParent => "PARENT NOTE",
            Self::AddChild => "CHILD NOTE",
        }
    }

    fn footer(self) -> &'static str {
        match self {
            Self::SetParent => "Only the parent relationship changes; the notebook stays the same.",
            Self::AddChild => {
                "An existing parent is replaced; the child stays in its current notebook."
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteHierarchyPanelView {
    pub notes: NoteService,
    pub library: LibraryService,
    pub note_id: NoteId,
    pub read_only: bool,
    pub library_generation: Signal<u64>,
    pub note_generation: Signal<u64>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteHierarchyPanelActions {
    pub onopen: EventHandler<NoteId>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn NoteHierarchyPanel(
    view: NoteHierarchyPanelView,
    actions: NoteHierarchyPanelActions,
) -> Element {
    let NoteHierarchyPanelView {
        notes,
        library,
        note_id,
        read_only,
        library_generation,
        note_generation,
        privacy_epoch,
    } = view;
    let NoteHierarchyPanelActions { onopen, onstatus } = actions;
    let mut hierarchy = use_signal(NoteHierarchy::default);
    let mut loading = use_signal(|| true);
    let mut picker_mode = use_signal(|| None::<HierarchyPickerMode>);
    let mut mutation_busy = use_signal(|| false);
    let mut reload_generation = use_signal(|| 0_u64);
    let mut request_generation = use_signal(|| 0_u64);
    let mut published_token = use_signal(|| None::<super::privacy::PrivacyPublicationToken>);
    let privacy = use_context::<PrivacyPublicationService>();
    let multi_note_privacy = use_memo(move || privacy_epoch.observe_multi_note());
    let hierarchy_is_current = published_token
        .read()
        .as_ref()
        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable);

    let hierarchy_notes = notes.clone();
    let hierarchy_note_id = note_id.clone();
    let hierarchy_privacy = privacy.clone();
    let _hierarchy_effect = use_effect(move || {
        let _multi_note_privacy = *multi_note_privacy.read();
        let _library_generation = *library_generation.read();
        let Some(publication_token) = privacy_epoch.capture_multi_note_publication_with_lifecycle(
            &hierarchy_privacy,
            library_generation,
            note_generation,
        ) else {
            hierarchy.set(NoteHierarchy::default());
            published_token.set(None);
            loading.set(false);
            return;
        };
        hierarchy.set(NoteHierarchy::default());
        let _reload_generation = *reload_generation.read();
        let next_generation = (*request_generation.peek()).wrapping_add(1);
        request_generation.set(next_generation);
        loading.set(true);
        let notes = hierarchy_notes.clone();
        let note_id = hierarchy_note_id.clone();
        spawn(async move {
            let result = ApplicationExecutor::read("note hierarchy", move || {
                notes
                    .hierarchy(&note_id)
                    .map_err(super::task_error::DataTaskError::from)
            })
            .await;
            if *request_generation.peek() != next_generation {
                return;
            }
            let Some(_publication_lease) = publication_token.begin_publication() else {
                loading.set(false);
                return;
            };
            loading.set(false);
            match result {
                Ok(current) => {
                    published_token.set(Some(publication_token.clone()));
                    hierarchy.set(current);
                }
                Err(error) => {
                    hierarchy.set(NoteHierarchy::default());
                    onstatus.call(format!("Could not load note hierarchy: {error}"));
                }
            }
        });
    });

    let current_hierarchy = if hierarchy_is_current {
        hierarchy.read().clone()
    } else {
        NoteHierarchy::default()
    };
    let parent = current_hierarchy.parent;
    let children = current_hierarchy.children;
    let child_count = children.len();

    rsx! {
        section { class: "hierarchy-panel", aria_label: "Note hierarchy",
            div { class: "right-panel-summary",
                strong {
                    if child_count == 1 {
                        "1 child note"
                    } else {
                        "{child_count} child notes"
                    }
                }
                span { "Hierarchy is independent of notebooks and version history." }
            }

            if *loading.read() || !hierarchy_is_current {
                div { class: "right-panel-empty", span { "Loading hierarchy…" } }
            } else {
                div { class: "hierarchy-section",
                    div { class: "hierarchy-section-header",
                        div {
                            span { class: "hierarchy-label", "Parent" }
                            span { class: "hierarchy-hint", "At most one parent" }
                        }
                        if !read_only {
                            button {
                                class: "hierarchy-action",
                                disabled: *mutation_busy.read(),
                                onclick: move |_| picker_mode.set(Some(HierarchyPickerMode::SetParent)),
                                if parent.is_some() { "Change" } else { "Set parent" }
                            }
                        }
                    }
                    if let Some(parent_note) = parent.clone() {
                        HierarchyNoteRow {
                            note: parent_note,
                            relation: "Parent",
                            onopen,
                        }
                        if !read_only {
                            button {
                                class: "hierarchy-remove",
                                disabled: *mutation_busy.read(),
                                onclick: {
                                    let notes = notes.clone();
                                    let child_note_id = note_id.clone();
                                    let privacy = privacy.clone();
                                    move |_| {
                                        if *mutation_busy.peek() {
                                            return;
                                        }
                                        let Some(publication_token) = privacy_epoch.capture_multi_note_publication_with_lifecycle(&privacy, library_generation, note_generation)
                                        else {
                                            return;
                                        };
                                        mutation_busy.set(true);
                                        let notes = notes.clone();
                                        let child_note_id = child_note_id.clone();
                                        spawn(async move {
                                            let result = ApplicationExecutor::read("remove note parent", move || {
                                                notes
                                                    .remove_parent(&child_note_id)
                                                    .map_err(super::task_error::DataTaskError::from)
                                            })
                                            .await;
                                            let Some(_publication_lease) = publication_token.begin_publication() else {
                                                mutation_busy.set(false);
                                                return;
                                            };
                                            mutation_busy.set(false);
                                            match result {
                                                Ok(()) => {
                                                    let next = (*reload_generation.peek()).wrapping_add(1);
                                                    reload_generation.set(next);
                                                    onstatus.call("Parent relationship removed".to_string());
                                                }
                                                Err(error) => onstatus.call(format!(
                                                    "Could not remove parent: {error}"
                                                )),
                                            }
                                        });
                                    }
                                },
                                "Remove parent"
                            }
                        }
                    } else {
                        div { class: "hierarchy-empty-row", "No parent note" }
                    }
                }

                div { class: "hierarchy-section hierarchy-children-section",
                    div { class: "hierarchy-section-header",
                        div {
                            span { class: "hierarchy-label", "Children" }
                            span { class: "hierarchy-hint", "Independent notes grouped beneath this note" }
                        }
                        if !read_only {
                            button {
                                class: "hierarchy-action",
                                disabled: *mutation_busy.read(),
                                onclick: move |_| picker_mode.set(Some(HierarchyPickerMode::AddChild)),
                                "Add child"
                            }
                        }
                    }
                    if children.is_empty() {
                        div { class: "hierarchy-empty-row", "No child notes" }
                    } else {
                        div { class: "hierarchy-list",
                            for child in children {
                                HierarchyNoteRow {
                                    key: "{child.id}",
                                    note: child,
                                    relation: "Child",
                                    onopen,
                                }
                            }
                        }
                    }
                }

                if read_only {
                    div { class: "hierarchy-read-only",
                        "Hierarchy is read-only while this note is in Trash. Recover the note to edit relationships."
                    }
                }
            }
        }

        if let Some(mode) = *picker_mode.read() {
            NotePicker {
                view: NotePickerView {
                    library: library.clone(),
                    current_note_id: note_id.clone(),
                    initial_query: String::new(),
                    exclude_current: true,
                    eyebrow: mode.eyebrow(),
                    title: mode.title(),
                    aria_label: mode.title(),
                    footer: mode.footer(),
                    privacy_epoch,
                    library_generation,
                    note_generation,
                },
                actions: NotePickerActions {
                    onselect: {
                        let privacy = privacy.clone();
                        EventHandler::new(move |candidate: NoteCandidate| {
                        if *mutation_busy.peek() {
                            return;
                        }
                        let Some(publication_token) = privacy_epoch.capture_multi_note_publication_with_lifecycle(&privacy, library_generation, note_generation)
                        else {
                            return;
                        };
                        mutation_busy.set(true);
                        let notes = notes.clone();
                        let current_note_id = note_id.clone();
                        let selected_note_id = candidate.id.clone();
                        spawn(async move {
                            let result = ApplicationExecutor::read("update note hierarchy", move || {
                                match mode {
                                    HierarchyPickerMode::SetParent => notes
                                        .set_parent(&current_note_id, &selected_note_id),
                                    HierarchyPickerMode::AddChild => notes
                                        .set_parent(&selected_note_id, &current_note_id),
                                }
                                .map_err(super::task_error::DataTaskError::from)
                            })
                            .await;
                            let Some(_publication_lease) = publication_token.begin_publication() else {
                                mutation_busy.set(false);
                                return;
                            };
                            mutation_busy.set(false);
                            match result {
                                Ok(()) => {
                                    picker_mode.set(None);
                                    let next = (*reload_generation.peek()).wrapping_add(1);
                                    reload_generation.set(next);
                                    onstatus.call(match mode {
                                        HierarchyPickerMode::SetParent => {
                                            "Parent relationship updated".to_string()
                                        }
                                        HierarchyPickerMode::AddChild => {
                                            "Child relationship updated".to_string()
                                        }
                                    });
                                }
                                Err(error) => onstatus.call(format!(
                                    "Could not update hierarchy: {error}"
                                )),
                            }
                        });
                        })
                    },
                    oncancel: EventHandler::new(move |_| picker_mode.set(None)),
                    onstatus,
                },
            }
        }
    }
}

#[component]
fn HierarchyNoteRow(
    note: HierarchyNoteSummary,
    relation: &'static str,
    onopen: EventHandler<NoteId>,
) -> Element {
    let note_id = note.id.clone();
    let title = note.display_title().to_string();
    let mut context = note.notebook_name.clone();
    if note.trashed {
        context.push_str(" · Trash");
    } else if note.archived {
        context.push_str(" · Archived");
    }
    let class = if note.trashed {
        "hierarchy-note trashed"
    } else if note.archived {
        "hierarchy-note archived"
    } else {
        "hierarchy-note"
    };

    rsx! {
        button {
            class,
            title: "Open {relation}: {title}",
            onclick: move |_| onopen.call(note_id.clone()),
            strong { "{title}" }
            span { "{context}" }
        }
    }
}
