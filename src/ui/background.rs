use dioxus::prelude::*;

use crate::application::executor::{ApplicationExecutionStatus, ApplicationOperation};

#[component]
pub(super) fn BackgroundOperationViewport(status: Signal<ApplicationExecutionStatus>) -> Element {
    let state = status.read().clone();
    let quiet_background_operation = matches!(
        state.current,
        Some(ApplicationOperation::StartupMaintenance)
            | Some(ApplicationOperation::TrashMaintenance)
            | Some(ApplicationOperation::AutomaticBackup)
    ) && state.queued.is_none();
    if !state.is_active() || quiet_background_operation {
        return rsx! {};
    }
    let progress = if state.progress.trim().is_empty() {
        "Restoration in progress…".to_string()
    } else {
        state.progress.clone()
    };
    let editing_label = if state.editing_available {
        "Editing remains available"
    } else {
        "Editing is temporarily paused"
    };
    rsx! {
        aside {
            class: "background-operation-status",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            "data-editing-available": if state.editing_available { "true" } else { "false" },
            strong { "{progress}" }
            if let Some(waiting) = state.waiting {
                span { class: "background-operation-waiting", "{waiting}" }
            }
            span { class: "background-operation-editing", "{editing_label}" }
        }
    }
}
