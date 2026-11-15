//! Shared context-menu controller and renderer.

use dioxus::prelude::*;

use super::entity_actions::{EntityAction, EntityActionEntry};

#[derive(Clone, Debug, PartialEq, Eq)]
enum ContextMenuOrigin {
    Pointer { x: i32, y: i32 },
    Anchor { element_id: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ContextMenuController {
    origin: Option<ContextMenuOrigin>,
}

impl ContextMenuController {
    pub(super) fn is_open(&self) -> bool {
        self.origin.is_some()
    }

    pub(super) fn close(&mut self) {
        self.origin = None;
    }

    pub(super) fn open_pointer(&mut self, x: i32, y: i32) {
        self.origin = Some(ContextMenuOrigin::Pointer { x, y });
    }

    pub(super) fn open_anchor(&mut self, element_id: impl Into<String>) {
        self.origin = Some(ContextMenuOrigin::Anchor {
            element_id: element_id.into(),
        });
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct EntityContextMenuView {
    pub id: String,
    pub label: String,
    pub return_focus_id: String,
    pub fallback_focus_id: String,
    pub controller: Signal<ContextMenuController>,
    pub entries: Vec<EntityActionEntry>,
}

#[derive(Clone, PartialEq)]
pub(super) struct EntityContextMenuActions {
    pub onaction: EventHandler<EntityAction>,
}

#[component]
pub(super) fn EntityContextMenu(
    view: EntityContextMenuView,
    actions: EntityContextMenuActions,
) -> Element {
    let EntityContextMenuView {
        id,
        label,
        return_focus_id,
        fallback_focus_id,
        mut controller,
        entries,
    } = view;
    let EntityContextMenuActions { onaction } = actions;
    let expanded_entry = use_signal(|| None::<usize>);
    let confirming = use_signal(|| None::<EntityAction>);
    let Some(origin) = controller.read().origin.clone() else {
        return rsx! {};
    };

    let (origin_x, origin_y, anchor_id) = match origin {
        ContextMenuOrigin::Pointer { x, y } => (x.to_string(), y.to_string(), String::new()),
        ContextMenuOrigin::Anchor { element_id } => (String::new(), String::new(), element_id),
    };

    rsx! {
        div {
            id: "{id}",
            class: "entity-context-menu",
            role: "menu",
            aria_label: "{label}",
            "data-context-menu": "true",
            "data-origin-x": "{origin_x}",
            "data-origin-y": "{origin_y}",
            "data-anchor-id": "{anchor_id}",
            "data-transient-surface": "true",
            "data-transient-kind": "menu",
            "data-dismiss-outside": "true",
            "data-return-focus": "{return_focus_id}",
            "data-fallback-focus": "{fallback_focus_id}",
            onkeydown: move |event| event.stop_propagation(),
            button {
                class: "entity-context-close-control",
                tabindex: "-1",
                aria_hidden: "true",
                "data-transient-close": "{id}",
                onclick: move |_| controller.write().close(),
            }
            for (index, entry) in entries.into_iter().enumerate() {
                EntityActionEntryView {
                    key: "{index}",
                    index,
                    entry,
                    expanded_entry,
                    confirming,
                    onactivate: EventHandler::new({
                        move |action: EntityAction| {
                            onaction.call(action);
                            controller.write().close();
                        }
                    }),
                }
            }
        }
    }
}

#[component]
fn EntityActionEntryView(
    index: usize,
    entry: EntityActionEntry,
    expanded_entry: Signal<Option<usize>>,
    confirming: Signal<Option<EntityAction>>,
    onactivate: EventHandler<EntityAction>,
) -> Element {
    let separator_before = entry.separator_before;
    if entry.children.is_empty() {
        return rsx! {
            if separator_before {
                div { class: "entity-context-separator", role: "separator" }
            }
            ContextMenuActionButton { entry, confirming, onactivate }
        };
    }

    let label = entry.label;
    let symbol = entry.symbol;
    let enabled = entry.enabled;
    let disabled_hint = entry.disabled_hint;
    let children = entry.children;
    let expanded = *expanded_entry.read() == Some(index);
    let button_class = if expanded {
        "entity-context-item expanded"
    } else {
        "entity-context-item"
    };
    let mut expanded_entry = expanded_entry;
    let mut confirming = confirming;

    rsx! {
        if separator_before {
            div { class: "entity-context-separator", role: "separator" }
        }
        div { class: "entity-context-submenu-shell",
            button {
                class: button_class,
                role: "menuitem",
                disabled: !enabled,
                title: disabled_hint.as_deref().unwrap_or(""),
                aria_haspopup: "menu",
                aria_expanded: "{expanded}",
                onclick: move |event| {
                    event.stop_propagation();
                    if !enabled {
                        return;
                    }
                    let next = if *expanded_entry.read() == Some(index) {
                        None
                    } else {
                        Some(index)
                    };
                    expanded_entry.set(next);
                    confirming.set(None);
                },
                span { class: "entity-context-symbol", aria_hidden: "true", "{symbol}" }
                span { class: "entity-context-label", "{label}" }
                span { class: "entity-context-chevron", aria_hidden: "true", "›" }
            }
            if expanded {
                div {
                    class: "entity-context-submenu",
                    role: "menu",
                    aria_label: "{label}",
                    for (child_index, child) in children.into_iter().enumerate() {
                        ContextMenuActionButton {
                            key: "{child_index}",
                            entry: child,
                            confirming,
                            onactivate,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ContextMenuActionButton(
    entry: EntityActionEntry,
    confirming: Signal<Option<EntityAction>>,
    onactivate: EventHandler<EntityAction>,
) -> Element {
    let Some(action) = entry.action else {
        return rsx! {};
    };
    let label = entry.label;
    let symbol = entry.symbol;
    let enabled = entry.enabled;
    let danger = entry.danger;
    let confirm_label = entry.confirm_label;
    let confirm_hint = entry.confirm_hint;
    let disabled_hint = entry.disabled_hint;
    let is_confirming = confirming.read().as_ref() == Some(&action);
    let visible_label = if is_confirming {
        confirm_label
            .as_deref()
            .unwrap_or(label.as_str())
            .to_string()
    } else {
        label
    };
    let class = if danger {
        "entity-context-item danger"
    } else {
        "entity-context-item"
    };
    let mut confirming = confirming;

    rsx! {
        button {
            class,
            role: "menuitem",
            disabled: !enabled,
            title: disabled_hint.as_deref().unwrap_or(""),
            onclick: move |event| {
                event.stop_propagation();
                if !enabled {
                    return;
                }
                if confirm_label.is_some() && !is_confirming {
                    confirming.set(Some(action.clone()));
                    return;
                }
                onactivate.call(action.clone());
            },
            span { class: "entity-context-symbol", aria_hidden: "true", "{symbol}" }
            span { class: "entity-context-label", "{visible_label}" }
            span {}
        }
        if is_confirming {
            if let Some(confirm_hint) = confirm_hint {
                span { class: "entity-context-warning", "{confirm_hint}" }
            }
        }
    }
}
