use std::path::PathBuf;

use dioxus::prelude::*;

use crate::build_info::FULL_BUILD_IDENTIFIER;
use crate::library::LibraryRuntime;
use crate::domain::{
    NoteFilters, NoteListCursor, NoteListQuery, NoteSort, NoteSummary, Notebook, Section,
};

use crate::application::{
    context::{ApplicationOpenError, ApplicationServices},
    executor::ApplicationExecutor,
    settings::{EditorPreferences, SettingsService},
};
use crate::application::errors::DataAccessError;
use super::app::RunningApp;
use super::responsive::{clamp_note_list_width, clamp_sidebar_width};

#[derive(Clone, PartialEq)]
pub(super) struct InitialApplication {
    pub services: ApplicationServices,
    pub notebooks: Vec<Notebook>,
    pub tags: Vec<String>,
    pub notes: Vec<NoteSummary>,
    pub note_count: usize,
    pub next_note_cursor: Option<NoteListCursor>,
    pub sort_order: NoteSort,
    pub filters: NoteFilters,
    pub dark_mode: bool,
    pub editor_preferences: EditorPreferences,
    pub sidebar_width: i32,
    pub note_list_width: i32,
}

#[derive(Clone, PartialEq)]
enum StartupState {
    Ready(Box<InitialApplication>),
    Failed {
        message: String,
        data_directory: PathBuf,
    },
}

#[derive(Debug)]
enum StartupError {
    Application(ApplicationOpenError),
    Data(DataAccessError),
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Application(error) => std::fmt::Display::fmt(error, formatter),
            Self::Data(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl From<ApplicationOpenError> for StartupError {
    fn from(error: ApplicationOpenError) -> Self { Self::Application(error) }
}

impl From<DataAccessError> for StartupError {
    fn from(error: DataAccessError) -> Self { Self::Data(error) }
}

fn initialize_application(libraries: &LibraryRuntime) -> StartupState {
    let data_directory = SettingsService::expected_data_directory();
    let result = (|| -> Result<InitialApplication, StartupError> {
        let services = ApplicationServices::open(libraries)?;
        let library = services.library();
        let settings = services.settings();
        let notebooks = library.list_notebooks()?;
        let tags = library.list_tags()?;
        let preferences = library.note_list_preferences()?;
        let sort_order = NoteSort::from_value(&preferences.0);
        let filters = NoteFilters::from_storage(&preferences.1);
        let initial_page = library
            .list_notes_page(
                &NoteListQuery {
                    section: Section::Notes,
                    notebook_id: None,
                    search: String::new(),
                    sort: sort_order,
                    filters: filters.clone(),
                },
                None,
            )?;
        let dark_mode = settings.dark_mode()?;
        let editor_preferences = settings.editor_preferences()?;
        let widths = settings.layout_widths()?;
        Ok(InitialApplication {
            services,
            notebooks,
            tags,
            note_count: initial_page.total_count.unwrap_or(initial_page.notes.len()),
            next_note_cursor: initial_page.next_cursor,
            notes: initial_page.notes,
            sort_order,
            filters,
            dark_mode,
            editor_preferences,
            sidebar_width: clamp_sidebar_width(widths.0),
            note_list_width: clamp_note_list_width(widths.1),
        })
    })();
    match result {
        Ok(application) => StartupState::Ready(Box::new(application)),
        Err(error) => StartupState::Failed {
            message: error.to_string(),
            data_directory,
        },
    }
}

#[component]
pub fn App() -> Element {
    // Long-running workers own only this Arc-backed executor. The local signal below
    // is a rendering projection updated by a task in this scope; worker lifetimes are
    // owned by the application executor rather than by individual Dioxus components.
    let background_coordinator = use_hook(ApplicationExecutor::new);
    let initial_status_source = background_coordinator.clone();
    let mut background_operations = use_signal(move || initial_status_source.snapshot());
    let coordinator_context = background_coordinator.clone();
    let _background_coordinator_context = use_context_provider(move || coordinator_context);
    let _background_operations_context = use_context_provider(|| background_operations);
    let status_source = background_coordinator.clone();
    let _background_status_bridge = use_future(move || {
        let mut updates = status_source.subscribe();
        async move {
            let initial = updates.borrow_and_update().clone();
            background_operations.set(initial);
            while updates.changed().await.is_ok() {
                let next = updates.borrow_and_update().clone();
                background_operations.set(next);
            }
        }
    });
    let libraries = use_context::<LibraryRuntime>();
    let startup_libraries = libraries.clone();
    let mut startup = use_signal(move || initialize_application(&startup_libraries));
    let application = match startup.read().clone() {
        StartupState::Ready(initial) => {
            let initial = *initial;
            rsx! { RunningApp { initial } }
        }
        StartupState::Failed {
            message,
            data_directory,
        } => {
            let retry_libraries = libraries.clone();
            let reset_libraries = libraries.clone();
            rsx! {
            StartupRecovery {
                message,
                data_directory,
                onretry: move |_| startup.set(initialize_application(&retry_libraries)),
                onreset: move |_| {
                    match SettingsService::reset_configured_locations() {
                        Ok(()) => startup.set(initialize_application(&reset_libraries)),
                        Err(error) => startup.set(StartupState::Failed {
                            message: format!("Could not reset storage locations: {error}"),
                            data_directory: SettingsService::expected_data_directory(),
                        }),
                    }
                },
            }
            }
        }
    };

    rsx! { {application} }
}


#[component]
fn StartupRecovery(
    message: String,
    data_directory: PathBuf,
    onretry: EventHandler<()>,
    onreset: EventHandler<()>,
) -> Element {
    rsx! {
        main { class: "startup-recovery",
            section { class: "startup-recovery-card", role: "alert",
                span { class: "startup-recovery-mark", aria_hidden: "true", "!" }
                h1 { "Rustnote could not open its local library" }
                p { "Rustnote stopped before opening the editor. Correct the problem below, then retry." }
                pre { class: "startup-error", "{message}" }
                dl { class: "startup-diagnostics",
                    div {
                        dt { "Version" }
                        dd { "{FULL_BUILD_IDENTIFIER}" }
                    }
                    div {
                        dt { "Data directory" }
                        dd { "{data_directory.display()}" }
                    }
                }
                div { class: "settings-button-row",
                    button {
                        class: "primary-settings-button",
                        onclick: move |_| onretry.call(()),
                        "Retry startup"
                    }
                    button {
                        class: "secondary-settings-button",
                        onclick: move |_| onreset.call(()),
                        "Use default storage folders"
                    }
                }
                small {
                    "Retry after correcting the path or permissions. Using default storage folders changes only the next location; no library files are deleted."
                }
            }
        }
    }
}
