use dioxus::prelude::*;

use crate::application::{
    context::ApplicationServices,
    settings::{
        AUTO_FIRST_LINE_INDENT_STEP_POINTS, EditorPreferences, MIN_AUTO_FIRST_LINE_INDENT_POINTS,
        max_auto_first_line_indent_points,
    },
};
use crate::domain::{Note, Notebook};

use super::TagCatalogMutation;
use super::editor_preferences::commit_editor_preferences;
use super::editor_toolbar_support::{
    BlockStyleButton, CommandButton, FormatButton, ListCommandButton, ListStyleButton,
    TitleTypographyCommand, TypographyFormatButton, adjust_font_size, apply_font_size_input,
    apply_typography, begin_link_popover, exit_editor_focus, insert_link_from_popover,
    open_insert_menu, refresh_editor_surface, toggle_editor_focus, toggle_window_fullscreen,
};
use super::note_metadata::{NoteMetadata, NoteMetadataActions, NoteMetadataView};
use super::right_panel::RightPanelMode;
use super::synchronization::EditorSession;
use super::{EditorPopover, FormatAction};

#[derive(Clone, PartialEq)]
pub(super) struct EditorToolbarView {
    pub notebooks: Vec<Notebook>,
    pub services: ApplicationServices,
    pub selected_note: Signal<Option<Note>>,
    pub editor_session: Signal<Option<EditorSession>>,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub library_generation: Signal<u64>,
    pub note_generation: Signal<u64>,
    pub draft: Signal<Note>,
    pub tags_text: Signal<String>,
    pub right_panel_mode: Signal<Option<RightPanelMode>>,
    pub right_panel_last_mode: Signal<RightPanelMode>,
    pub right_panel_locked: bool,
    pub editing_available: bool,
    pub editor_popover: Signal<Option<EditorPopover>>,
    pub link_url: Signal<String>,
    pub link_label: Signal<String>,
    pub link_error: Signal<Option<String>>,
    pub editor_preferences: Signal<EditorPreferences>,
    pub title_focused: Signal<bool>,
}

#[derive(Clone, PartialEq)]
pub(super) struct EditorToolbarActions {
    pub onback: EventHandler<()>,
    pub onpanelrequest: EventHandler<RightPanelMode>,
    pub onchanged: EventHandler<Note>,
    pub onopentag: EventHandler<String>,
    pub ontagcatalogchanged: EventHandler<TagCatalogMutation>,
    pub ontitletypography: EventHandler<TitleTypographyCommand>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn EditorToolbar(view: EditorToolbarView, actions: EditorToolbarActions) -> Element {
    let EditorToolbarView {
        notebooks,
        services,
        selected_note,
        editor_session,
        privacy_epoch,
        library_generation,
        note_generation,
        draft,
        tags_text,
        mut right_panel_mode,
        right_panel_last_mode,
        right_panel_locked,
        editing_available,
        mut editor_popover,
        mut link_url,
        link_label,
        mut link_error,
        mut editor_preferences,
        title_focused,
    } = view;
    let EditorToolbarActions {
        onback,
        onpanelrequest,
        onchanged,
        onopentag,
        ontagcatalogchanged,
        ontitletypography,
        onstatus,
    } = actions;
    let note_read_only = draft.read().trashed || draft.read().protection.is_locked();
    let panel_open = right_panel_mode.read().is_some();
    let auto_indent_services = services.clone();
    let auto_indent_amount_decrease_services = services.clone();
    let auto_indent_amount_increase_services = services.clone();
    let link_publication_scope = privacy_epoch.note_publication_scope(
        services.privacy(),
        draft.read().id.clone(),
        note_generation,
    );
    let link_popover_publication_scope = link_publication_scope.clone();
    let link_enter_publication_scope = link_publication_scope.clone();
    let link_apply_publication_scope = link_publication_scope;
    let mut typography_targets_title = use_signal(|| false);

    rsx! {
        div { class: "editor-toolbar-stack", aria_busy: right_panel_locked,
            header { class: "editor-toolbar editor-application-toolbar",
                button {
                    class: "editor-back-button",
                    title: "Back to note list",
                    aria_label: "Back to note list",
                    aria_controls: "note-list-main",
                    onclick: move |_| onback.call(()),
                    span { aria_hidden: "true", "←" }
                    span { class: "responsive-nav-label", "Notes" }
                }
                div { class: "editor-toolbar-note-context",
                    NoteMetadata {
                        view: NoteMetadataView {
                            notebooks: notebooks.clone(),
                            services: services.clone(),
                            selected_note,
                            editor_session,
                            privacy_epoch,
                            library_generation,
                            note_generation,
                            draft,
                            tags_text,
                            editor_popover,
                            editing_available,
                        },
                        actions: NoteMetadataActions {
                            onchanged,
                            onopentag,
                            ontagcatalogchanged,
                            onstatus,
                        },
                    }
                }
                div { class: "editor-actions", aria_label: "Note context",
                    button {
                        id: "editor-focus-trigger",
                        class: "tool-button editor-focus-trigger",
                        title: "Focus editor",
                        aria_label: "Focus editor",
                        aria_pressed: "false",
                        onclick: move |_| toggle_editor_focus(),
                        "Focus"
                    }
                    button {
                        id: "editor-right-panel-trigger",
                        class: if panel_open { "tool-button overflow-button active" } else { "tool-button overflow-button" },
                        title: if panel_open { "Close note context" } else { "Open note context" },
                        aria_label: if panel_open { "Close note context" } else { "Open note context" },
                        aria_controls: "editor-right-panel",
                        aria_expanded: if panel_open { "true" } else { "false" },
                        disabled: right_panel_locked,
                        onclick: move |_| {
                            editor_popover.set(None);
                            let panel_is_open = right_panel_mode.peek().is_some();
                            if panel_is_open {
                                right_panel_mode.set(None);
                            } else {
                                let requested_mode = *right_panel_last_mode.peek();
                                onpanelrequest.call(requested_mode);
                            }
                        },
                        "⋯"
                    }
                }
            }

            if note_read_only {
                div {
                    class: "editor-read-only-banner",
                    role: "status",
                    "Trash is read-only. Recover this note to edit it."
                }
                div { class: "editor-focus-readonly-actions", aria_label: "Focus controls",
                    button {
                        class: "tool-button editor-focus-window-button",
                        title: "Toggle app full screen",
                        aria_label: "Toggle app full screen",
                        onclick: move |_| toggle_window_fullscreen(),
                        "Full Screen"
                    }
                    button {
                        class: "tool-button editor-focus-done-button",
                        title: "Exit editor focus",
                        aria_label: "Exit editor focus",
                        onclick: move |_| exit_editor_focus(),
                        "Done"
                    }
                }
            } else {
            div { class: "editor-format-toolbar", role: "toolbar", aria_label: "Formatting controls",
                div { class: "format-group",
                    FormatButton { label: "↶", title: "Undo", action: FormatAction::Undo }
                    FormatButton { label: "↷", title: "Redo", action: FormatAction::Redo }
                    span { class: "toolbar-divider", aria_hidden: "true" }
                    div { class: "editor-popover-anchor typography-anchor",
                        button {
                            id: "editor-typography-trigger",
                            class: if *editor_popover.read() == Some(EditorPopover::Typography) { "typography-trigger active" } else { "typography-trigger" },
                            title: "Typography",
                            aria_label: "Typography",
                            aria_controls: "editor-typography-menu",
                            aria_haspopup: "dialog",
                            aria_expanded: if *editor_popover.read() == Some(EditorPopover::Typography) { "true" } else { "false" },
                            "data-transient-trigger": "true",
                            onpointerdown: move |event| event.prevent_default(),
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                let next = if *editor_popover.read() == Some(EditorPopover::Typography) {
                                    None
                                } else {
                                    Some(EditorPopover::Typography)
                                };
                                editor_popover.set(next);
                                if next.is_some() {
                                    let targets_title = *title_focused.peek();
                                    typography_targets_title.set(targets_title);
                                    if !targets_title {
                                        refresh_editor_surface();
                                    }
                                } else {
                                    typography_targets_title.set(false);
                                }
                            },
                            span { aria_hidden: "true", "Aa" }
                        }
                        if *editor_popover.read() == Some(EditorPopover::Typography) {
                            div {
                                id: "editor-typography-menu",
                                class: "editor-popover typography-popover",
                                role: "dialog",
                                aria_label: "Typography",
                                "data-transient-surface": "true",
                                "data-transient-kind": "popover",
                                "data-dismiss-outside": "true",
                                "data-return-focus": "editor-typography-trigger",
                                "data-fallback-focus": if *typography_targets_title.read() { "note-title-input" } else { "note-editor-main" },
                                "data-preserve-external-selection": if *typography_targets_title.read() { "note-title-input" } else { "" },
                                section { class: "typography-section typography-character-section",
                                    h3 { "Character" }
                                    div { class: "typography-character-grid",
                                        TypographyFormatButton { label: "Bold", symbol: "B", title: "Bold selected text", action: FormatAction::Bold, title_target: *typography_targets_title.read(), title_pressed: draft.read().title_style.bold, ontitleaction: ontitletypography }
                                        TypographyFormatButton { label: "Italic", symbol: "I", title: "Italicise selected text", action: FormatAction::Italic, title_target: *typography_targets_title.read(), title_pressed: draft.read().title_style.italic, ontitleaction: ontitletypography }
                                        TypographyFormatButton { label: "Underline", symbol: "U", title: "Underline selected text", action: FormatAction::Underline, title_target: *typography_targets_title.read(), title_pressed: draft.read().title_style.underline, ontitleaction: ontitletypography }
                                        TypographyFormatButton { label: "Strike", symbol: "S̶", title: "Strike through selected text", action: FormatAction::Strikethrough, title_target: *typography_targets_title.read(), title_pressed: draft.read().title_style.strikethrough, ontitleaction: ontitletypography }
                                        TypographyFormatButton { label: "Superscript", symbol: "x²", title: "Superscript selected text", action: FormatAction::Superscript, title_target: *typography_targets_title.read(), title_pressed: false, ontitleaction: ontitletypography }
                                        TypographyFormatButton { label: "Subscript", symbol: "x₂", title: "Subscript selected text", action: FormatAction::Subscript, title_target: *typography_targets_title.read(), title_pressed: false, ontitleaction: ontitletypography }
                                    }
                                }
                                section { class: "typography-section",
                                    h3 { "Font" }
                                    div { class: "typography-choice-grid typography-font-grid",
                                        for (label, value, extra_class) in [("System", "system", ""), ("Serif", "serif", " typography-font-serif"), ("Rounded", "rounded", " typography-font-rounded"), ("Mono", "mono", " typography-font-mono")] {
                                            button {
                                                class: if *typography_targets_title.read() && draft.read().title_style.font_family == value { format!("typography-choice{extra_class} active") } else { format!("typography-choice{extra_class}") },
                                                role: "radio",
                                                aria_checked: if *typography_targets_title.read() && draft.read().title_style.font_family == value { "true" } else { "false" },
                                                "data-live-editor-font-family": if *typography_targets_title.read() { "" } else { value },
                                                onpointerdown: move |event| event.prevent_default(),
                                                onclick: move |_| {
                                                    if *typography_targets_title.peek() {
                                                        ontitletypography.call(TitleTypographyCommand::SetFontFamily(value.to_string()));
                                                    } else {
                                                        apply_typography("font_family", value);
                                                    }
                                                },
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                                section { class: "typography-section typography-size-section",
                                    h3 { "Size" }
                                    div { class: "font-size-row",
                                        div { class: "font-size-control",
                                            button { class: "font-size-step", aria_label: "Decrease font size", "data-live-editor-font-size-step": if *typography_targets_title.read() { "" } else { "-1" }, onpointerdown: move |event| event.prevent_default(), onclick: move |_| { if *typography_targets_title.peek() { ontitletypography.call(TitleTypographyCommand::AdjustFontSize(-1)); } else { adjust_font_size(-1); } }, "−" }
                                            if *typography_targets_title.read() {
                                                input {
                                                    id: "editor-font-size-input",
                                                    class: "font-size-input",
                                                    r#type: "number",
                                                    min: "8",
                                                    max: "72",
                                                    step: "1",
                                                    inputmode: "numeric",
                                                    aria_label: "Title font size in points",
                                                    value: draft.read().title_style.font_size.unwrap_or(28).to_string(),
                                                    onkeydown: move |event| match event.key() {
                                                        Key::ArrowUp => { event.prevent_default(); ontitletypography.call(TitleTypographyCommand::AdjustFontSize(1)); }
                                                        Key::ArrowDown => { event.prevent_default(); ontitletypography.call(TitleTypographyCommand::AdjustFontSize(-1)); }
                                                        _ => {}
                                                    },
                                                    onchange: move |event| {
                                                        if let Ok(size) = event.value().parse::<u8>() {
                                                            ontitletypography.call(TitleTypographyCommand::SetFontSize(size));
                                                        }
                                                    },
                                                }
                                            } else {
                                                input {
                                                    id: "editor-font-size-input",
                                                    class: "font-size-input",
                                                    r#type: "number",
                                                    min: "8",
                                                    max: "72",
                                                    step: "1",
                                                    inputmode: "numeric",
                                                    aria_label: "Font size in points",
                                                    "data-live-editor-font-size-input": "true",
                                                    onkeydown: move |event| match event.key() {
                                                        Key::Enter => { event.prevent_default(); apply_font_size_input(); }
                                                        Key::ArrowUp => { event.prevent_default(); adjust_font_size(1); }
                                                        Key::ArrowDown => { event.prevent_default(); adjust_font_size(-1); }
                                                        _ => {}
                                                    },
                                                    onchange: move |_| apply_font_size_input(),
                                                }
                                            }
                                            span { class: "font-size-unit", "pt" }
                                            button { class: "font-size-step", aria_label: "Increase font size", "data-live-editor-font-size-step": if *typography_targets_title.read() { "" } else { "1" }, onpointerdown: move |event| event.prevent_default(), onclick: move |_| { if *typography_targets_title.peek() { ontitletypography.call(TitleTypographyCommand::AdjustFontSize(1)); } else { adjust_font_size(1); } }, "+" }
                                        }
                                        div { class: "font-size-presets", aria_label: "Common font sizes",
                                            for size in ["10", "12", "14", "18", "24"] {
                                                button { class: "font-size-preset", onpointerdown: move |event| event.prevent_default(), onclick: move |_| { if *typography_targets_title.peek() { if let Ok(parsed) = size.parse::<u8>() { ontitletypography.call(TitleTypographyCommand::SetFontSize(parsed)); } } else { apply_typography("font_size", size); } }, "{size}" }
                                            }
                                        }
                                    }
                                }
                                section { class: "typography-section",
                                    div { class: "typography-section-heading",
                                        h3 { "Text colour" }
                                        if *typography_targets_title.read() {
                                            span { class: "typography-colour-current",
                                                {match draft.read().title_style.text_color.as_str() {
                                                    "grey" => "Grey", "red" => "Red", "orange" => "Orange", "green" => "Green", "blue" => "Blue", "purple" => "Purple", _ => "Default",
                                                }}
                                            }
                                        } else {
                                            span { class: "typography-colour-current", "data-live-editor-text-color-label": "true", "Default" }
                                        }
                                    }
                                    div { class: "typography-colour-grid",
                                        for (label, value) in [("Default", "default"), ("Grey", "grey"), ("Red", "red"), ("Orange", "orange"), ("Green", "green"), ("Blue", "blue"), ("Purple", "purple")] {
                                            button { class: if *typography_targets_title.read() && draft.read().title_style.text_color == value { "typography-colour-choice active" } else { "typography-colour-choice" }, role: "radio", aria_checked: if *typography_targets_title.read() && draft.read().title_style.text_color == value { "true" } else { "false" }, aria_label: "{label} text", "data-live-editor-text-color": if *typography_targets_title.read() { "" } else { value }, onpointerdown: move |event| event.prevent_default(), onclick: move |_| { if *typography_targets_title.peek() { ontitletypography.call(TitleTypographyCommand::SetTextColor(value.to_string())); } else { apply_typography("text_color", value); } }, span { class: "typography-colour-dot", "data-colour": value } }
                                        }
                                    }
                                }
                                button {
                                    class: "clear-typography-button",
                                    title: "Reset font, size, colour, strike and script styling",
                                    aria_label: "Reset character typography",
                                    onpointerdown: move |event| event.prevent_default(),
                                    onclick: move |_| { if *typography_targets_title.peek() { ontitletypography.call(TitleTypographyCommand::Clear); } else { apply_typography("clear", "all"); } },
                                    "Reset character typography"
                                }
                            }
                        }
                    }
                    div { class: "editor-popover-anchor paragraph-layout-anchor",
                        button {
                            id: "editor-paragraph-layout-trigger",
                            class: if *editor_popover.read() == Some(EditorPopover::ParagraphLayout) { "paragraph-layout-trigger active" } else { "paragraph-layout-trigger" },
                            title: "Paragraph",
                            aria_label: "Paragraph",
                            aria_controls: "editor-paragraph-layout-menu",
                            aria_haspopup: "dialog",
                            aria_expanded: if *editor_popover.read() == Some(EditorPopover::ParagraphLayout) { "true" } else { "false" },
                            "data-transient-trigger": "true",
                            onpointerdown: move |event| event.prevent_default(),
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                let next = if *editor_popover.read() == Some(EditorPopover::ParagraphLayout) {
                                    None
                                } else {
                                    Some(EditorPopover::ParagraphLayout)
                                };
                                editor_popover.set(next);
                                if next.is_some() {
                                    refresh_editor_surface();
                                }
                            },
                            span { aria_hidden: "true", "¶" }
                        }
                        if *editor_popover.read() == Some(EditorPopover::ParagraphLayout) {
                            div {
                                id: "editor-paragraph-layout-menu",
                                class: "editor-popover paragraph-layout-popover",
                                role: "dialog",
                                aria_label: "Paragraph",
                                "data-transient-surface": "true",
                                "data-transient-kind": "popover",
                                "data-dismiss-outside": "true",
                                "data-return-focus": "editor-paragraph-layout-trigger",
                                "data-fallback-focus": "note-editor-main",
                                section { class: "paragraph-layout-section",
                                    h3 { "Alignment" }
                                    div { class: "paragraph-alignment-grid", role: "radiogroup", aria_label: "Paragraph alignment",
                                        for (label, value) in [("Left", "left"), ("Center", "center"), ("Right", "right"), ("Justify", "justify")] {
                                            button {
                                                class: "paragraph-alignment-choice",
                                                role: "radio",
                                                aria_checked: "false",
                                                "data-live-editor-text-alignment": value,
                                                "data-live-editor-paragraph-layout-control": "true",
                                                onpointerdown: move |event| event.prevent_default(),
                                                onclick: move |_| apply_typography("text_alignment", value),
                                                span { class: "paragraph-alignment-icon", "data-alignment-icon": value, aria_hidden: "true",
                                                    span {}
                                                    span {}
                                                    span {}
                                                }
                                                span { "{label}" }
                                            }
                                        }
                                    }
                                }
                                section { class: "paragraph-layout-section",
                                    h3 { "Indentation" }
                                    div { class: "paragraph-layout-row",
                                        span { "Left indent" }
                                        div { class: "paragraph-layout-stepper",
                                            button { "data-live-editor-paragraph-layout-control": "true", aria_label: "Decrease left indent", onpointerdown: move |event| event.prevent_default(), onclick: move |_| apply_typography("adjust_left_indent", "-1"), "−" }
                                            span { "data-live-editor-left-indent-value": "true", "0 pt" }
                                            button { "data-live-editor-paragraph-layout-control": "true", aria_label: "Increase left indent", onpointerdown: move |event| event.prevent_default(), onclick: move |_| apply_typography("adjust_left_indent", "1"), "+" }
                                        }
                                    }
                                    div { class: "paragraph-layout-row",
                                        span { "Right indent" }
                                        div { class: "paragraph-layout-stepper",
                                            button { "data-live-editor-paragraph-layout-control": "true", aria_label: "Decrease right indent", onpointerdown: move |event| event.prevent_default(), onclick: move |_| apply_typography("adjust_right_indent", "-1"), "−" }
                                            span { "data-live-editor-right-indent-value": "true", "0 pt" }
                                            button { "data-live-editor-paragraph-layout-control": "true", aria_label: "Increase right indent", onpointerdown: move |event| event.prevent_default(), onclick: move |_| apply_typography("adjust_right_indent", "1"), "+" }
                                        }
                                    }
                                    div { class: "paragraph-layout-row",
                                        span { "First-line indent" }
                                        div { class: "paragraph-layout-stepper",
                                            button { "data-live-editor-paragraph-layout-control": "true", aria_label: "Decrease first-line indent", onpointerdown: move |event| event.prevent_default(), onclick: move |_| apply_typography("adjust_first_line_indent", "-1"), "−" }
                                            span { "data-live-editor-first-line-indent-value": "true", "0 pt" }
                                            button { "data-live-editor-paragraph-layout-control": "true", aria_label: "Increase first-line indent", onpointerdown: move |event| event.prevent_default(), onclick: move |_| apply_typography("adjust_first_line_indent", "1"), "+" }
                                        }
                                    }
                                    button {
                                        class: "paragraph-layout-toggle",
                                        role: "switch",
                                        aria_checked: "false",
                                        "data-live-editor-drop-cap": "true",
                                        "data-live-editor-paragraph-layout-control": "true",
                                        onpointerdown: move |event| event.prevent_default(),
                                        onclick: move |_| apply_typography("drop_cap", "toggle"),
                                        span { "Drop cap" }
                                        span { "data-live-editor-drop-cap-label": "true", "Off" }
                                    }
                                }
                                section { class: "paragraph-layout-section",
                                    h3 { "Line spacing" }
                                    div { class: "typography-choice-grid typography-spacing-grid",
                                        for (label, value) in [("Auto", "default"), ("1.0", "1"), ("1.15", "1.15"), ("1.5", "1.5"), ("1.75", "1.75"), ("2.0", "2")] {
                                            button { class: "typography-choice", role: "radio", aria_checked: "false", "data-live-editor-line-spacing": value, onpointerdown: move |event| event.prevent_default(), onclick: move |_| apply_typography("line_spacing", value), "{label}" }
                                        }
                                    }
                                    p { class: "paragraph-layout-hint", "Auto clears explicit spacing and uses Rustnote's normal paragraph spacing. Settings → Editor controls the default for new paragraphs." }
                                }
                                section { class: "paragraph-layout-section",
                                    h3 { "New paragraphs" }
                                    button {
                                        class: "paragraph-layout-toggle",
                                        role: "switch",
                                        aria_checked: if editor_preferences.read().auto_indent { "true" } else { "false" },
                                        onclick: move |_| {
                                            let enabled = !editor_preferences.peek().auto_indent;
                                            let mut next = *editor_preferences.peek();
                                            next.auto_indent = enabled;
                                            commit_editor_preferences(
                                                &mut editor_preferences,
                                                auto_indent_services.settings(),
                                                next,
                                                onstatus,
                                            );
                                        },
                                        span { "Auto first-line indent" }
                                        span { if editor_preferences.read().auto_indent { "On" } else { "Off" } }
                                    }
                                    div { class: "paragraph-layout-row",
                                        span { "Default first-line indent" }
                                        div { class: "paragraph-layout-stepper",
                                            button {
                                                aria_label: "Decrease default auto first-line indent",
                                                disabled: editor_preferences.read().auto_first_line_indent <= MIN_AUTO_FIRST_LINE_INDENT_POINTS,
                                                onclick: move |_| {
                                                    let mut next = *editor_preferences.peek();
                                                    next.auto_first_line_indent = next.auto_first_line_indent
                                                        .saturating_sub(AUTO_FIRST_LINE_INDENT_STEP_POINTS)
                                                        .max(MIN_AUTO_FIRST_LINE_INDENT_POINTS);
                                                    commit_editor_preferences(
                                                        &mut editor_preferences,
                                                        auto_indent_amount_decrease_services.settings(),
                                                        next,
                                                        onstatus,
                                                    );
                                                },
                                                "−"
                                            }
                                            span { "{editor_preferences.read().auto_first_line_indent} pt" }
                                            button {
                                                aria_label: "Increase default auto first-line indent",
                                                disabled: editor_preferences.read().auto_first_line_indent >= max_auto_first_line_indent_points(),
                                                onclick: move |_| {
                                                    let mut next = *editor_preferences.peek();
                                                    next.auto_first_line_indent = next.auto_first_line_indent
                                                        .saturating_add(AUTO_FIRST_LINE_INDENT_STEP_POINTS)
                                                        .min(max_auto_first_line_indent_points());
                                                    commit_editor_preferences(
                                                        &mut editor_preferences,
                                                        auto_indent_amount_increase_services.settings(),
                                                        next,
                                                        onstatus,
                                                    );
                                                },
                                                "+"
                                            }
                                        }
                                    }
                                    p { class: "paragraph-layout-hint", "Applies only when a new normal paragraph is created. Manual removal always wins." }
                                }
                                section { class: "paragraph-layout-section",
                                    button {
                                        class: "clear-typography-button",
                                        "data-live-editor-paragraph-layout-control": "true",
                                        onpointerdown: move |event| event.prevent_default(),
                                        onclick: move |_| apply_typography("clear_paragraph_layout", "all"),
                                        "Reset paragraph formatting"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "editor-popover-anchor block-style-anchor",
                        button {
                            id: "editor-block-style-trigger",
                            class: if *editor_popover.read() == Some(EditorPopover::BlockStyle) { "block-style-button active" } else { "block-style-button" },
                            title: "Style",
                            aria_label: "Style",
                            aria_controls: "editor-block-style-menu",
                            aria_haspopup: "menu",
                            aria_expanded: if *editor_popover.read() == Some(EditorPopover::BlockStyle) { "true" } else { "false" },
                            "data-live-editor-block-style": "true",
                            "data-transient-trigger": "true",
                            onpointerdown: move |event| event.prevent_default(),
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                let next = if *editor_popover.read() == Some(EditorPopover::BlockStyle) {
                                    None
                                } else {
                                    Some(EditorPopover::BlockStyle)
                                };
                                editor_popover.set(next);
                                if next.is_some() {
                                    refresh_editor_surface();
                                }
                            },
                            span { class: "block-style-label", "Style" }
                            span { aria_hidden: "true", "⌄" }
                        }
                        if *editor_popover.read() == Some(EditorPopover::BlockStyle) {
                            div {
                                id: "editor-block-style-menu",
                                class: "editor-popover block-style-popover",
                                role: "menu",
                                aria_label: "Style",
                                "data-transient-surface": "true",
                                "data-transient-kind": "menu",
                                "data-dismiss-outside": "true",
                                "data-return-focus": "editor-block-style-trigger",
                                "data-fallback-focus": "note-editor-main",
                                BlockStyleButton { label: "Normal text", title: "Convert to normal text", action: FormatAction::Paragraph, editor_popover }
                                BlockStyleButton { label: "Heading 1", title: "Convert to heading 1", action: FormatAction::Heading1, editor_popover }
                                BlockStyleButton { label: "Heading 2", title: "Convert to heading 2", action: FormatAction::Heading2, editor_popover }
                                BlockStyleButton { label: "Heading 3", title: "Convert to heading 3", action: FormatAction::Heading3, editor_popover }
                            }
                        }
                    }
                    span { class: "toolbar-divider", aria_hidden: "true" }
                    div { class: "editor-popover-anchor highlight-control",
                        FormatButton { label: "Highlight", title: "Apply or remove the current highlight colour", action: FormatAction::Highlight }
                        button {
                            id: "editor-highlight-palette-trigger",
                            class: if *editor_popover.read() == Some(EditorPopover::HighlightPalette) { "highlight-palette-trigger active" } else { "highlight-palette-trigger" },
                            title: "Choose highlight colour",
                            aria_label: "Choose highlight colour",
                            aria_controls: "editor-highlight-palette",
                            aria_haspopup: "menu",
                            aria_expanded: if *editor_popover.read() == Some(EditorPopover::HighlightPalette) { "true" } else { "false" },
                            "data-transient-trigger": "true",
                            onpointerdown: move |event| event.prevent_default(),
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                let next = if *editor_popover.read() == Some(EditorPopover::HighlightPalette) { None } else { Some(EditorPopover::HighlightPalette) };
                                editor_popover.set(next);
                                if next.is_some() {
                                    refresh_editor_surface();
                                }
                            },
                            "⌄"
                        }
                        if *editor_popover.read() == Some(EditorPopover::HighlightPalette) {
                            div {
                                id: "editor-highlight-palette",
                                class: "editor-popover highlight-palette-popover",
                                role: "menu",
                                aria_label: "Highlight colour",
                                "data-transient-surface": "true",
                                "data-transient-kind": "menu",
                                "data-dismiss-outside": "true",
                                "data-return-focus": "editor-highlight-palette-trigger",
                                "data-fallback-focus": "note-editor-main",
                                for (label, value) in [("None", "none"), ("Yellow", "yellow"), ("Green", "green"), ("Blue", "blue"), ("Pink", "pink"), ("Purple", "purple")] {
                                    button { class: "highlight-colour-choice", role: "menuitemradio", aria_checked: "false", "data-live-editor-highlight-color": value, onpointerdown: move |event| event.prevent_default(), onclick: move |_| { apply_typography("highlight_color", value); editor_popover.set(None); }, span { class: "highlight-colour-dot", "data-highlight-colour": value } span { "{label}" } }
                                }
                            }
                        }
                    }
                    span { class: "toolbar-divider", aria_hidden: "true" }
                    div {
                        id: "editor-list-tools-anchor",
                        class: "list-tools-anchor",
                        FormatButton { label: "•", title: "Toggle a bullet list", action: FormatAction::BulletList }
                        FormatButton { label: "1.", title: "Toggle a numbered list", action: FormatAction::NumberedList }
                        FormatButton { label: "☐", title: "Toggle a checklist", action: FormatAction::Checklist }
                        div {
                            id: "editor-list-context-tools",
                            class: "list-context-tools",
                            role: "toolbar",
                            aria_label: "List controls",
                            aria_hidden: "true",
                            hidden: true,
                            "data-live-editor-list-context": "true",
                            div { class: "editor-popover-anchor list-style-anchor",
                                button {
                                    id: "editor-list-style-trigger",
                                    class: if *editor_popover.read() == Some(EditorPopover::ListStyle) { "list-style-button active" } else { "list-style-button" },
                                    title: "Choose list marker style",
                                    aria_label: "List marker style",
                                    aria_controls: "editor-list-style-menu",
                                    aria_haspopup: "menu",
                                    aria_expanded: if *editor_popover.read() == Some(EditorPopover::ListStyle) { "true" } else { "false" },
                                    "data-live-editor-list-style": "true",
                                    "data-transient-trigger": "true",
                                    onpointerdown: move |event| event.prevent_default(),
                                    onmousedown: move |event| event.prevent_default(),
                                    onclick: move |_| {
                                        let next = if *editor_popover.read() == Some(EditorPopover::ListStyle) {
                                            None
                                        } else {
                                            Some(EditorPopover::ListStyle)
                                        };
                                        editor_popover.set(next);
                                        if next.is_some() {
                                            refresh_editor_surface();
                                        }
                                    },
                                    span { "data-live-editor-list-style-label": "true", "Style: Auto" }
                                    span { aria_hidden: "true", "⌄" }
                                }
                                if *editor_popover.read() == Some(EditorPopover::ListStyle) {
                                    div {
                                        id: "editor-list-style-menu",
                                        class: "editor-popover list-style-popover",
                                        role: "menu",
                                        aria_label: "List marker style",
                                        "data-transient-surface": "true",
                                        "data-transient-kind": "menu",
                                        "data-dismiss-outside": "true",
                                        "data-return-focus": "editor-list-style-trigger",
                                        "data-fallback-focus": "note-editor-main",
                                        ListStyleButton { label: "• Bullet", title: "Use round bullet markers", action: FormatAction::BulletStyleDisc, editor_popover }
                                        ListStyleButton { label: "— Dash", title: "Use dash markers", action: FormatAction::BulletStyleDash, editor_popover }
                                        ListStyleButton { label: "Auto", title: "Cycle marker style by nesting depth", action: FormatAction::OrderedStyleAuto, editor_popover }
                                        ListStyleButton { label: "1, 2, 3", title: "Use decimal numbering", action: FormatAction::OrderedStyleDecimal, editor_popover }
                                        ListStyleButton { label: "i, ii, iii", title: "Use lower-case Roman numbering", action: FormatAction::OrderedStyleLowerRoman, editor_popover }
                                        ListStyleButton { label: "a, b, c", title: "Use lower-case alphabetic numbering", action: FormatAction::OrderedStyleLowerAlpha, editor_popover }
                                        div { class: "list-style-separator", aria_hidden: "true", "data-live-editor-list-order-only": "true" }
                                        ListCommandButton { label: "Restart at 1", title: "Restart this ordered list at 1", action: FormatAction::OrderedRestart, editor_popover }
                                        ListCommandButton { label: "Continue numbering", title: "Continue from the preceding ordered list", action: FormatAction::OrderedContinue, editor_popover }
                                    }
                                }
                            }
                            CommandButton { label: "⇤ Outdent", title: "Decrease list indent (Shift+Tab)", action: FormatAction::Outdent }
                            CommandButton { label: "Indent ⇥", title: "Increase list indent (Tab)", action: FormatAction::Indent }
                        }
                    }
                    span { class: "toolbar-divider", aria_hidden: "true" }
                    FormatButton { label: "❝", title: "Toggle a quotation block", action: FormatAction::Quote }
                    CommandButton { label: "―", title: "Insert horizontal divider", action: FormatAction::HorizontalRule }
                    span { class: "toolbar-divider", aria_hidden: "true" }
                    div { class: "editor-popover-anchor link-popover-anchor",
                        button {
                            id: "editor-link-trigger",
                            class: "editor-link-bridge-trigger",
                            aria_label: "Add or edit link",
                            aria_hidden: "true",
                            tabindex: "-1",
                            aria_controls: "editor-link-popover",
                            aria_haspopup: "dialog",
                            aria_expanded: if *editor_popover.read() == Some(EditorPopover::Link) { "true" } else { "false" },
                            aria_pressed: "false",
                            "data-live-editor-format": "link",
                            "data-transient-trigger": "true",
                            onpointerdown: move |event| event.prevent_default(),
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                begin_link_popover(
                                    editor_popover,
                                    link_url,
                                    link_label,
                                    link_error,
                                    link_popover_publication_scope.clone(),
                                );
                            },
                        }
                        if *editor_popover.read() == Some(EditorPopover::Link) {
                            section {
                                id: "editor-link-popover",
                                class: "editor-popover link-popover",
                                role: "dialog",
                                aria_label: "Link",
                                "data-transient-surface": "true",
                                "data-transient-kind": "popover",
                                "data-dismiss-outside": "true",
                                "data-return-focus": "editor-insert-trigger",
                                "data-fallback-focus": "note-editor-main",
                                header { class: "popover-header compact",
                                    div {
                                        p { class: "eyebrow", "LINK" }
                                        h3 { "Link" }
                                    }
                                    button {
                                        class: "popover-close",
                                        aria_label: "Close link editor",
                                        "data-transient-close": "editor-link-popover",
                                        onclick: move |_| editor_popover.set(None),
                                        "×"
                                    }
                                }
                                p { class: "link-label-preview",
                                    span { "Label" }
                                    strong { "{link_label}" }
                                }
                                label { class: "popover-field",
                                    span { "URL" }
                                    input {
                                        id: "note-link-url-input",
                                        "data-initial-focus": "true",
                                        value: "{link_url}",
                                        placeholder: "https://example.com",
                                        aria_invalid: "{link_error.read().is_some()}",
                                        oninput: move |event| {
                                            link_url.set(event.value());
                                            link_error.set(None);
                                        },
                                        onkeydown: move |event| {
                                            if event.key() == Key::Enter {
                                                event.prevent_default();
                                                insert_link_from_popover(
                                                    link_url.read().clone(),
                                                    link_label.read().clone(),
                                                    editor_popover,
                                                    link_error,
                                                    onstatus,
                                                    link_enter_publication_scope.clone(),
                                                );
                                            }
                                        }
                                    }
                                }
                                if let Some(error) = link_error.read().clone() {
                                    p { class: "popover-error", role: "alert", "{error}" }
                                }
                                div { class: "popover-actions",
                                    button {
                                        class: "popover-primary-button",
                                        onclick: move |_| {
                                            insert_link_from_popover(
                                                link_url.read().clone(),
                                                link_label.read().clone(),
                                                editor_popover,
                                                link_error,
                                                onstatus,
                                                link_apply_publication_scope.clone(),
                                            );
                                        },
                                        "Apply link"
                                    }
                                }
                            }
                        }
                    }
                    button {
                        id: "editor-insert-trigger",
                        class: "format-button insert-menu-trigger",
                        title: "Insert",
                        aria_label: "Insert",
                        aria_controls: "rustnote-insert-menu",
                        aria_haspopup: "menu",
                        aria_expanded: "false",
                        onpointerdown: move |event| event.prevent_default(),
                        onmousedown: move |event| event.prevent_default(),
                        onclick: move |_| open_insert_menu(),
                        span { class: "insert-menu-trigger-label", "Insert" }
                        span { aria_hidden: "true", "＋" }
                    }
                }
                div { class: "editor-focus-toolbar-actions", aria_label: "Focus controls",
                    button {
                        class: "tool-button editor-focus-window-button",
                        title: "Toggle app full screen",
                        aria_label: "Toggle app full screen",
                        onclick: move |_| toggle_window_fullscreen(),
                        "Full Screen"
                    }
                    button {
                        class: "tool-button editor-focus-done-button",
                        title: "Exit editor focus",
                        aria_label: "Exit editor focus",
                        onclick: move |_| exit_editor_focus(),
                        "Done"
                    }
                }
            }
            }
        }
    }
}
