use dioxus::prelude::*;

use crate::{
    link::{LinkValidationError, normalize_safe_href},
};

use super::live_editor::{
    live_editor_adjust_font_size_script, live_editor_capture_link_script,
    live_editor_exit_focus_script, live_editor_font_size_input_script,
    live_editor_format_and_dismiss_script, live_editor_format_script,
    live_editor_insert_link_script, live_editor_open_insert_menu_script,
    live_editor_refresh_surface_script, live_editor_toggle_focus_script,
    live_editor_toggle_window_fullscreen_script, live_editor_typography_script,
};
use super::{EditorPopover, FormatAction};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TitleTypographyCommand {
    Toggle(FormatAction),
    SetFontFamily(String),
    SetFontSize(u8),
    AdjustFontSize(i8),
    SetTextColor(String),
    Clear,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TextStatistics {
    pub(super) words: usize,
}

impl TextStatistics {
    pub(super) fn from_text(text: &str) -> Self {
        let mut words = 0_usize;
        let mut inside_word = false;

        for character in text.chars() {
            if character.is_whitespace() {
                inside_word = false;
            } else if !inside_word {
                words += 1;
                inside_word = true;
            }
        }

        Self { words }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct FindPanelView {
    pub find_query: Signal<String>,
    pub find_index: Signal<usize>,
    pub find_count: Signal<usize>,
}

#[component]
pub(super) fn FindPanel(view: FindPanelView) -> Element {
    let FindPanelView {
        mut find_query,
        mut find_index,
        find_count,
    } = view;
    let find_position = if *find_count.read() == 0 {
        "0/0".to_string()
    } else {
        format!(
            "{}/{}",
            (*find_index.read() % *find_count.read()) + 1,
            *find_count.read()
        )
    };

    rsx! {
        section { class: "right-panel-find", aria_label: "Find within this note",
            div { class: "right-panel-find-search",
                span { class: "note-find-symbol", aria_hidden: "true", "⌕" }
                input {
                    id: "note-find-input",
                    value: "{find_query}",
                    placeholder: "Find within this note",
                    aria_label: "Find within this note",
                    oninput: move |event| {
                        find_query.set(event.value());
                        find_index.set(0);
                    },
                    onkeydown: move |event| {
                        if event.key() == Key::Enter {
                            event.prevent_default();
                            let count = *find_count.read();
                            if count > 0 {
                                let current = *find_index.read();
                                let next = if event.modifiers().shift() {
                                    current.checked_sub(1).unwrap_or(count - 1)
                                } else {
                                    (current + 1) % count
                                };
                                find_index.set(next);
                            }
                        }
                    }
                }
            }
            div { class: "right-panel-find-navigation",
                span { class: "note-find-count", role: "status", aria_live: "polite", "{find_position}" }
                div { class: "right-panel-find-buttons",
                    button {
                        class: "note-find-button",
                        title: "Previous match",
                        aria_label: "Previous match",
                        disabled: *find_count.read() == 0,
                        onclick: move |_| {
                            let count = *find_count.read();
                            if count > 0 {
                                let current = *find_index.read();
                                find_index.set(current.checked_sub(1).unwrap_or(count - 1));
                            }
                        },
                        "↑"
                    }
                    button {
                        class: "note-find-button",
                        title: "Next match",
                        aria_label: "Next match",
                        disabled: *find_count.read() == 0,
                        onclick: move |_| {
                            let count = *find_count.read();
                            if count > 0 {
                                let current = *find_index.read();
                                find_index.set((current + 1) % count);
                            }
                        },
                        "↓"
                    }
                }
            }
        }
    }
}

pub(super) fn open_insert_menu() {
    spawn(async move {
        let script = live_editor_open_insert_menu_script();
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn apply_typography(kind: &'static str, value: &'static str) {
    let script = live_editor_typography_script(kind, value);
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn adjust_font_size(delta: i32) {
    let script = live_editor_adjust_font_size_script(delta);
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn apply_font_size_input() {
    spawn(async move {
        let script = live_editor_font_size_input_script();
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn refresh_editor_surface() {
    spawn(async move {
        let script = live_editor_refresh_surface_script();
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn toggle_editor_focus() {
    spawn(async move {
        let script = live_editor_toggle_focus_script();
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn exit_editor_focus() {
    spawn(async move {
        let script = live_editor_exit_focus_script();
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn toggle_window_fullscreen() {
    spawn(async move {
        let script = live_editor_toggle_window_fullscreen_script();
        let _ = document::eval(&script).join::<bool>().await;
    });
}

fn dispatch_format_action(action: FormatAction) {
    let script = format_script(action);
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

fn dispatch_typography_character_action(action: FormatAction) {
    let script = live_editor_format_and_dismiss_script(
        action.script_name(),
        "editor-typography-menu",
    );
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

fn dispatch_block_style_action(
    action: FormatAction,
    mut editor_popover: Signal<Option<EditorPopover>>,
) {
    let script = format_script(action);
    spawn(async move {
        if matches!(document::eval(&script).join::<bool>().await, Ok(true)) {
            editor_popover.set(None);
        }
    });
}

#[component]
pub(super) fn FormatButton(
    label: &'static str,
    title: &'static str,
    action: FormatAction,
) -> Element {
    rsx! {
        button {
            class: "format-button",
            title: "{title}",
            aria_label: "{title}",
            aria_pressed: "false",
            "data-live-editor-format": action.script_name(),
            onpointerdown: move |event| event.prevent_default(),
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| dispatch_format_action(action),
            "{label}"
        }
    }
}

#[component]
pub(super) fn TypographyFormatButton(
    label: &'static str,
    symbol: &'static str,
    title: &'static str,
    action: FormatAction,
    title_target: bool,
    title_pressed: bool,
    ontitleaction: EventHandler<TitleTypographyCommand>,
) -> Element {
    let title_supported = matches!(
        action,
        FormatAction::Bold
            | FormatAction::Italic
            | FormatAction::Underline
            | FormatAction::Strikethrough
    );
    rsx! {
        button {
            class: if title_target && title_pressed {
                "typography-character-choice active"
            } else {
                "typography-character-choice"
            },
            aria_label: "{title}",
            aria_pressed: if title_target && title_pressed { "true" } else { "false" },
            disabled: title_target && !title_supported,
            "data-live-editor-format": if title_target { "" } else { action.script_name() },
            onpointerdown: move |event| event.prevent_default(),
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| {
                if title_target {
                    if title_supported {
                        ontitleaction.call(TitleTypographyCommand::Toggle(action));
                    }
                } else {
                    dispatch_typography_character_action(action);
                }
            },
            span { class: "typography-character-symbol", aria_hidden: "true", "{symbol}" }
            span { "{label}" }
        }
    }
}

#[component]
pub(super) fn CommandButton(
    label: &'static str,
    title: &'static str,
    action: FormatAction,
) -> Element {
    rsx! {
        button {
            class: "format-button",
            title: "{title}",
            aria_label: "{title}",
            "data-live-editor-format": action.script_name(),
            onpointerdown: move |event| event.prevent_default(),
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| dispatch_format_action(action),
            "{label}"
        }
    }
}

#[component]
pub(super) fn BlockStyleButton(
    label: &'static str,
    title: &'static str,
    action: FormatAction,
    mut editor_popover: Signal<Option<EditorPopover>>,
) -> Element {
    rsx! {
        button {
            class: "block-style-option",
            role: "menuitemradio",
            title: "{title}",
            aria_label: "{title}",
            aria_checked: "false",
            "data-live-editor-block-option": action.script_name(),
            onpointerdown: move |event| event.prevent_default(),
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| dispatch_block_style_action(action, editor_popover),
            "{label}"
        }
    }
}

#[component]
pub(super) fn ListStyleButton(
    label: &'static str,
    title: &'static str,
    action: FormatAction,
    mut editor_popover: Signal<Option<EditorPopover>>,
) -> Element {
    rsx! {
        button {
            class: "list-style-option",
            role: "menuitemradio",
            title: "{title}",
            aria_label: "{title}",
            aria_checked: "false",
            "data-live-editor-list-style-option": action.script_name(),
            onpointerdown: move |event| event.prevent_default(),
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| dispatch_block_style_action(action, editor_popover),
            "{label}"
        }
    }
}

#[component]
pub(super) fn ListCommandButton(
    label: &'static str,
    title: &'static str,
    action: FormatAction,
    mut editor_popover: Signal<Option<EditorPopover>>,
) -> Element {
    rsx! {
        button {
            class: "list-style-option",
            role: "menuitem",
            title: "{title}",
            aria_label: "{title}",
            "data-live-editor-list-numbering-command": action.script_name(),
            onpointerdown: move |event| event.prevent_default(),
            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| dispatch_block_style_action(action, editor_popover),
            "{label}"
        }
    }
}

pub(super) fn begin_link_popover(
    mut editor_popover: Signal<Option<EditorPopover>>,
    mut link_url: Signal<String>,
    mut link_label: Signal<String>,
    mut link_error: Signal<Option<String>>,
    publication_scope: super::privacy::NotePrivacyPublicationScope,
) {
    let Some(publication_token) = publication_scope.capture() else {
        return;
    };
    link_url.set(String::new());
    link_error.set(None);
    spawn(async move {
        let script = live_editor_capture_link_script();
        let captured = document::eval(&script)
            .join::<String>()
            .await
            .unwrap_or_default();
        let parsed = serde_json::from_str::<serde_json::Value>(&captured).ok();
        let selected = parsed
            .as_ref()
            .and_then(|value| value.get("label"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim();
        let existing_url = parsed
            .as_ref()
            .and_then(|value| value.get("href"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let Some(_publication_lease) = publication_token.begin_publication() else {
            return;
        };
        link_label.set(if selected.is_empty() {
            "link text".to_string()
        } else {
            selected.to_string()
        });
        link_url.set(existing_url.to_string());
        editor_popover.set(Some(EditorPopover::Link));
        let _ = document::eval(
            "setTimeout(() => { const input = document.getElementById('note-link-url-input'); if (input) input.focus(); }, 0); return true;",
        )
        .join::<bool>()
        .await;
    });
}

pub(super) fn validate_link_url(raw: &str) -> Result<String, LinkValidationError> {
    normalize_safe_href(raw)
}

fn link_validation_message(error: LinkValidationError) -> &'static str {
    match error {
        LinkValidationError::Empty => "Enter a URL.",
        LinkValidationError::Whitespace => "URLs cannot contain spaces.",
        LinkValidationError::IncompleteWebAddress => "Enter a complete web address.",
        LinkValidationError::IncompleteEmailAddress => {
            "Enter a complete email address after mailto:."
        }
        LinkValidationError::UnsupportedScheme => {
            "Use an https://, http://, or mailto: address."
        }
    }
}

pub(super) fn insert_link_from_popover(
    raw_url: String,
    label: String,
    mut editor_popover: Signal<Option<EditorPopover>>,
    mut link_error: Signal<Option<String>>,
    onstatus: EventHandler<String>,
    publication_scope: super::privacy::NotePrivacyPublicationScope,
) {
    let url = match validate_link_url(&raw_url) {
        Ok(url) => url,
        Err(error) => {
            link_error.set(Some(link_validation_message(error).to_string()));
            return;
        }
    };
    let label = if label.trim().is_empty() {
        "link text".to_string()
    } else {
        label.trim().to_string()
    };
    let Some(publication_token) = publication_scope.capture() else {
        return;
    };
    let script = live_editor_insert_link_script(&url, &label);
    link_error.set(None);
    spawn(async move {
        let Some(_publication_lease) = publication_token.begin_publication() else {
            return;
        };
        match document::eval(&script).join::<bool>().await {
            Ok(true) => {
                editor_popover.set(None);
                onstatus.call("Link applied".to_string());
            }
            _ => {
                link_error.set(Some(
                    "The editor selection was lost. Place the caret or select text and try again."
                        .to_string(),
                ));
                onstatus.call("Could not apply link".to_string());
            }
        }
    });
}

pub(super) fn format_script(action: FormatAction) -> String {
    live_editor_format_script(action.script_name())
}
