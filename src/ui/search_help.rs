use dioxus::prelude::*;

#[component]
pub(super) fn SearchHelpPopover(onclose: EventHandler<()>) -> Element {
    rsx! {
        section {
            id: "search-help-popover",
            class: "list-popover search-help-popover",
            role: "dialog",
            aria_label: "Search help",
            "data-transient-surface": "true",
            "data-transient-kind": "popover",
            "data-dismiss-outside": "true",
            "data-return-focus": "search-help-trigger",
            "data-fallback-focus": "note-list-main",
            header { class: "popover-header",
                div {
                    p { class: "eyebrow", "SEARCH" }
                    h3 { "Find exactly what you need" }
                }
                button {
                    class: "popover-close",
                    aria_label: "Close search help",
                    "data-transient-close": "search-help-popover",
                    onclick: move |_| onclose.call(()),
                    "×"
                }
            }
            p { class: "popover-intro",
                "Search titles, note text, tags, and attachment names. Use ⌘/Ctrl + Shift + F from anywhere; ⌘/Ctrl + F finds text inside the open note."
            }
            SearchExample { query: "ethics", meaning: "Words anywhere" }
            SearchExample { query: "\"constitutional morality\"", meaning: "Exact phrase" }
            SearchExample { query: "title:governance", meaning: "Title only" }
            SearchExample { query: "tag:personal", meaning: "A specific tag" }
            SearchExample { query: "file:budget", meaning: "Attachment filename" }
            SearchExample { query: "policy -draft", meaning: "Exclude a word" }
            SearchExample { query: "ethics OR integrity", meaning: "Either term" }
        }
    }
}

#[component]
pub(super) fn SearchExample(query: &'static str, meaning: &'static str) -> Element {
    rsx! {
        div { class: "search-example",
            code { "{query}" }
            span { "{meaning}" }
        }
    }
}
