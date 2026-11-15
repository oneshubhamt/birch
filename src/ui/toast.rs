use dioxus::prelude::*;

use super::privacy::PrivacyPublicationToken;
use super::{ToastKind, ToastMessage};

#[component]
pub(super) fn ToastViewport(message: ToastMessage, ondismiss: EventHandler<()>) -> Element {
    let heading = toast_heading(&message.text, message.kind);
    rsx! {
        div {
            id: "operation-toast",
            class: message.kind.class_name(),
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            div { class: "toast-copy",
                strong { "{heading}" }
                span { "{message.text}" }
            }
            button {
                class: "toast-dismiss",
                r#type: "button",
                title: "Dismiss notification",
                aria_label: "Dismiss notification",
                onpointerdown: move |event| event.stop_propagation(),
                onclick: move |_| ondismiss.call(()),
                "×"
            }
        }
    }
}

fn toast_heading(message: &str, kind: ToastKind) -> &'static str {
    if message.starts_with("Incorrect password") {
        "Incorrect password"
    } else if kind == ToastKind::Error {
        "Action failed"
    } else {
        "Rustnote"
    }
}

pub(super) fn status_bar_class(status: &str) -> &'static str {
    match status {
        "Saving…" => "status-bar saving",
        "Save failed" => "status-bar error",
        _ => "status-bar",
    }
}

pub(super) fn route_status_message(
    message: String,
    save_status: Signal<String>,
    toast: Signal<Option<ToastMessage>>,
    toast_sequence: Signal<u64>,
) {
    route_status_message_with_privacy(message, save_status, toast, toast_sequence, None);
}

fn route_status_message_with_privacy(
    message: String,
    mut save_status: Signal<String>,
    mut toast: Signal<Option<ToastMessage>>,
    mut toast_sequence: Signal<u64>,
    privacy_token: Option<PrivacyPublicationToken>,
) {
    match message.as_str() {
        "Saved locally" | "Background save complete" => {
            save_status.set("Saved locally".to_string());
            return;
        }
        "Unsaved changes" | "Saving…" => {
            save_status.set("Saving…".to_string());
            return;
        }
        "Save failed" => {
            save_status.set("Save failed".to_string());
        }
        _ if is_save_failure(&message) => {
            save_status.set("Save failed".to_string());
        }
        _ => {}
    }

    if message == "Save failed" {
        return;
    }

    let kind = if is_error_message(&message) {
        ToastKind::Error
    } else {
        ToastKind::Info
    };
    let next_id = (*toast_sequence.peek()).wrapping_add(1);
    toast_sequence.set(next_id);
    toast.set(Some(ToastMessage {
        id: next_id,
        text: message,
        kind,
        privacy_token,
    }));

    spawn(async move {
        tokio::time::sleep(kind.timeout()).await;
        let should_clear = toast
            .peek()
            .as_ref()
            .is_some_and(|current| current.id == next_id);
        if should_clear {
            toast.set(None);
        }
    });
}

pub(super) fn is_save_failure(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.starts_with("save failed")
        || normalized.starts_with("autosave failed")
        || normalized.starts_with("could not schedule autosave")
        || normalized.starts_with("could not finish saving")
}

pub(super) fn is_error_message(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("failed")
        || normalized.contains("could not")
        || normalized.contains("error")
        || normalized.starts_with("incorrect password")
}

/// Central handoff for status text derived from an unlocked protected note.
/// The token is linearized at the actual global-state publication boundary,
/// then retained with the toast so later Lock/Protect revocation makes the
/// plaintext immediately non-renderable.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct ProtectedStatusPublisher {
    save_status: Signal<String>,
    toast: Signal<Option<ToastMessage>>,
    toast_sequence: Signal<u64>,
}

impl ProtectedStatusPublisher {
    pub(super) const fn new(
        save_status: Signal<String>,
        toast: Signal<Option<ToastMessage>>,
        toast_sequence: Signal<u64>,
    ) -> Self {
        Self {
            save_status,
            toast,
            toast_sequence,
        }
    }

    pub(super) fn publish(&self, token: PrivacyPublicationToken, message: String) -> bool {
        let retained_token = token.clone();
        token.publish(|| {
            route_status_message_with_privacy(
                message,
                self.save_status,
                self.toast,
                self.toast_sequence,
                Some(retained_token),
            );
        })
    }
}
