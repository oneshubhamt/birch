//! Pure user-interface helpers that can be tested without launching a WebView.

use std::sync::Arc;

use crate::application::executor::ApplicationExecutor;
use dioxus::prelude::*;

use crate::domain::NoteId;

use crate::application::settings::SettingsService;

pub const DEFAULT_SIDEBAR_WIDTH: i32 = 238;
pub const DEFAULT_NOTE_LIST_WIDTH: i32 = 340;
pub const MIN_SIDEBAR_WIDTH: i32 = 190;
pub const MAX_SIDEBAR_WIDTH: i32 = 380;
pub const MIN_NOTE_LIST_WIDTH: i32 = 280;
pub const MAX_NOTE_LIST_WIDTH: i32 = 620;
pub const MIN_EDITOR_WIDTH: i32 = 360;
pub const SPLITTER_WIDTH: i32 = 5;
pub const COMPACT_LAYOUT_BREAKPOINT: i32 = 760;
pub const SIDEBAR_OVERLAY_BREAKPOINT: i32 = 1120;
pub const KEYBOARD_RESIZE_STEP: i32 = 16;
const LAYOUT_SAVE_DEBOUNCE_MILLIS: u64 = 250;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeState {
    #[default]
    Idle,
    Sidebar {
        pointer_id: i32,
        fixed_note_list_width: i32,
    },
    NoteListPending {
        pointer_id: i32,
        fixed_sidebar_width: i32,
    },
    NoteList {
        pointer_id: i32,
        origin_x: i32,
        fixed_sidebar_width: i32,
    },
}

impl ResizeState {
    pub fn begin_sidebar(pointer_id: i32, fixed_note_list_width: i32) -> Self {
        Self::Sidebar {
            pointer_id,
            fixed_note_list_width: clamp_note_list_width(fixed_note_list_width),
        }
    }

    pub fn begin_note_list(pointer_id: i32, fixed_sidebar_width: i32) -> Self {
        Self::NoteListPending {
            pointer_id,
            fixed_sidebar_width: clamp_sidebar_width(fixed_sidebar_width),
        }
    }

    pub fn activate_note_list(self, pointer_id: i32, origin_x: i32) -> Self {
        match self {
            Self::NoteListPending {
                pointer_id: pending_id,
                fixed_sidebar_width,
            } if pending_id == pointer_id => Self::NoteList {
                pointer_id,
                origin_x,
                fixed_sidebar_width,
            },
            state => state,
        }
    }

    pub fn finish(self) -> Self {
        Self::Idle
    }

    pub fn is_active(self) -> bool {
        self != Self::Idle
    }

    pub fn pointer_id(self) -> Option<i32> {
        match self {
            Self::Idle => None,
            Self::Sidebar { pointer_id, .. }
            | Self::NoteListPending { pointer_id, .. }
            | Self::NoteList { pointer_id, .. } => Some(pointer_id),
        }
    }

    pub fn matches_pointer(self, pointer_id: i32) -> bool {
        self.pointer_id() == Some(pointer_id)
    }
}

pub fn clamp_sidebar_width(width: i32) -> i32 {
    width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
}

pub fn clamp_note_list_width(width: i32) -> i32 {
    width.clamp(MIN_NOTE_LIST_WIDTH, MAX_NOTE_LIST_WIDTH)
}

pub fn sidebar_width_from_pointer(pointer_x: i32, viewport_width: i32) -> i32 {
    sidebar_width_for_viewport(pointer_x, viewport_width, MIN_NOTE_LIST_WIDTH)
}

pub fn sidebar_width_for_viewport(
    pointer_x: i32,
    viewport_width: i32,
    fixed_note_list_width: i32,
) -> i32 {
    let viewport_max = viewport_width
        .saturating_sub(clamp_note_list_width(fixed_note_list_width))
        .saturating_sub(MIN_EDITOR_WIDTH)
        .saturating_sub(SPLITTER_WIDTH * 2);
    clamp_sidebar_width(pointer_x.min(viewport_max))
}

pub fn note_list_width_from_pointer(pointer_x: i32, note_list_left: i32) -> i32 {
    clamp_note_list_width(pointer_x.saturating_sub(note_list_left))
}

pub fn note_list_width_for_viewport(
    pointer_x: i32,
    note_list_left: i32,
    viewport_width: i32,
) -> i32 {
    let viewport_max = viewport_width
        .saturating_sub(note_list_left)
        .saturating_sub(SPLITTER_WIDTH)
        .saturating_sub(MIN_EDITOR_WIDTH);
    note_list_width_from_pointer(pointer_x, note_list_left).min(clamp_note_list_width(viewport_max))
}

pub fn resize_sidebar_with_keyboard(current: i32, direction: i32) -> i32 {
    clamp_sidebar_width(current + direction.signum() * KEYBOARD_RESIZE_STEP)
}

pub fn resize_note_list_with_keyboard(current: i32, direction: i32) -> i32 {
    clamp_note_list_width(current + direction.signum() * KEYBOARD_RESIZE_STEP)
}

pub fn reset_sidebar_width() -> i32 {
    DEFAULT_SIDEBAR_WIDTH
}

pub fn reset_note_list_width() -> i32 {
    DEFAULT_NOTE_LIST_WIDTH
}

pub fn effective_layout_widths(
    viewport_width: i32,
    preferred_sidebar_width: i32,
    preferred_note_list_width: i32,
) -> (i32, i32) {
    let preferred_sidebar_width = clamp_sidebar_width(preferred_sidebar_width);
    let preferred_note_list_width = clamp_note_list_width(preferred_note_list_width);

    if viewport_width <= COMPACT_LAYOUT_BREAKPOINT {
        return (preferred_sidebar_width, preferred_note_list_width);
    }

    if viewport_width <= SIDEBAR_OVERLAY_BREAKPOINT {
        let available_note_list_width = viewport_width
            .saturating_sub(SPLITTER_WIDTH)
            .saturating_sub(MIN_EDITOR_WIDTH);
        return (
            preferred_sidebar_width,
            preferred_note_list_width.min(clamp_note_list_width(available_note_list_width)),
        );
    }

    let available_columns_width = viewport_width
        .saturating_sub(SPLITTER_WIDTH * 2)
        .saturating_sub(MIN_EDITOR_WIDTH);
    let minimum_columns_width = MIN_SIDEBAR_WIDTH + MIN_NOTE_LIST_WIDTH;

    if preferred_sidebar_width + preferred_note_list_width <= available_columns_width {
        return (preferred_sidebar_width, preferred_note_list_width);
    }

    let extra_budget = available_columns_width
        .saturating_sub(minimum_columns_width)
        .max(0);
    let sidebar_extra = preferred_sidebar_width - MIN_SIDEBAR_WIDTH;
    let note_list_extra = preferred_note_list_width - MIN_NOTE_LIST_WIDTH;
    let desired_extra = sidebar_extra + note_list_extra;

    if desired_extra <= 0 {
        return (MIN_SIDEBAR_WIDTH, MIN_NOTE_LIST_WIDTH);
    }

    let allocated_sidebar_extra = extra_budget * sidebar_extra / desired_extra;
    let allocated_note_list_extra = extra_budget - allocated_sidebar_extra;

    (
        clamp_sidebar_width(MIN_SIDEBAR_WIDTH + allocated_sidebar_extra),
        clamp_note_list_width(MIN_NOTE_LIST_WIDTH + allocated_note_list_extra),
    )
}

pub fn layout_widths_for_resize(
    viewport_width: i32,
    preferred_sidebar_width: i32,
    preferred_note_list_width: i32,
    state: ResizeState,
) -> (i32, i32) {
    match state {
        ResizeState::Sidebar {
            fixed_note_list_width,
            ..
        } => (
            sidebar_width_for_viewport(
                preferred_sidebar_width,
                viewport_width,
                fixed_note_list_width,
            ),
            fixed_note_list_width,
        ),
        ResizeState::NoteListPending {
            fixed_sidebar_width,
            ..
        }
        | ResizeState::NoteList {
            fixed_sidebar_width,
            ..
        } => {
            let note_list_left = if viewport_width <= SIDEBAR_OVERLAY_BREAKPOINT {
                0
            } else {
                fixed_sidebar_width + SPLITTER_WIDTH
            };
            (
                fixed_sidebar_width,
                note_list_width_for_viewport(
                    note_list_left + preferred_note_list_width,
                    note_list_left,
                    viewport_width,
                ),
            )
        }
        ResizeState::Idle => effective_layout_widths(
            viewport_width,
            preferred_sidebar_width,
            preferred_note_list_width,
        ),
    }
}

pub(super) fn focus_search_input() {
    spawn(async {
        let _ = document::eval(
            "const input = document.getElementById('note-search-input'); if (input) { input.focus(); input.select(); return true; } return false;",
        )
        .join::<bool>()
        .await;
    });
}

pub(super) fn focus_note_find_input() {
    spawn(async {
        let _ = document::eval(
            "const input = document.getElementById('note-find-input'); if (input) { input.focus(); input.select(); return true; } return false;",
        )
        .join::<bool>()
        .await;
    });
}

pub(super) fn capture_resize_pointer(element_id: &'static str, pointer_id: i32) {
    let script = format!(
        r#"
const splitter = document.getElementById({element_id:?});
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

pub(super) fn begin_note_list_resize(
    pointer_id: i32,
    fallback_origin_x: i32,
    mut resize_state: Signal<ResizeState>,
) {
    let script = format!(
        r#"
const splitter = document.getElementById("notes-splitter");
const panel = document.getElementById("note-list-main");
if (splitter) {{
    try {{ splitter.setPointerCapture({pointer_id}); }} catch (_) {{}}
}}
return panel ? panel.getBoundingClientRect().left : {fallback_origin_x};
"#
    );
    spawn(async move {
        let origin_x = document::eval(&script)
            .join::<f64>()
            .await
            .map(|left| left.round() as i32)
            .unwrap_or(fallback_origin_x);
        let current = *resize_state.read();
        let next = current.activate_note_list(pointer_id, origin_x);
        if next != current {
            resize_state.set(next);
        }
    });
}

pub(super) fn release_resize_pointer(element_id: &'static str, pointer_id: i32) {
    let script = format!(
        r#"
const splitter = document.getElementById({element_id:?});
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

pub(super) fn finish_resize(
    settings: SettingsService,
    mut resize_state: Signal<ResizeState>,
    sidebar_width: Signal<i32>,
    note_list_width: Signal<i32>,
    layout_save_generation: Signal<u64>,
    persistence_lock: Arc<tokio::sync::Mutex<()>>,
    status: Signal<String>,
) {
    let current = *resize_state.read();
    if !current.is_active() {
        return;
    }

    if let Some(pointer_id) = current.pointer_id() {
        let element_id = match current {
            ResizeState::Sidebar { .. } => "sidebar-splitter",
            ResizeState::NoteListPending { .. } | ResizeState::NoteList { .. } => "notes-splitter",
            ResizeState::Idle => return,
        };
        release_resize_pointer(element_id, pointer_id);
    }

    resize_state.set(current.finish());
    schedule_layout_widths_save(
        settings,
        *sidebar_width.read(),
        *note_list_width.read(),
        layout_save_generation,
        persistence_lock,
        status,
    );
}

pub(super) fn schedule_layout_widths_save(
    settings: SettingsService,
    sidebar_width: i32,
    note_list_width: i32,
    mut generation: Signal<u64>,
    persistence_lock: Arc<tokio::sync::Mutex<()>>,
    status: Signal<String>,
) {
    let next_generation = (*generation.read()).wrapping_add(1);
    generation.set(next_generation);
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(
            LAYOUT_SAVE_DEBOUNCE_MILLIS,
        ))
        .await;
        let _persistence_guard = persistence_lock.lock().await;
        if *generation.read() != next_generation {
            return;
        }
        let result = ApplicationExecutor::read("layout preference", move || {
            settings
                .set_layout_widths(sidebar_width, note_list_width)
                .map_err(super::task_error::DataTaskError::from)
        })
        .await;
        if *generation.read() != next_generation {
            return;
        }
        if let Err(error) = result {
            let mut status = status;
            status.set(format!("Could not save panel widths: {error}"));
        }
    });
}

pub(super) fn focus_editor_panel() {
    focus_panel("note-editor-main");
}

pub(super) fn focus_note_list_panel() {
    focus_panel("note-list-scroll");
}

fn selected_note_focus_script(note_id: &NoteId) -> String {
    let note_id = serde_json::to_string(note_id.as_str()).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"
const noteId = {note_id};
requestAnimationFrame(() => {{
    if (window.matchMedia("(max-width: 760px)").matches) {{
        const editor = document.getElementById("note-editor-main");
        if (editor) editor.focus({{ preventScroll: true }});
        return;
    }}
    for (const card of document.querySelectorAll('.note-card[data-note-id]')) {{
        if (card.dataset.noteId === noteId) {{
            card.focus({{ preventScroll: true }});
            break;
        }}
    }}
}});
return true;
"#
    )
}

pub(super) fn focus_selected_note_surface(note_id: &NoteId) {
    let script = selected_note_focus_script(note_id);
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

#[derive(Clone, Copy)]
pub(super) enum NotebookTreeNavigation {
    Next,
    Previous,
    First,
    Last,
}

fn notebook_tree_focus_script(navigation: NotebookTreeNavigation) -> String {
    let movement = match navigation {
        NotebookTreeNavigation::Next => "next",
        NotebookTreeNavigation::Previous => "previous",
        NotebookTreeNavigation::First => "first",
        NotebookTreeNavigation::Last => "last",
    };
    format!(
        r#"
const tree = document.getElementById("notebook-tree-scroll");
if (!tree) return false;
const items = Array.from(tree.querySelectorAll('.notebook-link:not([disabled])'));
if (items.length === 0) return false;
let index = items.indexOf(document.activeElement);
if (index < 0) {{
    const selected = items.findIndex((item) => item.getAttribute('aria-selected') === 'true');
    index = selected >= 0 ? selected : 0;
}}
let target = index;
switch ({movement:?}) {{
    case "next": target = Math.min(items.length - 1, index + 1); break;
    case "previous": target = Math.max(0, index - 1); break;
    case "first": target = 0; break;
    case "last": target = items.length - 1; break;
}}
items[target].focus({{ preventScroll: true }});
items[target].scrollIntoView({{ block: "nearest" }});
return true;
"#
    )
}

pub(super) fn focus_notebook_tree_item(navigation: NotebookTreeNavigation) {
    let script = notebook_tree_focus_script(navigation);
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn focus_panel(id: &'static str) {
    let script = format!(
        "setTimeout(() => {{ const panel = document.getElementById('{id}'); if (panel) panel.focus({{ preventScroll: true }}); }}, 0); return true;"
    );
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn focus_title_input() {
    spawn(async {
        let _ = document::eval(
            "setTimeout(() => { const input = document.getElementById('note-title-input'); if (input) { input.focus(); input.select(); } }, 0); return true;",
        )
        .join::<bool>()
        .await;
    });
}

pub(super) const TRANSIENT_UI_SCRIPT: &str = r#"
if (window.__rustnoteTransientCleanup) {
    window.__rustnoteTransientCleanup();
}

const surfaceSelector = '[data-transient-surface="true"]';
const focusableSelector = [
    'a[href]',
    'button:not([disabled])',
    'input:not([disabled])',
    'select:not([disabled])',
    'textarea:not([disabled])',
    '[tabindex]:not([tabindex="-1"])'
].join(',');

let currentSurface = null;
let currentOpener = null;
let pendingOpener = null;
let pendingTargetId = null;
let switchingSurface = false;
let tooltipTarget = null;
let pendingTooltipTarget = null;
let tooltipTimer = null;
let longPress = null;
let suppressClickTarget = null;
let suppressClickUntil = 0;

const POINTER_TOOLTIP_DELAY_MS = 900;
const LONG_PRESS_DELAY_MS = 520;
const LONG_PRESS_MOVE_TOLERANCE_PX = 12;
const KEYBOARD_TOOLTIP_DELAY_MS = 450;

const tooltipElement = document.createElement('div');
tooltipElement.id = 'rustnote-shared-tooltip';
tooltipElement.className = 'shared-tooltip';
tooltipElement.setAttribute('role', 'tooltip');
tooltipElement.hidden = true;
document.body.appendChild(tooltipElement);

const isVisible = (element) => Boolean(
    element && element.isConnected &&
    (element.offsetWidth || element.offsetHeight || element.getClientRects().length)
);

const focusables = (surface) => Array.from(surface.querySelectorAll(focusableSelector))
    .filter(isVisible);

const tooltipCandidateSelector = [
    'button[title]',
    'a[href][title]',
    'input[title]',
    'select[title]',
    'textarea[title]',
    '[role="button"][title]',
    '[role="separator"][title]',
    '[tabindex][title]'
].join(',');

const enhanceTooltips = (root = document) => {
    const candidates = [];
    if (root instanceof Element && root.matches(tooltipCandidateSelector)) {
        candidates.push(root);
    }
    if (root.querySelectorAll) {
        candidates.push(...root.querySelectorAll(tooltipCandidateSelector));
    }
    for (const element of candidates) {
        const text = element.getAttribute('title');
        if (!text) continue;
        const visibleLabel = element.textContent.replace(/\s+/g, ' ').trim();
        const visiblyLabelled = /[A-Za-z]{3,}/.test(visibleLabel);
        const forceTooltip = element.getAttribute('data-tooltip-force') === 'true';
        if (visiblyLabelled && !forceTooltip) {
            element.removeAttribute('data-tooltip');
            element.removeAttribute('title');
            continue;
        }
        element.setAttribute('data-tooltip', text);
        if (!element.hasAttribute('aria-label') && !visibleLabel) {
            element.setAttribute('aria-label', text);
        }
        element.removeAttribute('title');
    }
};

const clearTooltipTimer = () => {
    if (tooltipTimer !== null) {
        window.clearTimeout(tooltipTimer);
        tooltipTimer = null;
    }
    pendingTooltipTarget = null;
};

const hideTooltip = (target = null) => {
    if (
        target &&
        target !== tooltipTarget &&
        target !== pendingTooltipTarget
    ) return;
    clearTooltipTimer();
    tooltipTarget = null;
    tooltipElement.hidden = true;
    tooltipElement.textContent = '';
};

const positionTooltip = (target) => {
    if (target !== tooltipTarget || !isVisible(target) || tooltipElement.hidden) return;
    const targetRect = target.getBoundingClientRect();
    const tooltipRect = tooltipElement.getBoundingClientRect();
    const margin = 8;
    const gap = 8;
    let left = targetRect.left + (targetRect.width - tooltipRect.width) / 2;
    left = Math.max(margin, Math.min(left, window.innerWidth - tooltipRect.width - margin));
    let top = targetRect.bottom + gap;
    if (top + tooltipRect.height > window.innerHeight - margin) {
        top = targetRect.top - tooltipRect.height - gap;
    }
    top = Math.max(margin, top);
    tooltipElement.style.left = `${Math.round(left)}px`;
    tooltipElement.style.top = `${Math.round(top)}px`;
};

const showTooltip = (target) => {
    const text = target?.getAttribute('data-tooltip');
    if (!text || !isVisible(target)) return;
    clearTooltipTimer();
    tooltipTarget = target;
    tooltipElement.textContent = text;
    tooltipElement.hidden = false;
    requestAnimationFrame(() => positionTooltip(target));
};

const scheduleTooltip = (target, delay) => {
    const text = target?.getAttribute('data-tooltip');
    if (!text || !isVisible(target)) return;
    hideTooltip();
    pendingTooltipTarget = target;
    tooltipTimer = window.setTimeout(() => {
        tooltipTimer = null;
        if (pendingTooltipTarget !== target) return;
        pendingTooltipTarget = null;
        showTooltip(target);
    }, delay);
};

const tooltipOwner = (node) => node?.closest?.('[data-tooltip]') || null;
const onPointerOverTooltip = (event) => {
    const owner = tooltipOwner(event.target);
    if (owner && !owner.contains(event.relatedTarget)) {
        scheduleTooltip(owner, POINTER_TOOLTIP_DELAY_MS);
    }
};
const onPointerOutTooltip = (event) => {
    const owner = tooltipOwner(event.target);
    if (owner && !owner.contains(event.relatedTarget)) hideTooltip(owner);
};
const onFocusInTooltip = (event) => {
    const owner = tooltipOwner(event.target);
    if (owner) scheduleTooltip(owner, KEYBOARD_TOOLTIP_DELAY_MS);
};
const onFocusOutTooltip = (event) => {
    const owner = tooltipOwner(event.target);
    if (owner && !owner.contains(event.relatedTarget)) hideTooltip(owner);
};
const onViewportChange = () => {
    hideTooltip();
    if (currentSurface?.getAttribute('data-context-menu') === 'true') {
        positionContextMenu(currentSurface);
    }
};

const longPressCandidate = (node) => node?.closest?.('[data-context-long-press="true"]') || null;

const clearLongPress = () => {
    if (longPress?.timer !== null && longPress?.timer !== undefined) {
        window.clearTimeout(longPress.timer);
    }
    longPress = null;
};

const longPressTargetAllowsContext = (candidate, eventTarget) => {
    if (!(candidate instanceof Element) || !isVisible(candidate)) return false;
    const interactive = eventTarget?.closest?.('button, a[href], input, textarea, select, option, [contenteditable="true"]');
    if (interactive && candidate.contains(interactive) && interactive !== candidate) return false;
    return true;
};

const onLongPressPointerDown = (event) => {
    if (!event.isPrimary || event.pointerType === 'mouse' || event.button !== 0) return;
    const candidate = longPressCandidate(event.target);
    if (!longPressTargetAllowsContext(candidate, event.target)) return;
    clearLongPress();
    const state = {
        pointerId: event.pointerId,
        target: candidate,
        startX: event.clientX,
        startY: event.clientY,
        clientX: event.clientX,
        clientY: event.clientY,
        timer: null,
    };
    state.timer = window.setTimeout(() => {
        if (longPress !== state || !isVisible(state.target)) return;
        longPress = null;
        suppressClickTarget = state.target;
        suppressClickUntil = performance.now() + 900;
        hideTooltip();
        state.target.dispatchEvent(new MouseEvent('contextmenu', {
            bubbles: true,
            cancelable: true,
            view: window,
            button: 2,
            buttons: 0,
            clientX: state.clientX,
            clientY: state.clientY,
        }));
    }, LONG_PRESS_DELAY_MS);
    longPress = state;
};

const onLongPressPointerMove = (event) => {
    if (!longPress || event.pointerId !== longPress.pointerId) return;
    longPress.clientX = event.clientX;
    longPress.clientY = event.clientY;
    const dx = event.clientX - longPress.startX;
    const dy = event.clientY - longPress.startY;
    if (Math.hypot(dx, dy) > LONG_PRESS_MOVE_TOLERANCE_PX) clearLongPress();
};

const onLongPressPointerEnd = (event) => {
    if (longPress && event.pointerId === longPress.pointerId) clearLongPress();
};

const onContextMenuCapture = (event) => {
    if (longPressCandidate(event.target)) clearLongPress();
};

const onSuppressedClickCapture = (event) => {
    if (!suppressClickTarget || performance.now() > suppressClickUntil) {
        suppressClickTarget = null;
        suppressClickUntil = 0;
        return;
    }
    const candidate = longPressCandidate(event.target);
    if (candidate !== suppressClickTarget) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    suppressClickTarget = null;
    suppressClickUntil = 0;
};

const restoreFocus = (surface, opener) => {
    const fallbackId = surface?.getAttribute('data-return-focus');
    const fallbackIds = (surface?.getAttribute('data-fallback-focus') || '').split(/\s+/).filter(Boolean);
    const returnTarget = fallbackId ? document.getElementById(fallbackId) : null;
    const fallbackTarget = fallbackIds
        .map((id) => document.getElementById(id))
        .find(isVisible);
    const target = isVisible(opener)
        ? opener
        : (isVisible(returnTarget) ? returnTarget : fallbackTarget);
    if (target && typeof target.focus === 'function') {
        requestAnimationFrame(() => target.focus({ preventScroll: true }));
    }
};

const closeSurface = (surface, restore = true) => {
    if (!surface || !surface.isConnected) return;
    if (surface.getAttribute('aria-busy') === 'true') return;
    const id = surface.id;
    const escaped = window.CSS?.escape ? CSS.escape(id) : id.replace(/[^a-zA-Z0-9_-]/g, '\\$&');
    const closeControl = id
        ? document.querySelector(`[data-transient-close="${escaped}"]`)
        : null;
    const opener = currentSurface === surface ? currentOpener : null;
    switchingSurface = !restore;
    if (closeControl) {
        closeControl.click();
    } else {
        const trigger = id
            ? document.querySelector(`[data-transient-trigger][aria-controls="${escaped}"]`)
            : null;
        trigger?.click();
    }
    if (restore) restoreFocus(surface, opener);
    if (currentSurface === surface) {
        currentSurface = null;
        currentOpener = null;
    }
    switchingSurface = false;
};

window.__rustnoteDismissTransientSurface = (surfaceId, restore = false) => {
    const surface = document.getElementById(String(surfaceId || ''));
    if (!surface || !surface.matches(surfaceSelector) || !isVisible(surface)) {
        return false;
    }
    closeSurface(surface, Boolean(restore));
    return true;
};

const positionContextMenu = (surface) => {
    if (!surface || surface.getAttribute('data-context-menu') !== 'true') return;
    requestAnimationFrame(() => {
        if (!isVisible(surface)) return;
        const menuRect = surface.getBoundingClientRect();
        const margin = 8;
        const gap = 5;
        const rawX = surface.getAttribute('data-origin-x') || '';
        const rawY = surface.getAttribute('data-origin-y') || '';
        const pointerX = rawX === '' ? Number.NaN : Number(rawX);
        const pointerY = rawY === '' ? Number.NaN : Number(rawY);
        let left;
        let top;

        if (Number.isFinite(pointerX) && Number.isFinite(pointerY)) {
            left = pointerX;
            top = pointerY;
            if (left + menuRect.width > window.innerWidth - margin) {
                left = pointerX - menuRect.width;
            }
            if (top + menuRect.height > window.innerHeight - margin) {
                top = pointerY - menuRect.height;
            }
        } else {
            const anchorId = surface.getAttribute('data-anchor-id') || '';
            const anchor = anchorId ? document.getElementById(anchorId) : null;
            const anchorRect = isVisible(anchor) ? anchor.getBoundingClientRect() : null;
            if (anchorRect) {
                left = anchorRect.right - menuRect.width;
                top = anchorRect.bottom + gap;
                if (top + menuRect.height > window.innerHeight - margin) {
                    top = anchorRect.top - menuRect.height - gap;
                }
            } else {
                left = margin;
                top = margin;
            }
        }

        left = Math.max(margin, Math.min(left, window.innerWidth - menuRect.width - margin));
        top = Math.max(margin, Math.min(top, window.innerHeight - menuRect.height - margin));
        surface.style.left = `${Math.round(left)}px`;
        surface.style.top = `${Math.round(top)}px`;
    });
};

const activateSurface = (surface) => {
    if (!surface || !surface.isConnected) return;
    const opener = surface.id && surface.id === pendingTargetId ? pendingOpener : null;
    for (const other of document.querySelectorAll(surfaceSelector)) {
        if (other !== surface && isVisible(other)) {
            closeSurface(other, false);
        }
    }
    currentSurface = surface;
    currentOpener = opener?.isConnected ? opener : null;
    pendingOpener = null;
    pendingTargetId = null;
    positionContextMenu(surface);
    const kind = surface.getAttribute('data-transient-kind');
    const isDialog = surface.getAttribute('role') === 'dialog';
    const preservedExternalId = surface.getAttribute('data-preserve-external-selection') || '';
    const preservedExternal = preservedExternalId ? document.getElementById(preservedExternalId) : null;
    const keepExternalSelection = Boolean(preservedExternal && document.activeElement === preservedExternal);
    if ((kind === 'menu' || isDialog) && !keepExternalSelection) {
        requestAnimationFrame(() => {
            const initial = surface.querySelector('[data-initial-focus="true"]')
                || focusables(surface)[0]
                || surface;
            if (!surface.hasAttribute('tabindex')) surface.setAttribute('tabindex', '-1');
            initial.focus({ preventScroll: true });
        });
    }
};

const findAddedSurface = (node) => {
    if (!(node instanceof Element)) return null;
    if (node.matches(surfaceSelector)) return node;
    return node.querySelector(surfaceSelector);
};

const onClickCapture = (event) => {
    hideTooltip();
    const trigger = event.target.closest?.('[data-transient-trigger][aria-controls]');
    if (!trigger) return;
    const targetId = trigger.getAttribute('aria-controls');
    pendingOpener = trigger;
    pendingTargetId = targetId;

    const nestedTransition = Boolean(
        currentSurface &&
        currentSurface.isConnected &&
        currentSurface.contains(trigger) &&
        currentSurface.id !== targetId
    );

    if (!nestedTransition) {
        for (const surface of document.querySelectorAll(surfaceSelector)) {
            if (surface.id !== targetId && isVisible(surface)) {
                closeSurface(surface, false);
            }
        }
    }

    requestAnimationFrame(() => {
        if (pendingOpener !== trigger || pendingTargetId !== targetId) return;
        const targetSurface = targetId ? document.getElementById(targetId) : null;
        if (!isVisible(targetSurface)) {
            pendingOpener = null;
            pendingTargetId = null;
        }
    });
};

const onPointerDownCapture = (event) => {
    hideTooltip();
    const target = event.target;
    const title = document.getElementById('note-title-input');
    if (
        title
        && document.activeElement === title
        && target?.closest?.('#editor-typography-trigger')
    ) {
        event.preventDefault();
        return;
    }
    if (!currentSurface || !currentSurface.isConnected) return;
    const preservedExternalId = currentSurface.getAttribute('data-preserve-external-selection') || '';
    const preservedExternal = preservedExternalId ? document.getElementById(preservedExternalId) : null;
    if (
        preservedExternal
        && document.activeElement === preservedExternal
        && currentSurface.contains(target)
        && target?.closest?.('button')
    ) {
        event.preventDefault();
        return;
    }
    if (currentSurface.getAttribute('data-dismiss-outside') !== 'true') return;
    if (currentSurface.contains(target) || currentOpener?.contains(target)) return;
    closeSurface(currentSurface, true);
};

const onBeforeInputCapture = (event) => {
    hideTooltip();
    if (
        currentSurface?.id === 'editor-typography-menu' &&
        event.target?.closest?.('#note-prosemirror-editor .ProseMirror')
    ) {
        closeSurface(currentSurface, false);
    }
};

const onKeyDownCapture = (event) => {
    if (event.key === 'Escape') hideTooltip();
    const keyboardContextRequest = !event.isComposing && (
        event.key === 'ContextMenu'
        || (event.key === 'F10' && event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey)
    );
    if (keyboardContextRequest) {
        const candidate = longPressCandidate(event.target);
        if (longPressTargetAllowsContext(candidate, event.target)) {
            event.preventDefault();
            event.stopImmediatePropagation();
            const rect = candidate.getBoundingClientRect();
            candidate.dispatchEvent(new MouseEvent('contextmenu', {
                bubbles: true,
                cancelable: true,
                view: window,
                button: 2,
                buttons: 0,
                clientX: Math.round(rect.left + Math.min(18, rect.width / 2)),
                clientY: Math.round(rect.bottom),
            }));
            return;
        }
    }
    if (!currentSurface || !currentSurface.isConnected) return;
    if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        closeSurface(currentSurface, true);
        return;
    }
    const kind = currentSurface.getAttribute('data-transient-kind');
    if (kind === 'menu' && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
        const menuItems = focusables(currentSurface).filter((item) =>
            item.matches('[role="menuitem"], [role="menuitemradio"], [role="menuitemcheckbox"]')
        );
        if (menuItems.length > 0) {
            event.preventDefault();
            const currentIndex = Math.max(0, menuItems.indexOf(document.activeElement));
            let nextIndex = currentIndex;
            if (event.key === 'ArrowDown') nextIndex = (currentIndex + 1) % menuItems.length;
            if (event.key === 'ArrowUp') nextIndex = (currentIndex - 1 + menuItems.length) % menuItems.length;
            if (event.key === 'Home') nextIndex = 0;
            if (event.key === 'End') nextIndex = menuItems.length - 1;
            menuItems[nextIndex].focus();
        }
        return;
    }
    if (event.key !== 'Tab' || currentSurface.getAttribute('aria-modal') !== 'true') {
        return;
    }
    const items = focusables(currentSurface);
    if (items.length === 0) {
        event.preventDefault();
        currentSurface.focus();
        return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    if (!currentSurface.contains(document.activeElement)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
        return;
    }
    if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
    }
};

const observer = new MutationObserver((records) => {
    let addedSurface = null;
    for (const record of records) {
        if (record.type === 'attributes') {
            enhanceTooltips(record.target);
            continue;
        }
        for (const node of record.addedNodes) {
            enhanceTooltips(node);
            addedSurface = findAddedSurface(node) || addedSurface;
        }
    }
    if (addedSurface) {
        activateSurface(addedSurface);
    } else if (currentSurface && !currentSurface.isConnected && !switchingSurface) {
        restoreFocus(currentSurface, currentOpener);
        currentSurface = null;
        currentOpener = null;
    } else if (currentSurface?.getAttribute('data-context-menu') === 'true') {
        positionContextMenu(currentSurface);
    }
});

const cleanup = () => {
    clearTooltipTimer();
    clearLongPress();
    suppressClickTarget = null;
    suppressClickUntil = 0;
    observer.disconnect();
    document.removeEventListener('click', onClickCapture, true);
    document.removeEventListener('click', onSuppressedClickCapture, true);
    document.removeEventListener('pointerdown', onPointerDownCapture, true);
    document.removeEventListener('pointerdown', onLongPressPointerDown, true);
    document.removeEventListener('pointermove', onLongPressPointerMove, true);
    document.removeEventListener('pointerup', onLongPressPointerEnd, true);
    document.removeEventListener('pointercancel', onLongPressPointerEnd, true);
    document.removeEventListener('contextmenu', onContextMenuCapture, true);
    document.removeEventListener('keydown', onKeyDownCapture, true);
    document.removeEventListener('beforeinput', onBeforeInputCapture, true);
    document.removeEventListener('pointerover', onPointerOverTooltip, true);
    document.removeEventListener('pointerout', onPointerOutTooltip, true);
    document.removeEventListener('focusin', onFocusInTooltip, true);
    document.removeEventListener('focusout', onFocusOutTooltip, true);
    window.removeEventListener('resize', onViewportChange);
    window.removeEventListener('scroll', onViewportChange, true);
    window.removeEventListener('beforeunload', cleanup);
    tooltipElement.remove();
    delete window.__rustnoteDismissTransientSurface;
    delete window.__rustnoteTransientCleanup;
};

window.__rustnoteTransientCleanup = cleanup;
enhanceTooltips();
observer.observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['title']
});
document.addEventListener('click', onSuppressedClickCapture, true);
document.addEventListener('click', onClickCapture, true);
document.addEventListener('pointerdown', onPointerDownCapture, true);
document.addEventListener('pointerdown', onLongPressPointerDown, true);
document.addEventListener('pointermove', onLongPressPointerMove, true);
document.addEventListener('pointerup', onLongPressPointerEnd, true);
document.addEventListener('pointercancel', onLongPressPointerEnd, true);
document.addEventListener('contextmenu', onContextMenuCapture, true);
document.addEventListener('keydown', onKeyDownCapture, true);
document.addEventListener('beforeinput', onBeforeInputCapture, true);
document.addEventListener('pointerover', onPointerOverTooltip, true);
document.addEventListener('pointerout', onPointerOutTooltip, true);
document.addEventListener('focusin', onFocusInTooltip, true);
document.addEventListener('focusout', onFocusOutTooltip, true);
window.addEventListener('resize', onViewportChange, { passive: true });
window.addEventListener('scroll', onViewportChange, { passive: true, capture: true });
window.addEventListener('beforeunload', cleanup, { once: true });

await new Promise((resolve) => window.addEventListener('beforeunload', resolve, { once: true }));
return true;
"#;
