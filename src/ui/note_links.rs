use std::time::Duration;

use dioxus::prelude::*;

use crate::application::{
    executor::ApplicationExecutor, library::LibraryService, notes::NoteService,
    privacy::PrivacyPublicationService,
};
use crate::domain::{BacklinkSummary, NoteCandidate, NoteId};

use super::{
    note_picker::{NotePicker, NotePickerActions, NotePickerView},
};

const BACKLINK_REFRESH_DELAY_MS: u64 = 2_500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NoteLinkPickerRequest {
    pub request_id: String,
    pub initial_query: String,
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteLinkPickerView {
    pub library: LibraryService,
    pub request: NoteLinkPickerRequest,
    pub current_note_id: NoteId,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub library_generation: Signal<u64>,
    pub note_generation: Signal<u64>,
}

#[derive(Clone, PartialEq)]
pub(super) struct NoteLinkPickerActions {
    pub onselect: EventHandler<NoteCandidate>,
    pub oncancel: EventHandler<()>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn NoteLinkPicker(
    view: NoteLinkPickerView,
    actions: NoteLinkPickerActions,
) -> Element {
    let NoteLinkPickerView {
        library,
        request,
        current_note_id,
        privacy_epoch,
        library_generation,
        note_generation,
    } = view;
    let NoteLinkPickerActions {
        onselect,
        oncancel,
        onstatus,
    } = actions;

    rsx! {
        NotePicker {
            view: NotePickerView {
                library,
                current_note_id,
                initial_query: request.initial_query,
                exclude_current: false,
                eyebrow: "NOTE LINK",
                title: "Link to a note",
                aria_label: "Link to a note",
                footer: "Titles are labels; stable note IDs preserve the link.",
                privacy_epoch,
                library_generation,
                note_generation,
            },
            actions: NotePickerActions {
                onselect,
                oncancel,
                onstatus,
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct BacklinksPanelView {
    pub notes: NoteService,
    pub note_id: NoteId,
    pub document_revision: Signal<u64>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub library_generation: Signal<u64>,
    pub note_generation: Signal<u64>,
}

#[derive(Clone, PartialEq)]
pub(super) struct BacklinksPanelActions {
    pub onopen: EventHandler<NoteId>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn BacklinksPanel(
    view: BacklinksPanelView,
    actions: BacklinksPanelActions,
) -> Element {
    let BacklinksPanelView {
        notes,
        note_id,
        document_revision,
        privacy_epoch,
        library_generation,
        note_generation,
    } = view;
    let BacklinksPanelActions { onopen, onstatus } = actions;
    let mut backlinks = use_signal(Vec::<BacklinkSummary>::new);
    let mut loading = use_signal(|| true);
    let mut request_generation = use_signal(|| 0_u64);
    let mut backlinks_publication_token =
        use_signal(|| None::<super::privacy::PrivacyPublicationToken>);
    let privacy = use_context::<PrivacyPublicationService>();
    let multi_note_privacy = use_memo(move || privacy_epoch.observe_multi_note());
    let backlinks_are_current = backlinks_publication_token
        .read()
        .as_ref()
        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable);

    let _backlinks_effect = use_effect(move || {
        let _multi_note_privacy = *multi_note_privacy.read();
        let revision = *document_revision.read();
        let Some(publication_token) = privacy_epoch.capture_multi_note_publication_with_lifecycle(
            &privacy,
            library_generation,
            note_generation,
        ) else {
            backlinks.set(Vec::new());
            backlinks_publication_token.set(None);
            loading.set(false);
            return;
        };
        backlinks.set(Vec::new());
        let next_generation = (*request_generation.peek()).wrapping_add(1);
        request_generation.set(next_generation);
        loading.set(true);
        let notes = notes.clone();
        let target_id = note_id.clone();
        spawn(async move {
            if revision > 0 {
                tokio::time::sleep(Duration::from_millis(BACKLINK_REFRESH_DELAY_MS)).await;
            }
            if *request_generation.peek() != next_generation
                || !publication_token.is_publishable()
            {
                return;
            }
            let query_id = target_id.clone();
            let result = ApplicationExecutor::read("backlinks", move || {
                notes
                    .backlinks(&query_id)
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
                Ok(items) => {
                    backlinks_publication_token.set(Some(publication_token.clone()));
                    backlinks.set(items);
                },
                Err(error) => {
                    backlinks.set(Vec::new());
                    onstatus.call(format!("Could not load backlinks: {error}"));
                }
            }
        });
    });

    let backlink_count = if backlinks_are_current {
        backlinks.read().len()
    } else {
        0
    };

    rsx! {
        section { class: "backlinks-panel", aria_label: "Backlinks to this note",
            div { class: "right-panel-summary",
                strong {
                    if backlinks_are_current && backlinks.read().len() == 1 {
                        "1 backlink"
                    } else {
                        "{backlink_count} backlinks"
                    }
                }
                span { "Notes whose saved content links to this note." }
            }
            if *loading.read() || !backlinks_are_current {
                div { class: "right-panel-empty", span { "Loading backlinks…" } }
            } else if backlinks.read().is_empty() {
                div { class: "right-panel-empty",
                    strong { "No backlinks" }
                    span { "When another note links here, it will appear in this list." }
                }
            } else {
                div { class: "backlinks-list",
                    for backlink in backlinks.read().clone() {
                        {
                            let source_id = backlink.source_note_id.clone();
                            let title = backlink.display_title().to_string();
                            let mut context = backlink.notebook_name.clone();
                            if backlink.trashed {
                                context.push_str(" · Trash");
                            } else if backlink.archived {
                                context.push_str(" · Archived");
                            }
                            rsx! {
                                button {
                                    key: "{source_id}",
                                    class: if backlink.trashed { "backlink-item trashed" } else if backlink.archived { "backlink-item archived" } else { "backlink-item" },
                                    title: "Open {title}",
                                    onclick: move |_| {
                                        if backlinks_publication_token
                                            .peek()
                                            .as_ref()
                                            .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable)
                                        {
                                            onopen.call(source_id.clone());
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
        }
    }
}
