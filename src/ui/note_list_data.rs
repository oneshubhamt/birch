use std::cmp::Ordering;

use crate::application::errors::DataAccessError;
use crate::application::executor::{
    ApplicationExecutionError, ApplicationExecutor, ExecutionFailure,
};
use dioxus::prelude::*;

use crate::application::{editor::EditorService, library::LibraryService};
use crate::domain::{
    NOTE_LIST_PAGE_SIZE, Note, NoteId, NoteListCursor, NoteListPage, NoteListQuery, NoteSort,
    NoteSummary, Section,
};

use super::NotesListState;
use super::synchronization::SynchronizationMode;

pub(super) const NOTE_CARD_HEIGHT_PX: f64 = 104.0;
pub(super) const NOTE_CARD_GAP_PX: f64 = 6.0;
pub(super) const NOTE_CARD_ROW_PX: f64 = NOTE_CARD_HEIGHT_PX + NOTE_CARD_GAP_PX;
const NOTE_LIST_MIN_RENDERED: usize = 40;
const NOTE_LIST_MAX_RENDERED: usize = 80;
const NOTE_LIST_OVERSCAN: usize = 12;
const NOTE_LIST_LOAD_AHEAD_ROWS: usize = 12;
const NOTE_LIST_CACHE_PAGES: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct NoteListCache {
    start_page: usize,
    pages: Vec<Vec<NoteSummary>>,
}

impl NoteListCache {
    pub(super) fn from_first_page(notes: Vec<NoteSummary>) -> Self {
        if notes.is_empty() {
            Self::default()
        } else {
            Self {
                start_page: 0,
                pages: vec![notes],
            }
        }
    }

    pub(super) fn start_page(&self) -> usize {
        self.start_page
    }

    pub(super) fn end_page_exclusive(&self) -> usize {
        self.start_page.saturating_add(self.pages.len())
    }

    pub(super) fn start_index(&self) -> usize {
        self.start_page.saturating_mul(NOTE_LIST_PAGE_SIZE)
    }

    pub(super) fn len(&self) -> usize {
        self.pages.iter().map(Vec::len).sum()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pages.iter().all(Vec::is_empty)
    }

    pub(super) fn end_index(&self) -> usize {
        self.start_index().saturating_add(self.len())
    }

    pub(super) fn contains_page(&self, page_index: usize) -> bool {
        page_index >= self.start_page && page_index < self.end_page_exclusive()
    }

    pub(super) fn contains_index(&self, absolute_index: usize) -> bool {
        absolute_index >= self.start_index() && absolute_index < self.end_index()
    }

    pub(super) fn cloned_local_range(&self, start: usize, end: usize) -> Vec<NoteSummary> {
        self.pages
            .iter()
            .flat_map(|page| page.iter())
            .skip(start)
            .take(end.saturating_sub(start))
            .cloned()
            .collect()
    }

    pub(super) fn note_by_id(&self, note_id: &NoteId) -> Option<&NoteSummary> {
        self.pages
            .iter()
            .flat_map(|page| page.iter())
            .find(|note| &note.id == note_id)
    }

    pub(super) fn note_at_absolute(&self, absolute_index: usize) -> Option<&NoteSummary> {
        if !self.contains_index(absolute_index) {
            return None;
        }
        let local_index = absolute_index.saturating_sub(self.start_index());
        self.pages
            .iter()
            .flat_map(|page| page.iter())
            .nth(local_index)
    }

    pub(super) fn absolute_index_of(&self, note_id: &NoteId) -> Option<usize> {
        self.pages
            .iter()
            .flat_map(|page| page.iter())
            .position(|note| &note.id == note_id)
            .map(|local_index| self.start_index().saturating_add(local_index))
    }

    fn insert_page(&mut self, page_index: usize, notes: Vec<NoteSummary>, replace_cache: bool) {
        if notes.is_empty() {
            if replace_cache {
                self.start_page = page_index;
                self.pages.clear();
            }
            return;
        }
        if replace_cache || self.pages.is_empty() {
            self.start_page = page_index;
            self.pages = vec![notes];
            return;
        }

        let end_page = self.end_page_exclusive();
        if self.contains_page(page_index) {
            self.pages[page_index - self.start_page] = notes;
        } else if page_index == end_page {
            self.pages.push(notes);
        } else if page_index.saturating_add(1) == self.start_page {
            self.start_page = page_index;
            self.pages.insert(0, notes);
        } else {
            self.start_page = page_index;
            self.pages = vec![notes];
        }

        while self.pages.len() > NOTE_LIST_CACHE_PAGES {
            let midpoint = self.start_page.saturating_add(self.pages.len() / 2);
            if page_index >= midpoint {
                self.pages.remove(0);
                self.start_page = self.start_page.saturating_add(1);
            } else {
                self.pages.pop();
            }
        }
    }

    fn update_note(&mut self, saved_note: &Note, sort: NoteSort) -> CachedNoteUpdate {
        let summary = NoteSummary::from(saved_note);
        let Some(previous) = self.note_by_id(&summary.id).cloned() else {
            return CachedNoteUpdate::Missing;
        };
        let ordering_changed = note_ordering_changed(&previous, &summary, sort);

        for page in &mut self.pages {
            if let Some(index) = page.iter().position(|note| note.id == summary.id) {
                page[index] = summary.clone();
                break;
            }
        }
        if !ordering_changed {
            return CachedNoteUpdate::Updated;
        }

        let page_lengths = self.pages.iter().map(Vec::len).collect::<Vec<_>>();
        let mut notes = std::mem::take(&mut self.pages)
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        notes.sort_by(|left, right| compare_note_summaries(left, right, sort));

        let mut notes = notes.into_iter();
        self.pages = page_lengths
            .into_iter()
            .map(|length| notes.by_ref().take(length).collect())
            .collect();
        CachedNoteUpdate::OrderingChanged
    }

    fn remove_note(&mut self, note_id: &NoteId) -> bool {
        for page in &mut self.pages {
            let previous_len = page.len();
            page.retain(|note| &note.id != note_id);
            if page.len() != previous_len {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CachedNoteUpdate {
    Missing,
    Updated,
    OrderingChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoteNavigation {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
}

pub(super) fn navigation_target(
    current_index: usize,
    total_note_count: usize,
    page_rows: usize,
    navigation: NoteNavigation,
) -> Option<usize> {
    if total_note_count == 0 {
        return None;
    }
    let last_index = total_note_count - 1;
    let target = match navigation {
        NoteNavigation::Previous => current_index.saturating_sub(1),
        NoteNavigation::Next => current_index.saturating_add(1).min(last_index),
        NoteNavigation::PagePrevious => current_index.saturating_sub(page_rows.max(1)),
        NoteNavigation::PageNext => current_index
            .saturating_add(page_rows.max(1))
            .min(last_index),
        NoteNavigation::First => 0,
        NoteNavigation::Last => last_index,
    };
    (target != current_index).then_some(target)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NotePageLoadMode {
    Extend,
    Replace,
    Reanchor,
}

impl NotePageLoadMode {
    fn replaces_cache(self) -> bool {
        matches!(self, Self::Replace | Self::Reanchor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct NotePageRequest {
    pub page_index: usize,
    pub mode: NotePageLoadMode,
    pub restore_scroll_top: f64,
    pub reconcile_selection: bool,
    pub select_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct VirtualNoteWindow {
    pub start: usize,
    pub end: usize,
    pub top_spacer_px: f64,
    pub bottom_spacer_px: f64,
}

pub(super) fn virtual_note_window(
    total_note_count: usize,
    cache_start_index: usize,
    cached_count: usize,
    scroll_top: f64,
    viewport_height: f64,
) -> VirtualNoteWindow {
    if total_note_count == 0 || cached_count == 0 {
        return VirtualNoteWindow {
            start: 0,
            end: 0,
            top_spacer_px: 0.0,
            bottom_spacer_px: total_note_count as f64 * NOTE_CARD_ROW_PX,
        };
    }

    let visible_rows = (viewport_height.max(NOTE_CARD_ROW_PX) / NOTE_CARD_ROW_PX).ceil() as usize;
    let window_size = (visible_rows + NOTE_LIST_OVERSCAN * 2)
        .clamp(NOTE_LIST_MIN_RENDERED, NOTE_LIST_MAX_RENDERED)
        .min(total_note_count);
    let first_visible = ((virtual_scroll_anchor(scroll_top) / NOTE_CARD_ROW_PX) as usize)
        .min(total_note_count.saturating_sub(1));
    let mut desired_start = first_visible.saturating_sub(NOTE_LIST_OVERSCAN);
    let mut desired_end = desired_start
        .saturating_add(window_size)
        .min(total_note_count);
    if desired_end.saturating_sub(desired_start) < window_size {
        desired_start = desired_end.saturating_sub(window_size);
        desired_end = total_note_count;
    }

    let cache_end_index = cache_start_index.saturating_add(cached_count);
    let rendered_start = desired_start.max(cache_start_index).min(cache_end_index);
    let rendered_end = desired_end.min(cache_end_index).max(rendered_start);

    VirtualNoteWindow {
        start: rendered_start.saturating_sub(cache_start_index),
        end: rendered_end.saturating_sub(cache_start_index),
        top_spacer_px: rendered_start as f64 * NOTE_CARD_ROW_PX,
        bottom_spacer_px: total_note_count.saturating_sub(rendered_end) as f64 * NOTE_CARD_ROW_PX,
    }
}

pub(super) fn virtual_scroll_anchor(scroll_top: f64) -> f64 {
    (scroll_top.max(0.0) / NOTE_CARD_ROW_PX).floor() * NOTE_CARD_ROW_PX
}

pub(super) fn page_for_viewport(
    cache: &NoteListCache,
    total_note_count: usize,
    loading: bool,
    scroll_top: f64,
    viewport_height: f64,
) -> Option<usize> {
    if loading || total_note_count == 0 {
        return None;
    }
    let first_visible = ((scroll_top.max(0.0) / NOTE_CARD_ROW_PX).floor() as usize)
        .min(total_note_count.saturating_sub(1));
    let visible_rows = (viewport_height.max(NOTE_CARD_ROW_PX) / NOTE_CARD_ROW_PX).ceil() as usize;
    let last_visible = first_visible
        .saturating_add(visible_rows)
        .min(total_note_count.saturating_sub(1));

    if cache.is_empty() || first_visible < cache.start_index() || first_visible >= cache.end_index()
    {
        return Some(first_visible / NOTE_LIST_PAGE_SIZE);
    }

    if first_visible
        < cache
            .start_index()
            .saturating_add(NOTE_LIST_LOAD_AHEAD_ROWS)
        && cache.start_page() > 0
    {
        return Some(cache.start_page() - 1);
    }

    if last_visible.saturating_add(NOTE_LIST_LOAD_AHEAD_ROWS) >= cache.end_index()
        && cache.end_index() < total_note_count
    {
        return Some(cache.end_page_exclusive());
    }
    None
}

pub(super) fn current_note_list_query(state: NotesListState) -> NoteListQuery {
    NoteListQuery {
        section: *state.section.peek(),
        notebook_id: state.selected_notebook.peek().clone(),
        search: state.search.peek().clone(),
        sort: *state.sort_order.peek(),
        filters: state.filters.peek().clone(),
    }
}

fn known_page_cursor(state: NotesListState, page_index: usize) -> Option<Option<NoteListCursor>> {
    if page_index == 0 {
        return Some(None);
    }
    state
        .page_start_cursors
        .peek()
        .get(page_index)
        .cloned()
        .flatten()
        .map(Some)
}

fn nearest_loadable_page(
    cache: &NoteListCache,
    cursors: &[Option<NoteListCursor>],
    requested_page: usize,
) -> Option<usize> {
    if requested_page == 0 || cursors.get(requested_page).is_some_and(Option::is_some) {
        return Some(requested_page);
    }
    if cache.is_empty() {
        return Some(0);
    }
    let frontier = if requested_page < cache.start_page() {
        cache.start_page().checked_sub(1)?
    } else {
        cache.end_page_exclusive()
    };
    if frontier == 0 || cursors.get(frontier).is_some_and(Option::is_some) {
        Some(frontier)
    } else {
        None
    }
}

fn queue_page_request(mut state: NotesListState, request: NotePageRequest) {
    if *state.list_loading.peek() {
        return;
    }
    if request.mode != NotePageLoadMode::Reanchor
        && known_page_cursor(state, request.page_index).is_none()
    {
        return;
    }
    state.list_loading.set(true);
    state.list_page_request.set(Some(request));
    let next_request = (*state.list_request_sequence.peek()).wrapping_add(1);
    state.list_request_sequence.set(next_request);
}

pub(super) fn reload_note_list(mut state: NotesListState) {
    let next_generation = (*state.list_generation.peek()).wrapping_add(1);
    state.list_generation.set(next_generation);

    // Keep the last known-good cache visible until the replacement query succeeds.
    // Clearing first turns any transient query failure into an apparently empty
    // library even though the database is intact.
    state.list_loading.set(false);
    eprintln!(
        "[rustnote][note-list] reload generation={} cached={} total={}",
        next_generation,
        state.note_cache.peek().len(),
        *state.note_count.peek(),
    );
    queue_page_request(
        state,
        NotePageRequest {
            page_index: 0,
            mode: NotePageLoadMode::Replace,
            restore_scroll_top: 0.0,
            reconcile_selection: true,
            select_index: None,
        },
    );
}

pub(super) fn refresh_note_list(mut state: NotesListState) {
    let next_generation = (*state.list_generation.peek()).wrapping_add(1);
    state.list_generation.set(next_generation);
    state.list_loading.set(false);

    // A mutation can move a row across any existing keyset boundary. Cursors
    // therefore belong to the list generation that produced them and must not
    // survive a mutation refresh. Re-anchor only the visible logical page;
    // ordinary forward scrolling remains keyset-paginated from that point.
    state.page_start_cursors.set(vec![None]);
    let scroll_top = *state.list_scroll_top.peek();
    let page_index = ((scroll_top / NOTE_CARD_ROW_PX).floor() as usize) / NOTE_LIST_PAGE_SIZE;
    queue_page_request(
        state,
        NotePageRequest {
            page_index,
            mode: NotePageLoadMode::Reanchor,
            restore_scroll_top: scroll_top,
            reconcile_selection: true,
            select_index: None,
        },
    );
}

pub(super) fn load_note_page_for_viewport(
    state: NotesListState,
    scroll_top: f64,
    viewport_height: f64,
) {
    let page_index = {
        let cache = state.note_cache.peek();
        page_for_viewport(
            &cache,
            *state.note_count.peek(),
            *state.list_loading.peek(),
            scroll_top,
            viewport_height,
        )
    };
    let Some(requested_page) = page_index else {
        return;
    };
    let (page_index, mode) = {
        let cache = state.note_cache.peek();
        if cache.contains_page(requested_page) {
            return;
        }
        let cursors = state.page_start_cursors.peek();
        match nearest_loadable_page(&cache, &cursors, requested_page) {
            Some(page_index) => (page_index, NotePageLoadMode::Extend),
            None if requested_page < cache.start_page() => {
                (requested_page, NotePageLoadMode::Reanchor)
            }
            None => return,
        }
    };
    queue_page_request(
        state,
        NotePageRequest {
            page_index,
            mode,
            restore_scroll_top: scroll_top,
            reconcile_selection: false,
            select_index: None,
        },
    );
}

pub(super) fn navigate_to_note_index(
    editor: EditorService,
    target_index: usize,
    state: NotesListState,
) {
    if target_index >= *state.note_count.peek() {
        return;
    }
    if let Some(summary) = state
        .note_cache
        .peek()
        .note_at_absolute(target_index)
        .cloned()
    {
        select_note(editor, summary.id, state, move |note_id| {
            focus_and_reveal_note_card(note_id, target_index)
        });
        return;
    }
    let requested_page = target_index / NOTE_LIST_PAGE_SIZE;
    let (page_index, mode) = {
        let cache = state.note_cache.peek();
        let cursors = state.page_start_cursors.peek();
        match nearest_loadable_page(&cache, &cursors, requested_page) {
            Some(page_index) => (page_index, NotePageLoadMode::Extend),
            None if requested_page < cache.start_page() => {
                (requested_page, NotePageLoadMode::Reanchor)
            }
            None => return,
        }
    };
    queue_page_request(
        state,
        NotePageRequest {
            page_index,
            mode,
            restore_scroll_top: *state.list_scroll_top.peek(),
            reconcile_selection: false,
            select_index: Some(target_index),
        },
    );
}

fn fallback_after_empty_replacement(
    request: NotePageRequest,
    current_total_count: usize,
) -> Option<(usize, NotePageRequest)> {
    if !request.mode.replaces_cache() || request.page_index == 0 {
        return None;
    }
    let corrected_count =
        current_total_count.min(request.page_index.saturating_mul(NOTE_LIST_PAGE_SIZE));
    let last_row = corrected_count.saturating_sub(1);
    let last_page = last_row / NOTE_LIST_PAGE_SIZE;
    Some((
        corrected_count,
        NotePageRequest {
            page_index: (request.page_index - 1).min(last_page),
            mode: if request.mode == NotePageLoadMode::Reanchor {
                NotePageLoadMode::Reanchor
            } else {
                NotePageLoadMode::Replace
            },
            restore_scroll_top: request
                .restore_scroll_top
                .min(last_row as f64 * NOTE_CARD_ROW_PX),
            reconcile_selection: request.reconcile_selection,
            select_index: request
                .select_index
                .filter(|target| *target < corrected_count),
        },
    ))
}

fn generations_match(current: u64, expected: u64) -> bool {
    current == expected
}

fn generation_is_current(state: NotesListState, expected: u64) -> bool {
    generations_match(*state.list_generation.peek(), expected)
}

fn inconsistent_empty_replacement(request: NotePageRequest, page: &NoteListPage) -> bool {
    request.mode.replaces_cache()
        && request.page_index == 0
        && page.notes.is_empty()
        && page.total_count.is_some_and(|count| count > 0)
}

#[derive(Debug, PartialEq)]
enum SelectionReconciliation {
    Keep,
    Replace(Option<NoteId>),
}

fn selection_reconciliation(
    keep_selected: bool,
    fallback_note_id: Option<NoteId>,
) -> SelectionReconciliation {
    if keep_selected {
        SelectionReconciliation::Keep
    } else {
        SelectionReconciliation::Replace(fallback_note_id)
    }
}

#[derive(Debug)]
enum NotePageLoadError {
    Data(DataAccessError),
    Execution(ApplicationExecutionError),
}

impl std::fmt::Display for NotePageLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Data(error) => write!(formatter, "note-list worker failed: {error}"),
            Self::Execution(error) => write!(formatter, "note-list worker failed: {error}"),
        }
    }
}

impl std::error::Error for NotePageLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Data(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<DataAccessError> for NotePageLoadError {
    fn from(error: DataAccessError) -> Self {
        Self::Data(error)
    }
}

impl ExecutionFailure for NotePageLoadError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

pub(super) fn execute_note_page_request(
    library: LibraryService,
    editor: EditorService,
    mut state: NotesListState,
) {
    let Some(request) = *state.list_page_request.peek() else {
        state.list_loading.set(false);
        return;
    };
    let cursor = if request.mode == NotePageLoadMode::Reanchor {
        None
    } else {
        let Some(cursor) = known_page_cursor(state, request.page_index) else {
            state.list_loading.set(false);
            return;
        };
        cursor
    };
    let generation = *state.list_generation.peek();
    let Some(publication_token) = state
        .privacy_epoch
        .capture_multi_note_publication(&state.services.peek().privacy(), state.list_generation)
    else {
        state.list_loading.set(false);
        return;
    };
    let query = current_note_list_query(state);
    let selected_id = state
        .selected_note
        .peek()
        .as_ref()
        .map(|note| note.id.clone());

    spawn(async move {
        let worker_library = library.clone();
        let worker_query = query.clone();
        let worker_selected_id = selected_id.clone();
        let result = ApplicationExecutor::read("note page load", move || {
            let (page_start_cursor, page) = match request.mode {
                NotePageLoadMode::Reanchor => worker_library
                    .reanchor_notes_page_with_total(&worker_query, request.page_index)
                    .map_err(NotePageLoadError::from)?,
                NotePageLoadMode::Replace => (
                    cursor.clone(),
                    worker_library
                        .list_notes_page_with_total(&worker_query, cursor.as_ref())
                        .map_err(NotePageLoadError::from)?,
                ),
                NotePageLoadMode::Extend => (
                    cursor.clone(),
                    worker_library
                        .list_notes_page(&worker_query, cursor.as_ref())
                        .map_err(NotePageLoadError::from)?,
                ),
            };
            let selection = if request.reconcile_selection {
                let keep_selected = match worker_selected_id.as_ref() {
                    Some(note_id) => worker_library
                        .note_matches_list_query(note_id, &worker_query)
                        .map_err(NotePageLoadError::from)?,
                    None => false,
                };
                let fallback_note_id = if keep_selected {
                    None
                } else {
                    page.notes.first().map(|first| first.id.clone())
                };
                Some((keep_selected, fallback_note_id))
            } else {
                None
            };
            Ok::<_, NotePageLoadError>((page_start_cursor, page, selection))
        })
        .await;

        if !generation_is_current(state, generation) {
            return;
        }
        let Some(_publication_lease) = publication_token.begin_publication() else {
            return;
        };
        match result {
            Ok((page_start_cursor, page, selection)) => {
                if inconsistent_empty_replacement(request, &page) {
                    state.list_loading.set(false);
                    eprintln!(
                        "[rustnote][note-list] rejected inconsistent replacement generation={} page={} mode={:?} total={:?} cached={}",
                        generation,
                        request.page_index,
                        request.mode,
                        page.total_count,
                        state.note_cache.peek().len(),
                    );
                    state.status.set(
                        "Could not refresh the note list safely; keeping the previous list"
                            .to_string(),
                    );
                    return;
                }
                if page.notes.is_empty() {
                    let current_total_count = page.total_count.unwrap_or_else(|| {
                        let note_count = state.note_count.peek();
                        *note_count
                    });
                    if let Some((corrected_count, fallback)) =
                        fallback_after_empty_replacement(request, current_total_count)
                    {
                        state.note_count.set(corrected_count);
                        state.list_loading.set(false);
                        queue_page_request(state, fallback);
                        return;
                    }
                }
                apply_page(request, page_start_cursor, page, state);
                if let Some((keep_selected, fallback_note_id)) = selection
                    && let SelectionReconciliation::Replace(target_note_id) =
                        selection_reconciliation(keep_selected, fallback_note_id)
                {
                    match target_note_id {
                        Some(target_note_id) => {
                            // Reconciliation chooses only an identity under the
                            // aggregate list lease. The target note itself is loaded
                            // and published through the ordinary source/target
                            // navigation path, which acquires fresh note-local
                            // authority for that target.
                            select_note(editor.clone(), target_note_id, state, |_| {});
                        }
                        None => {
                            state.advance_note_selection_generation();
                            state.selected_note.set(None);
                        }
                    }
                }
                restore_note_list_scroll(request.restore_scroll_top);
                if let Some(target_index) = request.select_index {
                    if let Some(summary) = state
                        .note_cache
                        .peek()
                        .note_at_absolute(target_index)
                        .cloned()
                    {
                        select_note(editor.clone(), summary.id, state, move |note_id| {
                            focus_and_reveal_note_card(note_id, target_index)
                        });
                    } else {
                        navigate_to_note_index(editor.clone(), target_index, state);
                    }
                } else {
                    // A scrollbar can move beyond the currently known keyset
                    // frontier. Continue advancing one bounded page at a time
                    // until the viewport is represented by the cache.
                    load_note_page_for_viewport(
                        state,
                        request.restore_scroll_top,
                        *state.list_viewport_height.peek(),
                    );
                }
            }
            Err(error) => {
                state.list_loading.set(false);
                eprintln!(
                    "[rustnote][note-list] load failed generation={} page={} mode={:?} cached={} total={}: {}",
                    generation,
                    request.page_index,
                    request.mode,
                    state.note_cache.peek().len(),
                    *state.note_count.peek(),
                    error,
                );
                state.status.set(format!("Could not load notes: {error}"));
            }
        }
    });
}

fn apply_page(
    request: NotePageRequest,
    page_start_cursor: Option<NoteListCursor>,
    page: NoteListPage,
    mut state: NotesListState,
) {
    let NoteListPage {
        notes,
        next_cursor,
        total_count,
    } = page;
    if request.mode.replaces_cache() {
        state.list_scroll_top.set(request.restore_scroll_top);
    }
    if let Some(total_count) = total_count {
        state.note_count.set(total_count);
    } else if notes.is_empty() && request.mode == NotePageLoadMode::Extend {
        let upper_bound = request.page_index.saturating_mul(NOTE_LIST_PAGE_SIZE);
        if *state.note_count.peek() > upper_bound {
            state.note_count.set(upper_bound);
        }
    }

    {
        let mut cursors = state.page_start_cursors.write();
        if request.mode == NotePageLoadMode::Reanchor {
            cursors.clear();
            cursors.resize(request.page_index + 1, None);
            cursors[request.page_index] = page_start_cursor;
        } else if request.mode == NotePageLoadMode::Replace && request.page_index == 0 {
            cursors.clear();
            cursors.push(None);
        }
        if cursors.len() <= request.page_index {
            cursors.resize(request.page_index + 1, None);
        }
        match next_cursor {
            Some(next_cursor) => {
                if cursors.len() <= request.page_index + 1 {
                    cursors.resize(request.page_index + 2, None);
                }
                cursors[request.page_index + 1] = Some(next_cursor);
                if request.mode.replaces_cache() {
                    cursors.truncate(request.page_index + 2);
                }
            }
            None => cursors.truncate(request.page_index + 1),
        }
    }

    state
        .note_cache
        .write()
        .insert_page(request.page_index, notes, request.mode.replaces_cache());
    state.list_loading.set(false);
}

pub(super) fn select_note<F>(
    editor: EditorService,
    note_id: NoteId,
    mut state: NotesListState,
    on_selected: F,
) where
    F: FnOnce(&NoteId) + 'static,
{
    // Source and target are deliberately authorized independently. A Vault
    // transition on unrelated B may invalidate aggregate surfaces without
    // invalidating the checkpoint of source A or publication of target C.
    let selection_generation = state.advance_note_selection_generation();
    let mut on_selected = Some(on_selected);
    state.spawn(async move {
        if !state.selected_source_checkpointed_by_active_vault_transition() {
            let source = state
                .synchronize_current_editor_when_privacy_ready(
                    editor.clone(),
                    SynchronizationMode::Checkpoint,
                    selection_generation,
                )
                .await;
            match source {
                Ok(Some(_)) => {}
                Ok(None) => return,
                Err(error) => {
                    state
                        .status
                        .set(format!("Could not finish saving: {error}"));
                    return;
                }
            }
        }

        loop {
            if !state.note_selection_is_current(selection_generation) {
                return;
            }
            let Some(target_token) =
                state.capture_note_publication_token(&note_id, state.note_selection_generation)
            else {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                continue;
            };

            let worker = editor.clone();
            let worker_note_id = note_id.clone();
            let result =
                ApplicationExecutor::read("note load", move || worker.load_note(&worker_note_id))
                    .await;

            if !state.note_selection_is_current(selection_generation) {
                return;
            }
            if !target_token.is_publishable() {
                continue;
            }
            let Some(_publication_lease) = target_token.begin_publication() else {
                continue;
            };
            match result {
                Ok(Some(note)) => {
                    state.selected_note.set(Some(note));
                    if let Some(on_selected) = on_selected.take() {
                        on_selected(&note_id);
                    }
                }
                Ok(None) => state
                    .status
                    .set("The selected note no longer exists".to_string()),
                Err(error) => state.status.set(format!("Could not open note: {error}")),
            }
            return;
        }
    });
}

fn restore_note_list_scroll(scroll_top: f64) {
    let script = format!(
        "const top = {scroll_top}; requestAnimationFrame(() => {{ const list = document.querySelector('.note-list'); if (list) list.scrollTop = top; }}); return true;"
    );
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

fn focus_and_reveal_note_card(note_id: &NoteId, target_index: usize) {
    let encoded_id = serde_json::to_string(note_id.as_str()).unwrap_or_else(|_| "\"\"".to_string());
    let row_top = target_index as f64 * NOTE_CARD_ROW_PX;
    let row_bottom = row_top + NOTE_CARD_ROW_PX;
    let script = format!(
        "const id = {encoded_id}; const list = document.querySelector('.note-list'); if (!list) return false; const top = {row_top}; const bottom = {row_bottom}; if (top < list.scrollTop) list.scrollTop = top; else if (bottom > list.scrollTop + list.clientHeight) list.scrollTop = bottom - list.clientHeight; requestAnimationFrame(() => {{ for (const card of document.querySelectorAll('.note-card[data-note-id]')) {{ if (card.dataset.noteId === id) {{ card.focus(); break; }} }} }}); return true;"
    );
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

pub(super) fn update_loaded_note(saved_note: &Note, mut state: NotesListState) -> CachedNoteUpdate {
    let sort = *state.sort_order.read();
    state.note_cache.write().update_note(saved_note, sort)
}

pub(super) fn note_list_query_is_scoped(state: NotesListState) -> bool {
    let query = current_note_list_query(state);
    query.section != Section::Notes
        || query.notebook_id.is_some()
        || !query.search.trim().is_empty()
        || !query.filters.is_empty()
}

fn note_ordering_changed(previous: &NoteSummary, updated: &NoteSummary, sort: NoteSort) -> bool {
    if previous.pinned != updated.pinned {
        return true;
    }
    match sort {
        NoteSort::UpdatedNewest | NoteSort::UpdatedOldest => {
            previous.updated_at != updated.updated_at
        }
        NoteSort::CreatedNewest => previous.created_at != updated.created_at,
        NoteSort::TitleAscending | NoteSort::TitleDescending => !previous
            .display_title()
            .eq_ignore_ascii_case(updated.display_title()),
    }
}

pub(super) fn remove_loaded_note(note_id: &NoteId, mut state: NotesListState) -> bool {
    let removed = state.note_cache.write().remove_note(note_id);
    let count = (*state.note_count.read()).saturating_sub(1);
    state.note_count.set(count);
    removed
}

fn compare_note_summaries(left: &NoteSummary, right: &NoteSummary, sort: NoteSort) -> Ordering {
    right.pinned.cmp(&left.pinned).then_with(|| match sort {
        NoteSort::UpdatedNewest => right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.id.cmp(&left.id)),
        NoteSort::UpdatedOldest => left
            .updated_at
            .cmp(&right.updated_at)
            .then_with(|| left.id.cmp(&right.id)),
        NoteSort::CreatedNewest => right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id)),
        NoteSort::TitleAscending => left
            .display_title()
            .to_ascii_lowercase()
            .cmp(&right.display_title().to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id)),
        NoteSort::TitleDescending => right
            .display_title()
            .to_ascii_lowercase()
            .cmp(&left.display_title().to_ascii_lowercase())
            .then_with(|| right.id.cmp(&left.id)),
    })
}
