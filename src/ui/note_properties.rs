use dioxus::prelude::*;
use crate::application::{executor::ApplicationExecutor, notes::NoteService, privacy::PrivacyPublicationService};

use crate::domain::{Note, NoteId, NotePropertiesContext, Notebook};

use super::sidebar::notebook_name_for;

#[derive(Clone, PartialEq)]
pub(super) struct NotePropertiesPanelView {
    pub notes: NoteService,
    pub note: Note,
    pub notebooks: Vec<Notebook>,
    pub persistence_generation: Signal<u64>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub note_generation: Signal<u64>,
}

#[derive(Clone, PartialEq)]
pub(super) struct NotePropertiesPanelActions {
    pub onopen: EventHandler<NoteId>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn NotePropertiesPanel(
    view: NotePropertiesPanelView,
    actions: NotePropertiesPanelActions,
) -> Element {
    let NotePropertiesPanelView {
        notes,
        note,
        notebooks,
        persistence_generation,
        privacy_epoch,
        note_generation,
    } = view;
    let NotePropertiesPanelActions { onopen, onstatus } = actions;
    let mut context = use_signal(NotePropertiesContext::default);
    let mut loading = use_signal(|| true);
    let mut request_generation = use_signal(|| 0_u64);
    let mut published_token =
        use_signal(|| None::<super::privacy::PrivacyPublicationToken>);
    let privacy = use_context::<PrivacyPublicationService>();
    let note_privacy = super::privacy::use_note_privacy(privacy_epoch, note.id.clone());
    let context_is_current = published_token
        .read()
        .as_ref()
        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable);

    let context_notes = notes.clone();
    let context_note_id = note.id.clone();
    let context_privacy = note_privacy;
    let _context_effect = use_effect(move || {
        let _note_privacy = *context_privacy.read();
        let _persistence_generation = *persistence_generation.read();
        let Some(publication_token) = privacy_epoch.capture_note_publication(
            &privacy,
            &context_note_id,
            note_generation,
        ) else {
            context.set(NotePropertiesContext::default());
            published_token.set(None);
            loading.set(false);
            return;
        };
        context.set(NotePropertiesContext::default());
        let next_generation = (*request_generation.peek()).wrapping_add(1);
        request_generation.set(next_generation);
        loading.set(true);
        let notes = context_notes.clone();
        let note_id = context_note_id.clone();
        spawn(async move {
            let result = ApplicationExecutor::read("note properties", move || {
                notes
                    .properties(&note_id)
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
                Ok(next) => {
                    published_token.set(Some(publication_token.clone()));
                    context.set(next);
                },
                Err(error) => {
                    context.set(NotePropertiesContext::default());
                    onstatus.call(format!("Could not load note properties: {error}"));
                }
            }
        });
    });

    let notebook_name = notebook_name_for(&notebooks, &note.notebook_id);
    let created = format_timestamp_utc(note.created_at);
    let modified = format_timestamp_utc(note.updated_at);
    let tags = if note.tags.is_empty() {
        "None".to_string()
    } else {
        note.tags.join(", ")
    };
    let state = if note.trashed {
        "Trash"
    } else if note.archived {
        "Archived"
    } else if note.pinned {
        "Pinned"
    } else {
        "Normal"
    };
    let current = if context_is_current {
        context.read().clone()
    } else {
        NotePropertiesContext::default()
    };

    rsx! {
        section { class: "properties-panel", aria_label: "Note properties",
            div { class: "right-panel-summary",
                strong { "Current note metadata" }
                span { "Read directly from the note and normalized relationship tables." }
            }

            dl { class: "properties-list",
                PropertyRow { label: "Created", value: created }
                PropertyRow { label: "Modified", value: modified }
                PropertyRow { label: "Notebook", value: notebook_name }
                PropertyRow { label: "Tags", value: tags }
                PropertyRow { label: "State", value: state.to_string() }
                if *loading.read() || !context_is_current {
                    PropertyRow { label: "Relationships", value: "Loading…".to_string() }
                } else {
                    PropertyRow { label: "Attachments", value: current.attachment_count.to_string() }
                    PropertyRow { label: "Outgoing links", value: current.outgoing_link_count.to_string() }
                    PropertyRow { label: "Backlinks", value: current.backlink_count.to_string() }
                    PropertyRow { label: "Children", value: current.child_count.to_string() }
                    div { class: "property-row",
                        dt { "Parent" }
                        dd {
                            if let Some(parent) = current.parent.clone() {
                                {
                                    let parent_id = parent.id.clone();
                                    let title = parent.display_title().to_string();
                                    rsx! {
                                        button {
                                            class: "property-note-link",
                                            title: "Open parent note",
                                            onclick: move |_| {
                                                if context_is_current
                                                    && published_token
                                                        .peek()
                                                        .as_ref()
                                                        .is_some_and(super::privacy::PrivacyPublicationToken::is_publishable)
                                                {
                                                    onopen.call(parent_id.clone());
                                                }
                                            },
                                            "{title}"
                                        }
                                    }
                                }
                            } else {
                                span { class: "property-empty", "None" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PropertyRow(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "property-row",
            dt { "{label}" }
            dd { "{value}" }
        }
    }
}

fn format_timestamp_utc(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let (year, month, day) = civil_date_from_days(days);
    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    format!("{day} {} {year}, {hour:02}:{minute:02} UTC", month_name(month))
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    }
}

// Howard Hinnant's civil-from-days conversion, using the Unix epoch as day zero.
fn civil_date_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524
        - day_of_era / 146_096)
        / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}
