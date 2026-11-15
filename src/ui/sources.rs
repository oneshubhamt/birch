use crate::application::{
    executor::ApplicationExecutor, notes::NoteService, privacy::PrivacyPublicationService,
};
use dioxus::prelude::*;

use crate::domain::{NoteId, NoteSourceSummary, Source, SourceId, SourceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceComposerMode {
    New,
    AttachExisting,
}

#[derive(Clone, PartialEq)]
pub(super) struct SourcesPanelView {
    pub notes: NoteService,
    pub note_id: NoteId,
    pub read_only: bool,
    pub library_generation: Signal<u64>,
    pub note_generation: Signal<u64>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
}

#[derive(Clone, PartialEq)]
pub(super) struct SourcesPanelActions {
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn SourcesPanel(view: SourcesPanelView, actions: SourcesPanelActions) -> Element {
    let SourcesPanelView {
        notes,
        note_id,
        read_only,
        library_generation,
        note_generation,
        privacy_epoch,
    } = view;
    let SourcesPanelActions { onstatus } = actions;

    let mut summaries = use_signal(Vec::<NoteSourceSummary>::new);
    let mut loading = use_signal(|| true);
    let mut mutation_busy = use_signal(|| false);
    let mut reload_generation = use_signal(|| 0_u64);
    let mut request_generation = use_signal(|| 0_u64);
    let mut composer_mode = use_signal(|| None::<SourceComposerMode>);
    let mut expanded_source = use_signal(|| None::<SourceId>);

    let mut new_kind = use_signal(|| SourceKind::Report);
    let mut new_title = use_signal(String::new);
    let mut new_reference = use_signal(String::new);
    let mut source_query = use_signal(String::new);
    let mut source_candidates = use_signal(Vec::<Source>::new);
    let mut source_search_busy = use_signal(|| false);
    let mut source_search_generation = use_signal(|| 0_u64);
    let mut citation_locator = use_signal(String::new);
    let mut summaries_publication_token =
        use_signal(|| None::<super::privacy::PrivacyPublicationToken>);
    let mut candidates_publication_token =
        use_signal(|| None::<super::privacy::PrivacyPublicationToken>);
    let privacy = use_context::<PrivacyPublicationService>();
    let note_privacy = super::privacy::use_note_privacy(privacy_epoch, note_id.clone());
    let multi_note_privacy = use_memo(move || privacy_epoch.observe_multi_note());
    let local_privacy_invalidator = note_privacy;
    let _local_privacy_invalidator = use_effect(move || {
        let (_, _, revocation_active) = *local_privacy_invalidator.read();
        if !revocation_active {
            return;
        }
        summaries.set(Vec::new());
        summaries_publication_token.set(None);
        loading.set(false);
        mutation_busy.set(false);
        composer_mode.set(None);
        expanded_source.set(None);
        new_title.set(String::new());
        new_reference.set(String::new());
        citation_locator.set(String::new());
        let next = (*request_generation.peek()).wrapping_add(1);
        request_generation.set(next);
    });
    let summaries_are_current = summaries_publication_token
        .read()
        .as_ref()
        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable);
    let candidates_are_current = candidates_publication_token
        .read()
        .as_ref()
        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable);

    let load_notes = notes.clone();
    let load_note_id = note_id.clone();
    let load_privacy = privacy.clone();
    let load_note_privacy = note_privacy;
    let _load_effect = use_effect(move || {
        let _note_privacy = *load_note_privacy.read();
        let Some(publication_token) =
            privacy_epoch.capture_note_publication(&load_privacy, &load_note_id, note_generation)
        else {
            summaries.set(Vec::new());
            source_candidates.set(Vec::new());
            summaries_publication_token.set(None);
            candidates_publication_token.set(None);
            loading.set(false);
            return;
        };
        summaries.set(Vec::new());
        source_candidates.set(Vec::new());
        let _reload_generation = *reload_generation.read();
        let next_generation = (*request_generation.peek()).wrapping_add(1);
        request_generation.set(next_generation);
        loading.set(true);
        let notes = load_notes.clone();
        let note_id = load_note_id.clone();
        spawn(async move {
            let result = ApplicationExecutor::read("note sources", move || {
                notes
                    .list_sources(&note_id)
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
                    summaries_publication_token.set(Some(publication_token.clone()));
                    summaries.set(items);
                }
                Err(error) => {
                    summaries.set(Vec::new());
                    onstatus.call(format!("Could not load sources: {error}"));
                }
            }
        });
    });

    let search_notes = notes.clone();
    let search_note_id = note_id.clone();
    let search_privacy = privacy.clone();
    let _search_effect = use_effect(move || {
        let _multi_note_privacy_reactivity = *multi_note_privacy.read();
        let mode = *composer_mode.read();
        let query = source_query.read().clone();
        let Some(publication_token) = privacy_epoch.capture_multi_note_publication_with_lifecycle(
            &search_privacy,
            library_generation,
            note_generation,
        ) else {
            source_candidates.set(Vec::new());
            candidates_publication_token.set(None);
            source_search_busy.set(false);
            return;
        };
        if mode != Some(SourceComposerMode::AttachExisting) {
            source_candidates.set(Vec::new());
            source_search_busy.set(false);
            return;
        }
        let next_generation = (*source_search_generation.peek()).wrapping_add(1);
        source_search_generation.set(next_generation);
        source_search_busy.set(true);
        let notes = search_notes.clone();
        let note_id = search_note_id.clone();
        spawn(async move {
            let query_for_search = query.clone();
            let result = ApplicationExecutor::read("source candidates", move || {
                notes
                    .search_source_candidates(&note_id, &query_for_search)
                    .map_err(super::task_error::DataTaskError::from)
            })
            .await;
            if *source_search_generation.peek() != next_generation
                || source_query.peek().as_str() != query.as_str()
            {
                return;
            }
            let Some(_publication_lease) = publication_token.begin_publication() else {
                source_search_busy.set(false);
                return;
            };
            source_search_busy.set(false);
            match result {
                Ok(items) => {
                    candidates_publication_token.set(Some(publication_token.clone()));
                    source_candidates.set(items);
                }
                Err(error) => {
                    source_candidates.set(Vec::new());
                    onstatus.call(format!("Could not search sources: {error}"));
                }
            }
        });
    });

    let visible_summaries = if summaries_are_current {
        summaries.read().clone()
    } else {
        Vec::new()
    };
    let visible_source_candidates = if candidates_are_current {
        source_candidates.read().clone()
    } else {
        Vec::new()
    };
    let source_count = visible_summaries.len();
    let citation_count = visible_summaries
        .iter()
        .map(|summary| summary.citations.len())
        .sum::<usize>();

    rsx! {
        section { class: "sources-panel", aria_label: "Sources and citations",
            div { class: "right-panel-summary",
                strong {
                    if source_count == 1 { "1 source" } else { "{source_count} sources" }
                }
                span {
                    if citation_count == 1 {
                        "1 citation locator"
                    } else {
                        "{citation_count} citation locators"
                    }
                }
            }

            if !read_only {
                div { class: "sources-toolbar",
                    button {
                        class: "source-action primary",
                        disabled: *mutation_busy.read(),
                        onclick: move |_| {
                            new_kind.set(SourceKind::Report);
                            new_title.set(String::new());
                            new_reference.set(String::new());
                            composer_mode.set(Some(SourceComposerMode::New));
                        },
                        "Add source"
                    }
                    button {
                        class: "source-action",
                        disabled: *mutation_busy.read(),
                        onclick: move |_| {
                            source_query.set(String::new());
                            composer_mode.set(Some(SourceComposerMode::AttachExisting));
                        },
                        "Attach existing"
                    }
                }
            }

            if let Some(mode) = *composer_mode.read() {
                div { class: "source-composer",
                    if mode == SourceComposerMode::New {
                        label { class: "source-field",
                            span { "Type" }
                            select {
                                value: "{new_kind.read().value()}",
                                disabled: *mutation_busy.read(),
                                onchange: move |event| {
                                    if let Some(kind) = SourceKind::parse(&event.value()) {
                                        new_kind.set(kind);
                                    }
                                },
                                for kind in SourceKind::ALL {
                                    option { value: "{kind.value()}", "{kind.label()}" }
                                }
                            }
                        }
                        label { class: "source-field",
                            span { "Title" }
                            input {
                                value: "{new_title}",
                                maxlength: "512",
                                placeholder: "Second ARC — Ethics in Governance",
                                disabled: *mutation_busy.read(),
                                oninput: move |event| new_title.set(event.value()),
                            }
                        }
                        label { class: "source-field",
                            span { "Reference (optional)" }
                            input {
                                value: "{new_reference}",
                                maxlength: "4096",
                                placeholder: "URL, publisher, edition, or short reference",
                                disabled: *mutation_busy.read(),
                                oninput: move |event| new_reference.set(event.value()),
                            }
                        }
                        div { class: "source-composer-actions",
                            button {
                                class: "source-action primary",
                                disabled: *mutation_busy.read() || new_title.read().trim().is_empty(),
                                onclick: {
                                    let notes = notes.clone();
                                    let note_id = note_id.clone();
                                    let privacy = privacy.clone();
                                    move |_| {
                                        if *mutation_busy.peek() {
                                            return;
                                        }
                                        let Some(publication_token) = privacy_epoch.capture_note_publication(&privacy, &note_id, note_generation)
                                        else {
                                            return;
                                        };
                                        let title = new_title.peek().clone();
                                        let reference = new_reference.peek().clone();
                                        let kind = *new_kind.peek();
                                        mutation_busy.set(true);
                                        let notes = notes.clone();
                                        let note_id = note_id.clone();
                                        spawn(async move {
                                            let result = ApplicationExecutor::read("create source", move || {
                                                notes
                                                    .create_source_and_attach(
                                                        &note_id,
                                                        kind,
                                                        &title,
                                                        Some(&reference),
                                                    )
                                                    .map_err(super::task_error::DataTaskError::from)
                                            })
                                            .await;
                                            let Some(_publication_lease) = publication_token.begin_publication() else {
                                                mutation_busy.set(false);
                                                return;
                                            };
                                            mutation_busy.set(false);
                                            match result {
                                                Ok(source) => {
                                                    expanded_source.set(Some(source.id));
                                                    composer_mode.set(None);
                                                    let next = (*reload_generation.peek()).wrapping_add(1);
                                                    reload_generation.set(next);
                                                    onstatus.call("Source added".to_string());
                                                }
                                                Err(error) => onstatus.call(format!("Could not add source: {error}")),
                                            }
                                        });
                                    }
                                },
                                "Add"
                            }
                            button {
                                class: "source-action",
                                disabled: *mutation_busy.read(),
                                onclick: move |_| composer_mode.set(None),
                                "Cancel"
                            }
                        }
                    } else {
                        label { class: "source-field",
                            span { "Find a source" }
                            input {
                                value: "{source_query}",
                                placeholder: "Search title or reference",
                                disabled: *mutation_busy.read(),
                                oninput: move |event| source_query.set(event.value()),
                            }
                        }
                        if *source_search_busy.read() || !candidates_are_current {
                            div { class: "source-picker-status", "Searching…" }
                        } else if visible_source_candidates.is_empty() {
                            div { class: "source-picker-status", "No unattached sources found." }
                        } else {
                            div { class: "source-candidate-list",
                                for candidate in visible_source_candidates.clone() {
                                    {
                                        let source_id = candidate.id.clone();
                                        let title = candidate.display_title().to_string();
                                        let context = source_context(&candidate);
                                        rsx! {
                                            button {
                                                key: "{source_id}",
                                                class: "source-candidate",
                                                disabled: *mutation_busy.read(),
                                                onclick: {
                                                    let notes = notes.clone();
                                                    let note_id = note_id.clone();
                                                    let privacy = privacy.clone();
                                                    move |_| {
                                                        if *mutation_busy.peek() {
                                                            return;
                                                        }
                                                        let Some(publication_token) = privacy_epoch.capture_note_publication(&privacy, &note_id, note_generation)
                                                        else {
                                                            return;
                                                        };
                                                        mutation_busy.set(true);
                                                        let notes = notes.clone();
                                                        let note_id = note_id.clone();
                                                        let source_id = source_id.clone();
                                                        spawn(async move {
                                                            let source_id_for_attach = source_id.clone();
                                                            let result = ApplicationExecutor::read("attach source", move || {
                                                                notes
                                                                    .attach_source(&note_id, &source_id_for_attach)
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
                                                                    expanded_source.set(Some(source_id));
                                                                    composer_mode.set(None);
                                                                    let next = (*reload_generation.peek()).wrapping_add(1);
                                                                    reload_generation.set(next);
                                                                    onstatus.call("Source attached".to_string());
                                                                }
                                                                Err(error) => onstatus.call(format!("Could not attach source: {error}")),
                                                            }
                                                        });
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
                        button {
                            class: "source-action",
                            disabled: *mutation_busy.read(),
                            onclick: move |_| composer_mode.set(None),
                            "Cancel"
                        }
                    }
                }
            }

            if *loading.read() || !summaries_are_current {
                div { class: "right-panel-empty", span { "Loading sources…" } }
            } else if summaries.read().is_empty() {
                div { class: "right-panel-empty",
                    strong { "No sources yet" }
                    span { "Attach a reusable source, then add page, chapter, or section locators as needed." }
                }
            } else {
                div { class: "source-list",
                    for summary in visible_summaries.clone() {
                        {
                            let source = summary.source.clone();
                            let source_id = source.id.clone();
                            let is_expanded = expanded_source.read().as_ref() == Some(&source_id);
                            let title = source.display_title().to_string();
                            let context = source_context(&source);
                            let citations = summary.citations.clone();
                            rsx! {
                                article { key: "{source_id}", class: "source-card",
                                    button {
                                        class: "source-card-header",
                                        aria_expanded: if is_expanded { "true" } else { "false" },
                                        onclick: move |_| {
                                            if is_expanded {
                                                expanded_source.set(None);
                                                citation_locator.set(String::new());
                                            } else {
                                                expanded_source.set(Some(source_id.clone()));
                                                citation_locator.set(String::new());
                                            }
                                        },
                                        div {
                                            strong { "{title}" }
                                            span { "{context}" }
                                        }
                                        span { class: "source-card-count",
                                            if citations.len() == 1 { "1 cite" } else { "{citations.len()} cites" }
                                        }
                                    }

                                    if is_expanded {
                                        div { class: "source-card-body",
                                            if let Some(reference) = source.reference.clone() {
                                                div { class: "source-reference", "{reference}" }
                                            }

                                            div { class: "citation-section",
                                                span { class: "citation-label", "Citation locators" }
                                                if citations.is_empty() {
                                                    div { class: "citation-empty", "No page/section locators yet." }
                                                } else {
                                                    ul { class: "citation-list",
                                                        for citation in citations {
                                                            {
                                                                let citation_id = citation.id.clone();
                                                                rsx! {
                                                                    li { key: "{citation_id}",
                                                                        span { "{citation.locator}" }
                                                                        if !read_only {
                                                                            button {
                                                                                class: "citation-remove",
                                                                                title: "Remove citation locator",
                                                                                aria_label: "Remove citation locator {citation.locator}",
                                                                                disabled: *mutation_busy.read(),
                                                                                onclick: {
                                                                                    let notes = notes.clone();
                                                                                    let note_id = note_id.clone();
                                                                                    let privacy = privacy.clone();
                                                                                    move |_| {
                                                                                        if *mutation_busy.peek() {
                                                                                            return;
                                                                                        }
                                                                                        let Some(publication_token) = privacy_epoch.capture_note_publication(&privacy, &note_id, note_generation)
                                                                                        else {
                                                                                            return;
                                                                                        };
                                                                                        mutation_busy.set(true);
                                                                                        let notes = notes.clone();
                                                                                        let note_id = note_id.clone();
                                                                                        let citation_id = citation_id.clone();
                                                                                        spawn(async move {
                                                                                            let result = ApplicationExecutor::read("remove citation", move || {
                                                                                                notes
                                                                                                    .remove_citation(&note_id, &citation_id)
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
                                                                                                    onstatus.call("Citation locator removed".to_string());
                                                                                                }
                                                                                                Err(error) => onstatus.call(format!("Could not remove citation: {error}")),
                                                                                            }
                                                                                        });
                                                                                    }
                                                                                },
                                                                                "×"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            if !read_only {
                                                div { class: "citation-add-row",
                                                    input {
                                                        value: "{citation_locator}",
                                                        maxlength: "1024",
                                                        placeholder: "page 137, chapter 6, section 4.2…",
                                                        disabled: *mutation_busy.read(),
                                                        oninput: move |event| citation_locator.set(event.value()),
                                                    }
                                                    button {
                                                        class: "source-action",
                                                        disabled: *mutation_busy.read() || citation_locator.read().trim().is_empty(),
                                                        onclick: {
                                                            let notes = notes.clone();
                                                            let note_id = note_id.clone();
                                                            let source_id = source.id.clone();
                                                            let privacy = privacy.clone();
                                                            move |_| {
                                                                if *mutation_busy.peek() {
                                                                    return;
                                                                }
                                                                let Some(publication_token) = privacy_epoch.capture_note_publication(&privacy, &note_id, note_generation)
                                                                else {
                                                                    return;
                                                                };
                                                                let locator = citation_locator.peek().clone();
                                                                mutation_busy.set(true);
                                                                let notes = notes.clone();
                                                                let note_id = note_id.clone();
                                                                let source_id = source_id.clone();
                                                                spawn(async move {
                                                                    let result = ApplicationExecutor::read("add citation", move || {
                                                                        notes
                                                                            .add_citation(&note_id, &source_id, &locator)
                                                                            .map_err(super::task_error::DataTaskError::from)
                                                                    })
                                                                    .await;
                                                                    let Some(_publication_lease) = publication_token.begin_publication() else {
                                                                        mutation_busy.set(false);
                                                                        return;
                                                                    };
                                                                    mutation_busy.set(false);
                                                                    match result {
                                                                        Ok(_) => {
                                                                            citation_locator.set(String::new());
                                                                            let next = (*reload_generation.peek()).wrapping_add(1);
                                                                            reload_generation.set(next);
                                                                            onstatus.call("Citation locator added".to_string());
                                                                        }
                                                                        Err(error) => onstatus.call(format!("Could not add citation: {error}")),
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        "Add locator"
                                                    }
                                                }

                                                button {
                                                    class: "source-detach",
                                                    disabled: *mutation_busy.read(),
                                                    onclick: {
                                                        let notes = notes.clone();
                                                        let note_id = note_id.clone();
                                                        let source_id = source.id.clone();
                                                        let privacy = privacy.clone();
                                                        move |_| {
                                                            if *mutation_busy.peek() {
                                                                return;
                                                            }
                                                            let Some(publication_token) = privacy_epoch.capture_note_publication(&privacy, &note_id, note_generation)
                                                            else {
                                                                return;
                                                            };
                                                            mutation_busy.set(true);
                                                            let notes = notes.clone();
                                                            let note_id = note_id.clone();
                                                            let source_id = source_id.clone();
                                                            spawn(async move {
                                                                let result = ApplicationExecutor::read("detach source", move || {
                                                                    notes
                                                                        .detach_source(&note_id, &source_id)
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
                                                                        expanded_source.set(None);
                                                                        let next = (*reload_generation.peek()).wrapping_add(1);
                                                                        reload_generation.set(next);
                                                                        onstatus.call("Source detached from note".to_string());
                                                                    }
                                                                    Err(error) => onstatus.call(format!("Could not detach source: {error}")),
                                                                }
                                                            });
                                                        }
                                                    },
                                                    "Detach from this note"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if read_only {
                div { class: "hierarchy-read-only",
                    "Sources are read-only while this note is in Trash. Recover the note to edit them."
                }
            }
        }
    }
}

fn source_context(source: &Source) -> String {
    match source
        .reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(reference) => format!("{} · {reference}", source.kind.label()),
        None => source.kind.label().to_string(),
    }
}
