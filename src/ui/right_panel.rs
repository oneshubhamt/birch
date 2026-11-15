use crate::domain::{Attachment, Note};
use crate::editor_document::{analyze_editor_state, extract_outline};
use dioxus::prelude::*;

use super::live_editor::{
    live_editor_focus_script, live_editor_remove_attachment_from_document_script,
};
use super::note_metadata::format_file_size;
use crate::application::attachments::{
    AttachmentFileCommand, AttachmentFileCommandOutcome, AttachmentService,
};
use crate::application::{
    executor::ApplicationExecutor, privacy::PrivacyPublicationService, settings::SettingsService,
};

pub(super) const DEFAULT_RIGHT_PANEL_WIDTH: i32 = 420;
pub(super) const MIN_RIGHT_PANEL_WIDTH: i32 = 280;
pub(super) const MAX_RIGHT_PANEL_WIDTH: i32 = 720;
const RIGHT_PANEL_KEYBOARD_STEP: i32 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RightPanelMode {
    Find,
    Outline,
    Backlinks,
    Hierarchy,
    Properties,
    Sources,
    Revisions,
    Attachments,
}

impl RightPanelMode {
    fn label(self) -> &'static str {
        match self {
            Self::Find => "Find",
            Self::Outline => "Outline",
            Self::Backlinks => "Backlinks",
            Self::Hierarchy => "Hierarchy",
            Self::Properties => "Properties",
            Self::Sources => "Sources",
            Self::Revisions => "Revisions",
            Self::Attachments => "Attachments",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Find => "⌕",
            Self::Outline => "☷",
            Self::Backlinks => "↗",
            Self::Hierarchy => "↳",
            Self::Properties => "ⓘ",
            Self::Sources => "§",
            Self::Revisions => "↶",
            Self::Attachments => "⌁",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RightPanelResize {
    pointer_id: i32,
    origin_x: i32,
    origin_width: i32,
}

pub(super) fn clamp_right_panel_width(width: i32) -> i32 {
    width.clamp(MIN_RIGHT_PANEL_WIDTH, MAX_RIGHT_PANEL_WIDTH)
}

pub(super) fn persist_right_panel_width(
    settings: SettingsService,
    width: i32,
    onstatus: EventHandler<String>,
) {
    let width = clamp_right_panel_width(width);
    spawn(async move {
        let result = ApplicationExecutor::read("right-panel preference", move || {
            settings
                .set_right_panel_width(width)
                .map_err(super::task_error::DataTaskError::from)
        })
        .await;
        if let Err(error) = result {
            onstatus.call(format!("Could not save right-panel width: {error}"));
        }
    });
}

pub(super) fn focus_live_editor() {
    spawn(async {
        let _ = document::eval(&live_editor_focus_script())
            .join::<bool>()
            .await;
    });
}

fn capture_right_panel_pointer(pointer_id: i32) {
    let script = format!(
        r#"
const splitter = document.getElementById("right-panel-splitter");
if (!splitter) return false;
try {{
    splitter.setPointerCapture({pointer_id});
    return splitter.hasPointerCapture({pointer_id});
}} catch (_) {{
    return false;
}}
"#
    );
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

fn release_right_panel_pointer(pointer_id: i32) {
    let script = format!(
        r#"
const splitter = document.getElementById("right-panel-splitter");
if (!splitter || !splitter.hasPointerCapture({pointer_id})) return false;
try {{
    splitter.releasePointerCapture({pointer_id});
    return true;
}} catch (_) {{
    return false;
}}
"#
    );
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

#[component]
pub(super) fn RightPanelShell(
    mode: RightPanelMode,
    panel_mode: Signal<Option<RightPanelMode>>,
    width: Signal<i32>,
    locked: bool,
    onmodechange: EventHandler<RightPanelMode>,
    onwidthcommit: EventHandler<i32>,
    children: Element,
) -> Element {
    let mut panel_mode = panel_mode;
    let mut width = width;
    let mut resize = use_signal(|| None::<RightPanelResize>);
    let panel_width = clamp_right_panel_width(*width.read());
    let panel_style = format!("--right-panel-width: {panel_width}px;");
    let resizing = resize.read().is_some();
    let mode_label = mode.label();
    let mode_class = match mode {
        RightPanelMode::Find => "right-panel-find-mode",
        RightPanelMode::Outline => "right-panel-outline",
        RightPanelMode::Backlinks => "right-panel-backlinks",
        RightPanelMode::Hierarchy => "right-panel-hierarchy",
        RightPanelMode::Properties => "right-panel-properties",
        RightPanelMode::Sources => "right-panel-sources",
        RightPanelMode::Revisions => "right-panel-revisions",
        RightPanelMode::Attachments => "right-panel-attachments",
    };
    let panel_class = if resizing {
        format!("right-panel {mode_class} resizing")
    } else {
        format!("right-panel {mode_class}")
    };

    rsx! {
        button {
            class: "right-panel-scrim",
            aria_label: "Close contextual panel",
            disabled: locked,
            onclick: move |_| {
                if !locked {
                    panel_mode.set(None);
                    focus_live_editor();
                }
            },
        }
        aside {
            id: "editor-right-panel",
            class: panel_class,
            style: "{panel_style}",
            aria_label: "Note context: {mode_label}",
            aria_busy: if locked { "true" } else { "false" },
            div {
                id: "right-panel-splitter",
                class: "right-panel-splitter",
                role: "separator",
                tabindex: "0",
                aria_label: "Resize contextual panel",
                aria_orientation: "vertical",
                aria_valuemin: "{MIN_RIGHT_PANEL_WIDTH}",
                aria_valuemax: "{MAX_RIGHT_PANEL_WIDTH}",
                aria_valuenow: "{panel_width}",
                title: "Drag to resize; use arrow keys; Home resets width",
                onkeydown: move |event| {
                    if locked {
                        return;
                    }
                    let next = match event.key() {
                        Key::ArrowLeft => Some(clamp_right_panel_width(
                            panel_width + RIGHT_PANEL_KEYBOARD_STEP,
                        )),
                        Key::ArrowRight => Some(clamp_right_panel_width(
                            panel_width - RIGHT_PANEL_KEYBOARD_STEP,
                        )),
                        Key::Home => Some(DEFAULT_RIGHT_PANEL_WIDTH),
                        _ => None,
                    };
                    if let Some(next) = next {
                        event.prevent_default();
                        width.set(next);
                        onwidthcommit.call(next);
                    }
                },
                onpointerdown: move |event| {
                    if locked {
                        return;
                    }
                    event.prevent_default();
                    let pointer_id = event.pointer_id();
                    resize.set(Some(RightPanelResize {
                        pointer_id,
                        origin_x: event.client_coordinates().x.round() as i32,
                        origin_width: panel_width,
                    }));
                    capture_right_panel_pointer(pointer_id);
                },
                onpointermove: move |event| {
                    let Some(active) = *resize.read() else {
                        return;
                    };
                    if active.pointer_id != event.pointer_id() {
                        return;
                    }
                    let current_x = event.client_coordinates().x.round() as i32;
                    let next = active
                        .origin_width
                        .saturating_add(active.origin_x.saturating_sub(current_x));
                    width.set(clamp_right_panel_width(next));
                },
                onpointerup: move |event| {
                    let Some(active) = *resize.read() else {
                        return;
                    };
                    if active.pointer_id != event.pointer_id() {
                        return;
                    }
                    release_right_panel_pointer(active.pointer_id);
                    resize.set(None);
                    let committed_width = clamp_right_panel_width(*width.peek());
                    onwidthcommit.call(committed_width);
                },
                onpointercancel: move |event| {
                    let Some(active) = *resize.read() else {
                        return;
                    };
                    if active.pointer_id != event.pointer_id() {
                        return;
                    }
                    release_right_panel_pointer(active.pointer_id);
                    resize.set(None);
                    let committed_width = clamp_right_panel_width(*width.peek());
                    onwidthcommit.call(committed_width);
                },
            }

            header { class: "right-panel-header",
                div {
                    p { class: "eyebrow", "NOTE CONTEXT" }
                    h2 { "{mode_label}" }
                }
                button {
                    class: "right-panel-close",
                    title: "Close contextual panel",
                    aria_label: "Close contextual panel",
                    disabled: locked,
                    onclick: move |_| {
                        if !locked {
                            panel_mode.set(None);
                            focus_live_editor();
                        }
                    },
                    "×"
                }
            }

            nav { class: "right-panel-tabs", aria_label: "Contextual panel sections",
                for candidate in [
                    RightPanelMode::Find,
                    RightPanelMode::Outline,
                    RightPanelMode::Backlinks,
                    RightPanelMode::Hierarchy,
                    RightPanelMode::Properties,
                    RightPanelMode::Sources,
                    RightPanelMode::Revisions,
                    RightPanelMode::Attachments,
                ] {
                    {
                        let candidate_label = candidate.label();
                        let candidate_symbol = candidate.symbol();
                        rsx! {
                            button {
                                key: "{candidate_label}",
                                class: if candidate == mode { "right-panel-tab active" } else { "right-panel-tab" },
                                aria_pressed: if candidate == mode { "true" } else { "false" },
                                disabled: locked,
                                onclick: move |_| {
                                    if !locked && candidate != mode {
                                        onmodechange.call(candidate);
                                    }
                                },
                                span { aria_hidden: "true", "{candidate_symbol}" }
                                span { "{candidate_label}" }
                            }
                        }
                    }
                }
            }

            div { class: "right-panel-content",
                {children}
            }
        }
    }
}

#[component]
pub(super) fn OutlinePanel(editor_state: std::sync::Arc<String>) -> Element {
    let headings = extract_outline(editor_state.as_str()).unwrap_or_default();

    rsx! {
        section { class: "outline-panel", aria_label: "Note outline",
            if headings.is_empty() {
                div { class: "right-panel-empty",
                    strong { "No headings yet" }
                    span { "Use heading styles in the note to build an outline." }
                }
            } else {
                nav { class: "outline-list", aria_label: "Headings in this note",
                    for (index, heading) in headings.into_iter().enumerate() {
                        {
                            let level = heading.level;
                            let text = heading.text;
                            let indentation = 9 + usize::from(level.saturating_sub(1)) * 13;
                            rsx! {
                                button {
                                    key: "{index}",
                                    class: "outline-item",
                                    style: "padding-left: {indentation}px;",
                                    title: "{text}",
                                    onclick: move |_| scroll_to_outline_heading(index),
                                    span { class: "outline-level", "H{level}" }
                                    span { class: "outline-text", "{text}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn scroll_to_outline_heading(index: usize) {
    let script = format!(
        r#"
const root = document.querySelector('#note-prosemirror-editor .ProseMirror');
if (!root) return false;
const headings = root.querySelectorAll('h1, h2, h3, h4, h5, h6');
const target = headings.item({index});
if (!target) return false;
target.scrollIntoView({{ behavior: 'smooth', block: 'start' }});
root.focus({{ preventScroll: true }});
return true;
"#
    );
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

#[derive(Clone, PartialEq)]
pub(super) struct AttachmentsPanelView {
    pub attachment_service: AttachmentService,
    pub draft: Signal<Note>,
    pub attachments: Signal<Vec<Attachment>>,
    pub attachment_busy: bool,
    pub editing_available: bool,
    pub privacy_epoch: super::privacy::UiPrivacyEpoch,
    pub privacy: PrivacyPublicationService,
    pub note_generation: Signal<u64>,
}

#[derive(Clone, PartialEq)]
pub(super) struct AttachmentsPanelActions {
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn AttachmentsPanel(
    view: AttachmentsPanelView,
    actions: AttachmentsPanelActions,
) -> Element {
    let AttachmentsPanelView {
        attachment_service,
        draft,
        attachments,
        attachment_busy,
        editing_available,
        privacy_epoch,
        privacy,
        note_generation,
    } = view;
    let AttachmentsPanelActions { onstatus } = actions;
    let note_read_only = draft.read().trashed || draft.read().protection.is_locked();
    let referenced_ids = analyze_editor_state(draft.read().editor_state())
        .map(|analysis| {
            analysis
                .attachment_ids
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let attachment_items = attachments
        .read()
        .iter()
        .filter(|attachment| referenced_ids.contains(&attachment.id))
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        section { class: "attachments-panel", aria_label: "Note attachments",
            div { class: "right-panel-summary",
                strong {
                    if attachment_items.len() == 1 {
                        "1 attachment"
                    } else {
                        "{attachment_items.len()} attachments"
                    }
                }
                span { "Files referenced by this note and managed by Rustnote." }
            }
            if attachment_items.is_empty() {
                div { class: "right-panel-empty",
                    strong { "No attachments" }
                    span { "Insert images, PDFs, audio, video, or files from the editor." }
                }
            } else {
                div { class: "right-panel-attachment-list",
                    for attachment in attachment_items {
                        AttachmentPanelItem {
                            key: "{attachment.id}",
                            view: AttachmentPanelItemView {
                                attachment_service: attachment_service.clone(),
                                attachment,
                                read_only: note_read_only,
                                operation_busy: attachment_busy,
                                editing_available,
                                privacy_epoch,
                                privacy: privacy.clone(),
                                note_generation,
                            },
                            actions: AttachmentPanelItemActions { onstatus },
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct AttachmentPanelItemView {
    attachment_service: AttachmentService,
    attachment: Attachment,
    read_only: bool,
    operation_busy: bool,
    editing_available: bool,
    privacy_epoch: super::privacy::UiPrivacyEpoch,
    privacy: PrivacyPublicationService,
    note_generation: Signal<u64>,
}

#[derive(Clone, PartialEq)]
struct AttachmentPanelItemActions {
    onstatus: EventHandler<String>,
}

#[component]
fn AttachmentPanelItem(
    view: AttachmentPanelItemView,
    actions: AttachmentPanelItemActions,
) -> Element {
    use super::context_menu::{
        ContextMenuController, EntityContextMenu, EntityContextMenuActions, EntityContextMenuView,
    };
    use super::entity_actions::{
        EntityAction, EntityActionContext, EntityActionRegistry, EntityActionState,
        EntityActionTargets, EntitySurface,
    };

    let AttachmentPanelItemView {
        attachment_service,
        attachment,
        read_only,
        operation_busy,
        editing_available,
        privacy_epoch,
        privacy,
        note_generation,
    } = view;
    let AttachmentPanelItemActions { onstatus } = actions;
    let mut context_menu = use_signal(ContextMenuController::default);
    let action_busy = use_signal(|| false);
    let busy = operation_busy || !editing_available || *action_busy.read();
    let item_id = format!("attachment-panel-item-{}", attachment.id);
    let trigger_id = format!("attachment-panel-menu-trigger-{}", attachment.id);
    let menu_id = format!("attachment-panel-menu-{}", attachment.id);
    let action_context = EntityActionContext {
        surface: EntitySurface::Attachment,
        entity_id: attachment.id.clone(),
        state: EntityActionState {
            read_only,
            busy,
            ..EntityActionState::default()
        },
    };
    let menu_entries =
        EntityActionRegistry::entries(&action_context, &EntityActionTargets::default());
    let open_privacy = privacy.clone();
    let menu_privacy = privacy;

    rsx! {
        article {
            id: "{item_id}",
            class: "right-panel-attachment",
            tabindex: "0",
            aria_label: "Attachment {attachment.file_name}",
            "data-context-long-press": "true",
            "aria-keyshortcuts": "Shift+F10",
            oncontextmenu: {
                let item_id = item_id.clone();
                move |event: MouseEvent| {
                    event.prevent_default();
                    event.stop_propagation();
                    let coordinates = event.client_coordinates();
                    let x = coordinates.x.round() as i32;
                    let y = coordinates.y.round() as i32;
                    if x == 0 && y == 0 {
                        context_menu.write().open_anchor(item_id.clone());
                    } else {
                        context_menu.write().open_pointer(x, y);
                    }
                }
            },
            onkeydown: {
                let item_id = item_id.clone();
                move |event: KeyboardEvent| {
                    if !event.is_composing()
                        && event.key() == Key::F10
                        && event.modifiers().shift()
                    {
                        event.prevent_default();
                        event.stop_propagation();
                        context_menu.write().open_anchor(item_id.clone());
                    }
                }
            },
            div { class: "right-panel-attachment-copy",
                strong { title: "{attachment.sha256}", "{attachment.file_name}" }
                span {
                    "{attachment_kind(&attachment)} · {format_file_size(attachment.byte_size)}"
                }
            }
            div { class: "right-panel-attachment-actions",
                button {
                    class: "attachment-action",
                    title: "Open attachment",
                    disabled: busy,
                    onclick: {
                        let attachment_service = attachment_service.clone();
                        let attachment = attachment.clone();
                        move |_| dispatch_attachment_action(
                            EntityAction::Open,
                            AttachmentActionContext {
                                attachment_service: attachment_service.clone(),
                                attachment: attachment.clone(),
                                busy: action_busy,
                                onstatus,
                                privacy_epoch,
                                privacy: open_privacy.clone(),
                                note_generation,
                            },
                        )
                    },
                    "Open"
                }
                button {
                    id: "{trigger_id}",
                    class: if context_menu.read().is_open() {
                        "attachment-action entity-action-trigger active"
                    } else {
                        "attachment-action entity-action-trigger"
                    },
                    title: "Attachment actions",
                    aria_label: "Open actions for {attachment.file_name}",
                    aria_controls: "{menu_id}",
                    aria_haspopup: "menu",
                    aria_expanded: "{context_menu.read().is_open()}",
                    disabled: busy,
                    "data-transient-trigger": "true",
                    onclick: {
                        let trigger_id = trigger_id.clone();
                        move |event| {
                            event.stop_propagation();
                            if context_menu.read().is_open() {
                                context_menu.write().close();
                            } else {
                                context_menu.write().open_anchor(trigger_id.clone());
                            }
                        }
                    },
                    "•••"
                }
            }
            if context_menu.read().is_open() {
                EntityContextMenu {
                    view: EntityContextMenuView {
                        id: menu_id.clone(),
                        label: format!("Actions for {}", attachment.file_name),
                        return_focus_id: item_id.clone(),
                        fallback_focus_id: "editor-right-panel".to_string(),
                        controller: context_menu,
                        entries: menu_entries,
                    },
                    actions: EntityContextMenuActions {
                        onaction: EventHandler::new({
                            let attachment_service = attachment_service.clone();
                            let attachment = attachment.clone();
                            move |action| dispatch_attachment_action(
                                action,
                                AttachmentActionContext {
                                    attachment_service: attachment_service.clone(),
                                    attachment: attachment.clone(),
                                    busy: action_busy,
                                    onstatus,
                                    privacy_epoch,
                                    privacy: menu_privacy.clone(),
                                    note_generation,
                                },
                            )
                        }),
                    },
                }
            }
        }
    }
}

struct AttachmentActionContext {
    attachment_service: AttachmentService,
    attachment: Attachment,
    busy: Signal<bool>,
    onstatus: EventHandler<String>,
    privacy_epoch: super::privacy::UiPrivacyEpoch,
    privacy: PrivacyPublicationService,
    note_generation: Signal<u64>,
}

fn dispatch_attachment_action(
    action: super::entity_actions::EntityAction,
    context: AttachmentActionContext,
) {
    let AttachmentActionContext {
        attachment_service,
        attachment,
        mut busy,
        onstatus,
        privacy_epoch,
        privacy,
        note_generation,
    } = context;
    use super::entity_actions::EntityAction;

    if *busy.read() {
        return;
    }
    let Some(publication_token) =
        privacy_epoch.capture_note_publication(&privacy, &attachment.note_id, note_generation)
    else {
        return;
    };
    let file_command = match action {
        EntityAction::Open => Some(AttachmentFileCommand::Open),
        EntityAction::SaveAttachmentAs => Some(AttachmentFileCommand::SaveAs),
        EntityAction::RevealAttachment => Some(AttachmentFileCommand::Reveal),
        _ => None,
    };
    if let Some(command) = file_command {
        busy.set(true);
        spawn(async move {
            let result = attachment_service
                .execute_file_command(attachment, command)
                .await;
            busy.set(false);
            match result {
                Ok(AttachmentFileCommandOutcome::Completed) => {
                    publication_token.publish(|| {
                        onstatus.call(command.success_message().to_string());
                    });
                }
                Ok(AttachmentFileCommandOutcome::Cancelled) => {}
                Err(error) => {
                    publication_token.publish(|| {
                        onstatus.call(format!(
                            "Could not {} attachment: {error}",
                            command.failure_verb()
                        ));
                    });
                }
            }
        });
        return;
    }

    if action == EntityAction::RemoveAttachmentFromNote {
        busy.set(true);
        let attachment_id = attachment.id.clone();
        let attachment_name = attachment.file_name.clone();
        spawn(async move {
            let Some(_publication_lease) = publication_token.begin_publication() else {
                busy.set(false);
                return;
            };
            let script = live_editor_remove_attachment_from_document_script(&attachment_id);
            let removed = document::eval(&script)
                .join::<bool>()
                .await
                .unwrap_or(false);
            busy.set(false);
            if removed {
                onstatus.call(format!("Removed {attachment_name} from note"));
            } else {
                onstatus.call(format!(
                    "Could not remove {attachment_name}; the attachment is no longer present in the editor"
                ));
            }
        });
    }
}

fn attachment_kind(attachment: &Attachment) -> &'static str {
    AttachmentService::display_kind(attachment)
}
