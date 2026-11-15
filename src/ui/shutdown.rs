use std::time::Duration;

use dioxus::prelude::*;

use crate::application::executor::{
    ApplicationExecutionError, ApplicationExecutor, ApplicationOperation, ExecutionFailure,
};
use dioxus_desktop::{DesktopContext, WindowCloseBehaviour};

use crate::{
    application::errors::DataAccessError,
    application::{
        editor::EditorService,
        errors::EditorError,
        privacy::{PrivacyOperationService, PrivacyPublicationService},
        runtime::RuntimeService,
    },
    domain::Note,
};

use super::synchronization::{
    EditorSession, EditorSynchronizationContext, SynchronizationError, SynchronizationMode,
    begin_whole_library_operation_for_synchronized_selection, synchronize_current_editor_scoped,
};

const CLOSE_DURABILITY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum CloseDurabilityState {
    #[default]
    Idle,
    Saving,
    Failed(String),
}

impl CloseDurabilityState {
    pub(super) fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }
}

#[derive(Debug)]
enum CloseDurabilityError {
    Synchronization(SynchronizationError),
    Editor(EditorError),
    Data(DataAccessError),
    Execution(ApplicationExecutionError),
}

impl std::fmt::Display for CloseDurabilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synchronization(error) => std::fmt::Display::fmt(error, formatter),
            Self::Editor(error) => std::fmt::Display::fmt(error, formatter),
            Self::Data(error) => std::fmt::Display::fmt(error, formatter),
            Self::Execution(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for CloseDurabilityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Synchronization(error) => Some(error),
            Self::Editor(error) => Some(error),
            Self::Data(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<SynchronizationError> for CloseDurabilityError {
    fn from(error: SynchronizationError) -> Self {
        Self::Synchronization(error)
    }
}

impl From<EditorError> for CloseDurabilityError {
    fn from(error: EditorError) -> Self {
        Self::Editor(error)
    }
}

impl From<DataAccessError> for CloseDurabilityError {
    fn from(error: DataAccessError) -> Self {
        Self::Data(error)
    }
}

impl ExecutionFailure for CloseDurabilityError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

pub(super) struct CloseDurabilityRequest {
    pub(super) editor: EditorService,
    pub(super) runtime: RuntimeService,
    pub(super) selected_note: Signal<Option<Note>>,
    pub(super) editor_session: Signal<Option<EditorSession>>,
    pub(super) privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub(super) privacy: PrivacyPublicationService,
    pub(super) privacy_operations: PrivacyOperationService,
    pub(super) executor: ApplicationExecutor,
    pub(super) list_generation: Signal<u64>,
    pub(super) note_selection_generation: Signal<u64>,
    pub(super) state: Signal<CloseDurabilityState>,
    pub(super) desktop: DesktopContext,
}

/// Start the bounded durability barrier for a native close request.
///
/// The running editor window is configured to hide rather than close before
/// this is called, so the WebView and editor remain alive while the latest
/// ProseMirror snapshot is synchronized and every dirty note is checkpointed.
/// Only a successful barrier restores true close behaviour and destroys the
/// window.
pub(super) fn begin_close_durability(request: CloseDurabilityRequest) {
    let CloseDurabilityRequest {
        editor,
        runtime,
        selected_note,
        editor_session,
        privacy_epoch,
        privacy,
        privacy_operations,
        executor,
        list_generation,
        note_selection_generation,
        mut state,
        desktop,
    } = request;
    let already_saving = matches!(&*state.read(), CloseDurabilityState::Saving);
    if already_saving {
        return;
    }
    state.set(CloseDurabilityState::Saving);
    spawn(async move {
        let result = tokio::time::timeout(CLOSE_DURABILITY_TIMEOUT, async {
            let mut operation_turn = executor
                .begin_operation(ApplicationOperation::DurabilityCheckpoint)
                .await
                .map_err(CloseDurabilityError::Execution)?;

            // checkpoint_all may perform note-local protected saves. It must run
            // before the read-side whole-library lease; otherwise a Lock admitted
            // after lease capture could wait for the lease while checkpoint_all
            // waited for that same Lock. The serialized executor turn spans both
            // stages, so no Restore/snapshot can interleave in the gap.
            let synchronization = EditorSynchronizationContext::new(
                editor.clone(),
                selected_note,
                editor_session,
                privacy_epoch,
                &privacy,
                list_generation,
                note_selection_generation,
            );
            let library_operation_lease = loop {
                synchronize_current_editor_scoped(
                    &synchronization,
                    SynchronizationMode::Checkpoint,
                )
                .await?;
                let synchronized_selection_generation = *note_selection_generation.peek();

                let checkpoint_editor = editor.clone();
                let (next_turn, ()) = operation_turn
                    .stage(move || {
                        checkpoint_editor
                            .checkpoint_all()
                            .map_err(CloseDurabilityError::from)
                    })
                    .await?;
                operation_turn = next_turn;

                if let Some(lease) = begin_whole_library_operation_for_synchronized_selection(
                    &privacy_operations,
                    note_selection_generation,
                    synchronized_selection_generation,
                )
                .await?
                {
                    break lease;
                }
            };

            let durability_runtime = runtime.clone();
            operation_turn
                .execute(move || {
                    durability_runtime
                        .durability_checkpoint(&library_operation_lease)
                        .map_err(CloseDurabilityError::from)
                })
                .await?;

            Ok::<(), CloseDurabilityError>(())
        })
        .await;

        match result {
            Ok(Ok(())) => {
                desktop.set_close_behavior(WindowCloseBehaviour::WindowCloses);
                desktop.close();
            }
            Ok(Err(error)) => {
                restore_after_close_failure(
                    &desktop,
                    state,
                    format!(
                        "Rustnote could not complete the close checkpoint: {error}. \
                         The app remains open and was not force-closed."
                    ),
                );
            }
            Err(_) => {
                restore_after_close_failure(
                    &desktop,
                    state,
                    format!(
                        "Rustnote could not finish the close checkpoint within {} seconds. \
                         The app remains open and was not force-closed.",
                        CLOSE_DURABILITY_TIMEOUT.as_secs()
                    ),
                );
            }
        }
    });
}

fn restore_after_close_failure(
    desktop: &DesktopContext,
    mut state: Signal<CloseDurabilityState>,
    message: String,
) {
    state.set(CloseDurabilityState::Failed(message));
    desktop.set_close_behavior(WindowCloseBehaviour::WindowHides);
    desktop.window.set_visible(true);
    desktop.window.set_focus();
}

#[component]
pub(super) fn CloseDurabilityOverlay(
    state: CloseDurabilityState,
    onretry: EventHandler<()>,
    oncancel: EventHandler<()>,
) -> Element {
    match state {
        CloseDurabilityState::Idle => rsx! {},
        CloseDurabilityState::Saving => rsx! {
            div {
                class: "close-durability-backdrop",
                section {
                    id: "close-durability-dialog",
                    class: "close-durability-dialog",
                    role: "dialog",
                    tabindex: "-1",
                    aria_modal: "true",
                    aria_labelledby: "close-durability-title",
                    aria_describedby: "close-durability-description",
                    aria_busy: "true",
                    "data-transient-surface": "true",
                    "data-transient-kind": "dialog",
                    div { class: "close-durability-progress",
                        span {
                            class: "close-durability-spinner",
                            aria_hidden: "true",
                        }
                        div {
                            h2 { id: "close-durability-title", "Saving before closing" }
                            p {
                                id: "close-durability-description",
                                aria_live: "assertive",
                                "Synchronizing the editor and checkpointing local changes."
                            }
                        }
                    }
                }
            }
        },
        CloseDurabilityState::Failed(message) => rsx! {
            div {
                class: "close-durability-backdrop",
                section {
                    id: "close-durability-dialog",
                    class: "close-durability-dialog",
                    role: "dialog",
                    tabindex: "-1",
                    aria_modal: "true",
                    aria_labelledby: "close-durability-title",
                    aria_describedby: "close-durability-description",
                    "data-transient-surface": "true",
                    "data-transient-kind": "dialog",
                    h2 { id: "close-durability-title", "Rustnote stayed open" }
                    p {
                        id: "close-durability-description",
                        "Closing was stopped because the latest changes could not be fully checkpointed."
                    }
                    pre {
                        class: "close-durability-error",
                        role: "alert",
                        aria_live: "assertive",
                        "{message}"
                    }
                    div { class: "close-durability-actions",
                        button {
                            class: "close-durability-button secondary",
                            "data-transient-close": "close-durability-dialog",
                            onclick: move |_| oncancel.call(()),
                            "Keep app open"
                        }
                        button {
                            class: "close-durability-button primary",
                            autofocus: true,
                            "data-initial-focus": "true",
                            onclick: move |_| onretry.call(()),
                            "Retry close"
                        }
                    }
                }
            }
        },
    }
}
