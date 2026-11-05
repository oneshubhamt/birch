use dioxus::prelude::*;

#[component]
pub(super) fn ShortcutsDialog(onclose: EventHandler<()>) -> Element {
    rsx! {
                    div {
            class: "shortcut-backdrop",
            "data-transient-close": "shortcuts-dialog",
            onclick: move |_| onclose.call(()),
            section {
                id: "shortcuts-dialog",
                class: "shortcut-modal",
                role: "dialog",
                aria_modal: "true",
                aria_label: "Keyboard shortcuts",
                "data-transient-surface": "true",
                "data-transient-kind": "dialog",
                "data-dismiss-outside": "true",
                "data-return-focus": "keyboard-shortcuts-trigger",
                "data-fallback-focus": "note-list-main note-editor-main",
                onclick: move |event| event.stop_propagation(),
                header {
                    div {
                        p { class: "eyebrow", "KEYBOARD" }
                        h2 { "Shortcuts" }
                    }
                    button {
                        class: "shortcut-close",
                        title: "Close keyboard shortcuts",
                        aria_label: "Close keyboard shortcuts",
                        "data-initial-focus": "true",
                        "data-transient-close": "shortcuts-dialog",
                        onclick: move |_| onclose.call(()),
                        "×"
                    }
                }
                ShortcutRow { keys: "⌘/Ctrl N", action: "Create a new note" }
                ShortcutRow { keys: "⌘/Ctrl F", action: "Find within the selected note" }
                ShortcutRow { keys: "⌘/Ctrl K", action: "Add or edit a link" }
                ShortcutRow { keys: "⌘/Ctrl ⇧ F", action: "Search the library" }
                ShortcutRow { keys: "Enter / ⇧Enter", action: "Next or previous in-note match" }
                ShortcutRow { keys: "⌘/Ctrl V", action: "Paste with supported rich formatting" }
                ShortcutRow { keys: "⌘/Ctrl ⇧ V", action: "Paste as plain text" }
                ShortcutRow { keys: "⌘/Ctrl S", action: "Save immediately" }
                ShortcutRow { keys: "⌘/Ctrl ⇧ P", action: "Pin or unpin selected note" }
                ShortcutRow { keys: "⌘/Ctrl 1–4", action: "Open Notes, Pinned, Archive or Trash" }
                ShortcutRow { keys: "Esc", action: "Close the active panel or clear search" }
                ShortcutRow { keys: "Double-click divider", action: "Reset a column width" }
            }
        }

    }
}

#[component]
pub(super) fn ShortcutRow(keys: &'static str, action: &'static str) -> Element {
    rsx! {
        div { class: "shortcut-row",
            kbd { "{keys}" }
            span { "{action}" }
        }
    }
}
