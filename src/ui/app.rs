use std::{sync::Arc, time::Duration};

use dioxus::prelude::*;
use dioxus_desktop::tao::event::{Event as TaoEvent, WindowEvent};

use crate::application::editor::EditorPersistenceEvent;
use crate::application::executor::{
    ApplicationExecutionStatus, ApplicationExecutor, ApplicationOperation,
};
use super::background::BackgroundOperationViewport;
use super::editor::{EditorPane, EditorPaneView};
use super::note_list_data::{
    CachedNoteUpdate, NoteListCache, execute_note_page_request, note_list_query_is_scoped,
    refresh_note_list, reload_note_list, update_loaded_note,
};
use super::notes_header::{
    NotesListPanel, NotesListPanelView, create_contextual_note, switch_library_section,
    toggle_selected_pin,
};
use super::responsive::{
    DEFAULT_NOTE_LIST_WIDTH, DEFAULT_SIDEBAR_WIDTH, MAX_NOTE_LIST_WIDTH, MAX_SIDEBAR_WIDTH,
    MIN_NOTE_LIST_WIDTH, MIN_SIDEBAR_WIDTH, ResizeState, SIDEBAR_OVERLAY_BREAKPOINT,
    SPLITTER_WIDTH, TRANSIENT_UI_SCRIPT, begin_note_list_resize, capture_resize_pointer,
    finish_resize, focus_search_input, layout_widths_for_resize, note_list_width_for_viewport,
    reset_note_list_width, reset_sidebar_width,
    resize_note_list_with_keyboard, resize_sidebar_with_keyboard,
    schedule_layout_widths_save, sidebar_width_for_viewport,
};
use super::settings::{SettingsOverlay, SettingsOverlayView};
use super::shortcuts_help::ShortcutsDialog;
use super::shutdown::{
    CloseDurabilityOverlay, CloseDurabilityRequest, CloseDurabilityState,
    begin_close_durability,
};
use super::sidebar::{Sidebar, SidebarView};
use super::startup::InitialApplication;
use super::synchronization::{
    EditorSession, EditorSynchronizationContext, SynchronizationMode, synchronize_then_begin_whole_library_operation,
};
use super::toast::{ProtectedStatusPublisher, ToastViewport, route_status_message};
use super::{
    CompactPane, ListPopover, NotesListState, ToastMessage, refresh_tags,
    request_note_list_preferences_save,
};
use crate::domain::{NoteId, Section};

#[component]
pub(super) fn RunningApp(initial: InitialApplication) -> Element {
    let task_scope = dioxus::core::current_scope_id();
    let InitialApplication {
        services: initial_services,
        notebooks: initial_notebooks,
        tags: initial_tags,
        notes: initial_notes,
        note_count: initial_note_count,
        next_note_cursor: initial_next_note_cursor,
        sort_order: initial_sort_order,
        filters: initial_filters,
        dark_mode: initial_dark_mode,
        editor_preferences: initial_editor_preferences,
        sidebar_width: initial_sidebar_width,
        note_list_width: initial_note_list_width,
    } = initial;
    let initial_privacy_publication = initial_services.privacy();
    let initial_selected_note = initial_notes
        .first()
        .and_then(|summary| initial_services.library().get_note(&summary.id).ok().flatten());
    let initial_note_cache = NoteListCache::from_first_page(initial_notes);
    let mut initial_page_cursors = vec![None];
    if let Some(cursor) = initial_next_note_cursor {
        initial_page_cursors.push(Some(cursor));
    }
    let services = use_signal(move || initial_services);
    let _privacy_publication_context =
        use_context_provider(move || initial_privacy_publication.clone());
    let editor_preferences = use_signal(move || initial_editor_preferences);
    let _editor_preferences_context = use_context_provider(move || editor_preferences);
    let preference_persistence_lock =
        use_signal(|| Arc::new(tokio::sync::Mutex::new(())));

    let notebooks = use_signal(move || initial_notebooks);
    let tags = use_signal(move || initial_tags);
    let section = use_signal(Section::default);
    let selected_notebook = use_signal(|| None::<String>);
    let mut search = use_signal(String::new);
    let mut search_generation = use_signal(|| 0_u64);
    let list_generation = use_signal(|| 0_u64);
    let mut persistence_generation = use_signal(|| 0_u64);
    let note_selection_generation = use_signal(|| 0_u64);
    let note_view_generation = use_signal(|| 0_u64);
    let privacy_state = use_signal(super::privacy::UiPrivacyState::default);
    let privacy_epoch = super::privacy::UiPrivacyEpoch::new(privacy_state);
    let list_request_sequence = use_signal(|| 0_u64);
    let list_page_request = use_signal(|| None::<super::note_list_data::NotePageRequest>);
    let sort_order = use_signal(move || initial_sort_order);
    let filters = use_signal(move || initial_filters);
    let preference_save_generation = use_signal(|| 0_u64);
    let tag_filter_query = use_signal(String::new);
    let mut show_shortcuts = use_signal(|| false);
    let mut show_settings = use_signal(|| false);
    let mut sidebar_overlay_open = use_signal(|| false);
    let mut compact_pane = use_signal(CompactPane::default);
    let mut note_find_request = use_signal(|| 0_u64);
    let mut list_popover = use_signal(|| None::<ListPopover>);
    let note_cache = use_signal(move || initial_note_cache);
    let note_count = use_signal(move || initial_note_count);
    let page_start_cursors = use_signal(move || initial_page_cursors);
    let list_loading = use_signal(|| false);
    let list_scroll_top = use_signal(|| 0.0_f64);
    let list_viewport_height = use_signal(|| 0.0_f64);
    let mut selected_note = use_signal(move || initial_selected_note);
    let vault_transition = use_signal(|| None::<super::VaultTransitionUiState>);
    let vault_action_busy = use_signal(|| None::<NoteId>);
    let editor_session = use_signal(|| None::<EditorSession>);
    let tag_editor_request = use_signal(|| None::<NoteId>);
    let layout_save_generation = use_signal(|| 0_u64);
    let mut close_durability = use_signal(CloseDurabilityState::default);
    let desktop = dioxus_desktop::use_window();
    let guarded_desktop = desktop.clone();
    let _native_close_guard = use_effect(move || {
        guarded_desktop
            .set_close_behavior(dioxus_desktop::WindowCloseBehaviour::WindowHides);
    });

    let background_operations = use_context::<Signal<ApplicationExecutionStatus>>();
    let background_coordinator = use_context::<ApplicationExecutor>();
    let _editing_availability_guard = use_effect(move || {
        let script = application_editing_availability_script(
            background_operations.read().editing_available,
        );
        spawn(async move {
            let _ = document::eval(&script).join::<bool>().await;
        });
    });

    let native_close_services = services.read().clone();
    let native_close_desktop = desktop.clone();
    let native_close_background = background_coordinator.clone();
    let _native_close_handler = dioxus_desktop::use_wry_event_handler(move |event, _| {
        if let TaoEvent::WindowEvent {
            event: WindowEvent::CloseRequested,
            window_id,
            ..
        } = event
            && *window_id == native_close_desktop.window.id()
        {
            if !native_close_background.editing_available() {
                let restore_desktop = native_close_desktop.clone();
                spawn(async move {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    restore_desktop.window.set_visible(true);
                    restore_desktop.window.set_focus();
                });
                return;
            }
            begin_close_durability(CloseDurabilityRequest {
                editor: native_close_services.editor(),
                runtime: native_close_services.runtime(),
                selected_note,
                editor_session,
                privacy_epoch,
                privacy: native_close_services.privacy(),
                privacy_operations: native_close_services.privacy_operations(),
                executor: native_close_background.clone(),
                list_generation,
                note_selection_generation,
                state: close_durability,
                desktop: native_close_desktop.clone(),
            });
        }
    });

    let dark_mode = use_signal(move || initial_dark_mode);
    let appearance_save_generation = use_signal(|| 0_u64);
    // `status` is an internal message bus. The router below separates durable save
    // state from temporary operation notifications.
    let mut status = use_signal(|| "Saved locally".to_string());
    let save_status = use_signal(|| "Saved locally".to_string());
    let mut toast = use_signal(|| None::<ToastMessage>);
    let toast_sequence = use_signal(|| 0_u64);
    let protected_status_publisher =
        ProtectedStatusPublisher::new(save_status, toast, toast_sequence);
    let _protected_status_context =
        use_context_provider(move || protected_status_publisher);

    let _status_router = use_effect(move || {
        route_status_message(status.read().clone(), save_status, toast, toast_sequence);
    });
    // Protected toast state retains its publication token. Reading the toast
    // and that token's bound generations makes this effect react immediately
    // to Lock/Protect, note selection replacement, or library replacement.
    let _protected_toast_revocation = use_effect(move || {
        if toast
            .read()
            .as_ref()
            .is_some_and(|message| !message.is_publishable())
        {
            toast.set(None);
        }
    });

    // A privacy transition can reclassify previously ordinary UI text as
    // sensitive (most notably Protect), so clear any existing global toast on
    // every UI privacy-generation change. `peek` deliberately avoids making
    // ordinary toast publication itself a dependency of this effect.
    let _privacy_transition_toast_clear = use_effect(move || {
        let _privacy_generation = privacy_epoch.observe_multi_note();
        if toast.peek().is_some() {
            toast.set(None);
        }
    });

    let _appearance_preference_persistence = use_effect(move || {
        let generation = *appearance_save_generation.read();
        if generation == 0 {
            return;
        }
        let enabled = *dark_mode.peek();
        let task_settings = services.peek().settings();
        let persistence_lock = preference_persistence_lock.peek().clone();
        spawn(async move {
            let _persistence_guard = persistence_lock.lock().await;
            if *appearance_save_generation.read() != generation {
                return;
            }
            let result = ApplicationExecutor::read("appearance preference", move || {
                task_settings
                    .set_dark_mode(enabled)
                    .map_err(super::task_error::DataTaskError::from)
            })
            .await;
            if *appearance_save_generation.read() != generation {
                return;
            }
            if let Err(error) = result {
                status.set(format!("Could not save appearance: {error}"));
            }
        });
    });

    let _note_list_preference_persistence = use_effect(move || {
        let generation = *preference_save_generation.read();
        if generation == 0 {
            return;
        }
        let task_library = services.peek().library();
        let persistence_lock = preference_persistence_lock.peek().clone();
        spawn(async move {
            let _persistence_guard = persistence_lock.lock().await;
            if *preference_save_generation.read() != generation {
                return;
            }
            let sort = sort_order.peek().value().to_string();
            let filter = filters.peek().to_storage();
            let result = ApplicationExecutor::read("note-list preference", move || {
                task_library
                    .set_note_list_preferences(&sort, &filter)
                    .map_err(super::task_error::DataTaskError::from)
            })
            .await;
            if *preference_save_generation.read() != generation {
                return;
            }
            if let Err(error) = result {
                status.set(format!("Could not save note-list preference: {error}"));
            }
        });
    });

    let library_state = NotesListState {
        task_scope,
        services,
        section,
        selected_notebook,
        search,
        search_generation,
        list_generation,
        note_selection_generation,
        note_view_generation,
        privacy_epoch,
        list_request_sequence,
        list_page_request,
        sort_order,
        filters,
        tags,
        preference_save_generation,
        note_cache,
        note_count,
        page_start_cursors,
        list_loading,
        list_scroll_top,
        list_viewport_height,
        selected_note,
        vault_transition,
        vault_action_busy,
        tag_editor_request,
        status,
        editor_session,
    };

    // Aggregate note-derived UI has a stricter lifetime than the selected editor.
    // Any note visibility transition invalidates the multi-note generation because
    // list summaries, pagination cursors, tag labels, backlinks/pickers and other
    // aggregate surfaces may retain plaintext derived from the transitioning note.
    // Drop the process-local aggregate caches immediately, then repopulate them
    // only after the revocation window closes and fresh multi-note authority can
    // be acquired. The selected note/editor is intentionally untouched here.
    let aggregate_privacy = use_memo(move || privacy_epoch.observe_multi_note());
    let mut aggregate_privacy_seen =
        use_signal(|| None::<(u64, u64, bool)>);
    let mut aggregate_list_generation = list_generation;
    let mut aggregate_list_page_request = list_page_request;
    let mut aggregate_note_cache = note_cache;
    let mut aggregate_page_start_cursors = page_start_cursors;
    let mut aggregate_list_loading = list_loading;
    let mut aggregate_tags = tags;
    let mut aggregate_filters = filters;
    let aggregate_services = services;
    let aggregate_status = status;
    let _aggregate_privacy_lifetime = use_effect(move || {
        let current = *aggregate_privacy.read();
        let previous = *aggregate_privacy_seen.peek();
        if previous.is_none() {
            aggregate_privacy_seen.set(Some(current));
            return;
        }
        if previous == Some(current) {
            return;
        }
        aggregate_privacy_seen.set(Some(current));

        if current.2 {
            let next_generation =
                (*aggregate_list_generation.peek()).wrapping_add(1);
            aggregate_list_generation.set(next_generation);
            aggregate_list_page_request.set(None);
            aggregate_note_cache.set(NoteListCache::default());
            aggregate_page_start_cursors.set(vec![None]);
            aggregate_list_loading.set(false);
            aggregate_tags.set(Vec::new());

            // Tag filters are themselves note-derived labels. Keeping them in a
            // live chip/filter cache after the matching protected note is revoked
            // would retain plaintext even though the catalog has been cleared.
            let mut next_filters = aggregate_filters.peek().clone();
            if !next_filters.tags.is_empty() {
                next_filters.tags.clear();
                aggregate_filters.set(next_filters.normalized());
                request_note_list_preferences_save(library_state);
            }
            return;
        }

        let task_library = aggregate_services.peek().library();
        library_state.spawn(async move {
            let Some(token) = library_state
                .wait_for_multi_note_publication_token(None)
                .await
            else {
                return;
            };
            let Some(_publication_lease) = token.begin_publication() else {
                return;
            };
            let _ = refresh_tags(&task_library, aggregate_tags, aggregate_status);
            reload_note_list(library_state);
        });
    });

    let _note_list_page_executor = use_effect(move || {
        let request = *list_request_sequence.read();
        if request == 0 {
            return;
        }
        execute_note_page_request(
            services.peek().library(),
            services.peek().editor(),
            library_state,
        );
    });

    let maintenance_runtime = services.read().runtime();
    let maintenance_library = services.read().library();
    let maintenance_background = background_coordinator.clone();
    let mut maintenance_status = status;
    let _maintenance_task = use_future(move || {
        let maintenance_runtime = maintenance_runtime.clone();
        let maintenance_library = maintenance_library.clone();
        let maintenance_background = maintenance_background.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let runtime_for_maintenance = maintenance_runtime.clone();
            match maintenance_background.clone().execute(ApplicationOperation::StartupMaintenance, move || {
                runtime_for_maintenance
                    .run_deferred_maintenance()
                    .map_err(super::task_error::DataTaskError::from)
            })
            .await
            {
                Ok(report) => {
                    if report.has_activity() {
                        let Some(publication_token) = library_state
                            .wait_for_multi_note_publication_token(None)
                            .await
                        else {
                            return;
                        };
                        let Some(_publication_lease) = publication_token.begin_publication()
                        else {
                            return;
                        };
                        let _ = refresh_tags(
                            &maintenance_library,
                            tags,
                            maintenance_status,
                        );
                        refresh_note_list(library_state);
                    }
                    if report.backup_recovery.found_corruption() {
                        maintenance_status.set(format!(
                            "Backup repository damage detected: {} artifact(s) were preserved in quarantine. See Settings → Diagnostics.",
                            report.backup_recovery.quarantined_artifact_count()
                        ));
                    }
                }
                Err(error) => {
                    maintenance_status.set(format!("Storage maintenance failed: {error}"));
                }
            }
        }
    });

    let ingest_recovery_runtime = services.read().runtime();
    let mut ingest_recovery_status = status;
    let _ingest_recovery_task = use_future(move || {
        let ingest_recovery_runtime = ingest_recovery_runtime.clone();
        async move {
            // Large pending ingests are intentionally recovered outside the global
            // tracked-operation queue. Their full-file hash may run for
            // minutes, but FINALIZE_LOCK still serializes attachment finalization.
            tokio::time::sleep(Duration::from_millis(500)).await;
            let result = ApplicationExecutor::read("attachment ingest recovery", move || {
                ingest_recovery_runtime
                    .reconcile_pending_attachment_ingests()
                    .map_err(super::task_error::DataTaskError::from)
            })
            .await;
            match result {
                Ok((_replayed, unresolved)) => {
                    if unresolved > 0 {
                        ingest_recovery_status.set(format!(
                            "{unresolved} interrupted attachment import(s) remain pending"
                        ));
                    }
                }
                Err(error) => {
                    ingest_recovery_status.set(format!(
                        "Attachment import recovery failed: {error}"
                    ));
                }
            }
        }
    });

    let _transient_ui_manager = use_future(move || async move {
        let eval = document::eval(TRANSIENT_UI_SCRIPT);
        let _ = eval.join::<bool>().await;
    });

    let trash_maintenance_runtime = services.read().runtime();
    let trash_maintenance_background = background_coordinator.clone();
    let mut trash_maintenance_status = status;
    let _trash_maintenance_task = use_future(move || {
        let trash_maintenance_runtime = trash_maintenance_runtime.clone();
        let trash_maintenance_background = trash_maintenance_background.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            loop {
                let runtime_for_trash = trash_maintenance_runtime.clone();
                let privacy_revocation = privacy_epoch.begin_multi_note_ui_revocation();
                let purge_result = trash_maintenance_background.clone().execute(ApplicationOperation::TrashMaintenance, move || {
                        runtime_for_trash
                            .purge_expired_notes()
                            .map_err(super::task_error::DataTaskError::from)
                    })
                .await;
                drop(privacy_revocation);
                match purge_result {
                    Ok(_) => {}
                    Err(error) => {
                        trash_maintenance_status
                            .set(format!("Trash maintenance failed: {error}"));
                    }
                }
                tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            }
        }
    });

    let automatic_backup_runtime = services.read().runtime();
    let automatic_backup_editor = services.read().editor();
    let automatic_backup_publication_privacy = services.read().privacy();
    let automatic_backup_operation_privacy = services.read().privacy_operations();
    let automatic_backup_background = background_coordinator.clone();
    let mut automatic_backup_status = status;
    let _automatic_backup_task = use_future(move || {
        let automatic_backup_runtime = automatic_backup_runtime.clone();
        let automatic_backup_editor = automatic_backup_editor.clone();
        let automatic_backup_publication_privacy = automatic_backup_publication_privacy.clone();
        let automatic_backup_operation_privacy = automatic_backup_operation_privacy.clone();
        let automatic_backup_background = automatic_backup_background.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            loop {
                let due_runtime = automatic_backup_runtime.clone();
                let due = ApplicationExecutor::read("automatic backup check", move || {
                    due_runtime
                        .automatic_backup_due()
                        .map_err(super::task_error::DataTaskError::from)
                })
                .await;
                match due {
                    Ok(false) => {
                        tokio::time::sleep(Duration::from_secs(60 * 60)).await;
                        continue;
                    }
                    Err(error) => {
                        automatic_backup_status
                            .set(format!("Automatic backup check failed: {error}"));
                        tokio::time::sleep(Duration::from_secs(60 * 60)).await;
                        continue;
                    }
                    Ok(true) => {}
                }

                let operation = ApplicationOperation::AutomaticBackup;
                let backup_turn = match automatic_backup_background
                    .clone()
                    .begin_operation(operation)
                    .await
                {
                    Ok(turn) => turn,
                    Err(error) => {
                        automatic_backup_status.set(format!(
                            "Automatic backup could not enter the storage queue: {error}"
                        ));
                        tokio::time::sleep(Duration::from_secs(60 * 60)).await;
                        continue;
                    }
                };
                let synchronization = EditorSynchronizationContext::new(
                    automatic_backup_editor.clone(),
                    selected_note,
                    editor_session,
                    privacy_epoch,
                    &automatic_backup_publication_privacy,
                    list_generation,
                    note_selection_generation,
                );
                let backup_lease = match synchronize_then_begin_whole_library_operation(
                    &backup_turn,
                    &synchronization,
                    &automatic_backup_operation_privacy,
                    SynchronizationMode::Persist,
                )
                .await
                {
                    Ok(lease) => lease,
                    Err(error) => {
                        drop(backup_turn);
                        automatic_backup_status.set(format!(
                            "Automatic backup could not synchronize the library: {error}"
                        ));
                        tokio::time::sleep(Duration::from_secs(60 * 60)).await;
                        continue;
                    }
                };
                let runtime_for_backup = automatic_backup_runtime.clone();
                match backup_turn.execute(move || {
                    runtime_for_backup
                        .maybe_create_automatic_backup(&backup_lease)
                        .map_err(super::task_error::DataTaskError::from)
                })
                .await
                {
                    Ok(_) => {}
                    Err(error) => {
                        automatic_backup_status.set(format!("Automatic backup failed: {error}"));
                    }
                }
                tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            }
        }
    });

    let notebook_name = use_signal(String::new);
    let show_notebook_form = use_signal(|| false);
    let mut sidebar_width = use_signal(move || initial_sidebar_width);
    let mut note_list_width = use_signal(move || initial_note_list_width);
    let mut viewport_width = use_signal(|| 1440_i32);
    let mut resize_state = use_signal(ResizeState::default);

    let _viewport_listener = use_future(move || async move {
        let mut eval = document::eval(
            r#"
let resizeFrame = 0;
const reportViewportWidth = () => {
    if (resizeFrame) return;
    resizeFrame = requestAnimationFrame(() => {
        resizeFrame = 0;
        dioxus.send(Math.max(0, Math.round(window.innerWidth)));
    });
};
reportViewportWidth();
window.addEventListener("resize", reportViewportWidth, { passive: true });
await new Promise((resolve) => window.addEventListener("beforeunload", resolve, { once: true }));
window.removeEventListener("resize", reportViewportWidth);
"#,
        );
        while let Ok(width) = eval.recv::<i32>().await {
            viewport_width.set(width);
        }
    });

    let persistence_listener = services.read().editor();
    let mut persistence_event_receiver =
        Some(persistence_listener.take_persistence_event_receiver());
    let _persistence_task = use_future(move || {
        let persistence_listener = persistence_listener.clone();
        let event_receiver = persistence_event_receiver.take().unwrap_or_else(|| {
            Err(crate::application::errors::EditorError::PersistenceListenerRestarted)
        });
        async move {
            let mut event_receiver = match event_receiver {
                Ok(receiver) => receiver,
                Err(error) => {
                    status.set(format!("Autosave event listener failed: {error}"));
                    return;
                }
            };
            while let Some(event) = event_receiver.recv().await {
                match event {
                    EditorPersistenceEvent::Saved(saved_note, version) => {
                        let saved_note = saved_note.as_ref();
                        if !persistence_listener.is_current_note_version(&saved_note.id, version) {
                            continue;
                        }
                        // Note-local autosave publication is independent from
                        // aggregate list/cache publication. Lock B must not discard
                        // a successful save/publication for unrelated note A.
                        let is_current = selected_note
                            .read()
                            .as_ref()
                            .is_some_and(|note| note.id == saved_note.id);

                        if let Some(note_token) = privacy_epoch.capture_note_publication(
                            &services.read().privacy(),
                            &saved_note.id,
                            note_selection_generation,
                        )
                            && let Some(_note_lease) = note_token.begin_publication()
                        {
                            let next_persistence_generation =
                                (*persistence_generation.peek()).wrapping_add(1);
                            persistence_generation.set(next_persistence_generation);
                            if selected_note
                                .read()
                                .as_ref()
                                .is_some_and(|note| note.id == saved_note.id)
                            {
                                selected_note.set(Some(saved_note.clone()));
                            }
                            status.set(if is_current {
                                "Saved locally".to_string()
                            } else {
                                "Background save complete".to_string()
                            });
                        }

                        if let Some(aggregate_token) = privacy_epoch
                            .capture_multi_note_publication(&services.read().privacy(), list_generation)
                            && let Some(_aggregate_lease) = aggregate_token.begin_publication()
                        {
                            let listed_current = note_cache
                                .read()
                                .note_by_id(&saved_note.id)
                                .cloned();
                            let tags_changed = listed_current
                                .as_ref()
                                .is_none_or(|note| note.tags != saved_note.tags);
                            let cache_update = update_loaded_note(saved_note, library_state);
                            if note_list_query_is_scoped(library_state)
                                || cache_update != CachedNoteUpdate::Updated
                            {
                                refresh_note_list(library_state);
                            }
                            if tags_changed {
                                let _ = refresh_tags(&services.read().library(), tags, status);
                            }
                        } else {
                            // Aggregate publication may be temporarily invalidated by
                            // another note's privacy transition. Retry only the list
                            // refresh; the note-local save above remains committed.
                            library_state.spawn(async move {
                                let Some(token) = library_state
                                    .wait_for_multi_note_publication_token(None)
                                    .await
                                else {
                                    return;
                                };
                                let Some(_lease) = token.begin_publication() else {
                                    return;
                                };
                                refresh_note_list(library_state);
                            });
                        }
                    }
                    EditorPersistenceEvent::Recovered(count) => {
                        status.set(if count == 1 {
                            "Recovered 1 unsaved draft after the previous shutdown".to_string()
                        } else {
                            format!(
                                "Recovered {count} unsaved drafts after the previous shutdown"
                            )
                        });
                    }
                    EditorPersistenceEvent::Failed(error) => {
                        status.set(format!("Autosave failed: {error}"));
                    }
                }
            }
        }
    });

    let current_section = *section.read();
    let preferred_sidebar_width = *sidebar_width.read();
    let preferred_note_list_width = *note_list_width.read();
    let current_resize_state = *resize_state.read();
    let (effective_sidebar_width, effective_note_list_width) = layout_widths_for_resize(
        *viewport_width.read(),
        preferred_sidebar_width,
        preferred_note_list_width,
        current_resize_state,
    );
    let note_list_left = if *viewport_width.read() <= SIDEBAR_OVERLAY_BREAKPOINT {
        0
    } else {
        effective_sidebar_width + SPLITTER_WIDTH
    };
    let layout_style = format!(
        "--sidebar-width: {effective_sidebar_width}px; --note-list-width: {effective_note_list_width}px; --preferred-sidebar-width: {preferred_sidebar_width}px; --preferred-note-list-width: {preferred_note_list_width}px;",
    );
    let mut app_classes = vec!["app"];
    if *dark_mode.read() {
        app_classes.push("dark");
    }
    if resize_state.read().is_active() {
        app_classes.push("resizing");
    }
    if *sidebar_overlay_open.read() {
        app_classes.push("sidebar-open");
    }
    if *compact_pane.read() == CompactPane::Editor {
        app_classes.push("compact-editor");
    }
    let editing_available = background_operations.read().editing_available;
    if !editing_available {
        app_classes.push("editing-paused");
    }
    let app_class = app_classes.join(" ");
    let close_durability_view = close_durability.read().clone();
    let retry_close_services = services.read().clone();
    let retry_close_desktop = desktop.clone();
    let retry_close_background = background_coordinator.clone();
    let shortcut_background = background_coordinator.clone();
    let pointer_move_background = background_coordinator.clone();
    let pointer_up_background = background_coordinator.clone();
    let pointer_cancel_background = background_coordinator.clone();

    rsx! {
        div {
            id: "rustnote-application",
            class: app_class,
            style: "{layout_style}",
            tabindex: "0",
            aria_busy: if editing_available { "false" } else { "true" },
            "data-editing-available": if editing_available { "true" } else { "false" },
            onkeydown: {
                move |event: KeyboardEvent| {
                    if close_durability.read().is_active() {
                        // The modal owns keyboard interaction while closure is
                        // pending or has failed. Do not run application shortcuts,
                        // but preserve native Tab/Enter/Space behaviour in it.
                        return;
                    }
                    if !shortcut_background.editing_available() {
                        return;
                    }
                    if event.is_composing() {
                        return;
                    }
                    let modifiers = event.modifiers();
                    let primary = modifiers.meta() || modifiers.ctrl();
                    let key = event.key();

                    if key == Key::Escape {
                        if *show_settings.read() {
                            show_settings.set(false);
                        } else if list_popover.read().is_some() {
                            list_popover.set(None);
                        } else if *show_shortcuts.read() {
                            show_shortcuts.set(false);
                        } else if *sidebar_overlay_open.read() {
                            sidebar_overlay_open.set(false);
                        } else if !search.read().is_empty() {
                            let current_generation = *search_generation.read();
                            search_generation.set(current_generation.wrapping_add(1));
                            search.set(String::new());
                            reload_note_list(library_state);
                        }
                        return;
                    }

                    if *show_settings.read() || *show_shortcuts.read() || !primary {
                        return;
                    }

                    match key {
                        Key::Character(value) if value.eq_ignore_ascii_case("n") => {
                            event.prevent_default();
                            create_contextual_note(services.read().clone(), library_state);
                            compact_pane.set(CompactPane::Editor);
                            sidebar_overlay_open.set(false);
                        }
                        Key::Character(value)
                            if value.eq_ignore_ascii_case("f") && modifiers.shift() =>
                        {
                            event.prevent_default();
                            focus_search_input();
                        }
                        Key::Character(value) if value.eq_ignore_ascii_case("f") => {
                            event.prevent_default();
                            if selected_note.read().is_some() {
                                let current = *note_find_request.read();
                                note_find_request.set(current.wrapping_add(1));
                            }
                        }
                        Key::Character(value) if value.eq_ignore_ascii_case("s") => {
                            event.prevent_default();
                            let task_services = services.read().clone();
                            let task_background = background_coordinator.clone();
                            spawn(async move {
                                let publication_privacy = task_services.privacy();
                                let operation_privacy = task_services.privacy_operations();
                                let operation_turn = match task_background
                                    .begin_operation(ApplicationOperation::DurabilityCheckpoint)
                                    .await
                                {
                                    Ok(turn) => turn,
                                    Err(error) => {
                                        status.set(format!("Save failed: {error}"));
                                        return;
                                    }
                                };
                                let synchronization = EditorSynchronizationContext::new(
                                    task_services.editor(),
                                    selected_note,
                                    editor_session,
                                    privacy_epoch,
                                    &publication_privacy,
                                    list_generation,
                                    note_selection_generation,
                                );
                                match synchronize_then_begin_whole_library_operation(
                                    &operation_turn,
                                    &synchronization,
                                    &operation_privacy,
                                    SynchronizationMode::Checkpoint,
                                )
                                .await
                                {
                                    Ok(checkpoint_lease) => {
                                        let checkpoint = operation_turn
                                            .execute(move || {
                                                task_services.runtime()
                                                    .durability_checkpoint(&checkpoint_lease)
                                                    .map_err(super::task_error::DataTaskError::from)
                                            })
                                            .await;
                                        match checkpoint {
                                            Ok(()) => status.set("Saved locally".to_string()),
                                            Err(error) => status.set(format!(
                                                "Save completed, but the durability checkpoint failed: {error}"
                                            )),
                                        }
                                    }
                                    Err(error) => {
                                        drop(operation_turn);
                                        status.set(format!("Save failed: {error}"));
                                    }
                                }
                            });
                        }
                        Key::Character(value)
                            if value.eq_ignore_ascii_case("p") && modifiers.shift() =>
                        {
                            event.prevent_default();
                            toggle_selected_pin(services.read().clone(), library_state);
                        }
                        Key::Character(value) if value == "/" => {
                            event.prevent_default();
                            let next = !*show_shortcuts.read();
                            show_shortcuts.set(next);
                        }
                        Key::Character(value) if value == "1" => {
                            event.prevent_default();
                            switch_library_section(
                                Section::Notes,
                                services.read().editor(),
                                library_state,
                            );
                            compact_pane.set(CompactPane::Notes);
                        }
                        Key::Character(value) if value == "2" => {
                            event.prevent_default();
                            switch_library_section(
                                Section::Pinned,
                                services.read().editor(),
                                library_state,
                            );
                            compact_pane.set(CompactPane::Notes);
                        }
                        Key::Character(value) if value == "3" => {
                            event.prevent_default();
                            switch_library_section(
                                Section::Archive,
                                services.read().editor(),
                                library_state,
                            );
                            compact_pane.set(CompactPane::Notes);
                        }
                        Key::Character(value) if value == "4" => {
                            event.prevent_default();
                            switch_library_section(
                                Section::Trash,
                                services.read().editor(),
                                library_state,
                            );
                            compact_pane.set(CompactPane::Notes);
                        }
                        _ => {}
                    }
                }
            },
            onpointermove: move |event| {
                if close_durability.read().is_active()
                    || !pointer_move_background.editing_available()
                {
                    return;
                }
                let current_resize = *resize_state.read();
                if !current_resize.matches_pointer(event.pointer_id()) {
                    return;
                }
                let pointer_x = event.client_coordinates().x.round() as i32;
                match current_resize {
                    ResizeState::Idle | ResizeState::NoteListPending { .. } => {}
                    ResizeState::Sidebar {
                        fixed_note_list_width,
                        ..
                    } => {
                        sidebar_width.set(sidebar_width_for_viewport(
                            pointer_x,
                            *viewport_width.read(),
                            fixed_note_list_width,
                        ));
                    }
                    ResizeState::NoteList { origin_x, .. } => {
                        note_list_width.set(note_list_width_for_viewport(
                            pointer_x,
                            origin_x,
                            *viewport_width.read(),
                        ));
                    }
                }
            },
            onpointerup: move |event| {
                if close_durability.read().is_active()
                    || !pointer_up_background.editing_available()
                {
                    return;
                }
                if resize_state.read().matches_pointer(event.pointer_id()) {
                    finish_resize(
                        services.read().settings(),
                        resize_state,
                        sidebar_width,
                        note_list_width,
                        layout_save_generation,
                        preference_persistence_lock.read().clone(),
                        status,
                    );
                }
            },
            onpointercancel: move |event| {
                if close_durability.read().is_active()
                    || !pointer_cancel_background.editing_available()
                {
                    return;
                }
                if resize_state.read().matches_pointer(event.pointer_id()) {
                    finish_resize(
                        services.read().settings(),
                        resize_state,
                        sidebar_width,
                        note_list_width,
                        layout_save_generation,
                        preference_persistence_lock.read().clone(),
                        status,
                    );
                }
            },

            a { class: "skip-link", href: "#note-list-main", "Skip to note list" }
            a { class: "skip-link second", href: "#note-editor-main", "Skip to editor" }

            if resize_state.read().is_active() {
                div { class: "resize-capture-layer", aria_hidden: "true" }
            }

            if *sidebar_overlay_open.read() {
                button {
                    class: "sidebar-overlay-scrim",
                    aria_label: "Close notebooks panel",
                    onclick: move |_| sidebar_overlay_open.set(false),
                }
            }

            Sidebar {
                view: SidebarView {
                    services,
                    library_state,
                    notebooks,
                    compact_pane,
                    sidebar_overlay_open,
                    notebook_name,
                    show_notebook_form,
                    show_settings,
                    current_section,
                }
            }

            div {
                id: "sidebar-splitter",
                class: "column-splitter sidebar-splitter",
                role: "separator",
                tabindex: "0",
                aria_label: "Resize notebooks panel",
                aria_orientation: "vertical",
                aria_valuemin: "{MIN_SIDEBAR_WIDTH}",
                aria_valuemax: "{MAX_SIDEBAR_WIDTH}",
                aria_valuenow: "{effective_sidebar_width}",
                title: "Drag to resize notebooks; use arrow keys; double-click to reset",
                ondoubleclick: move |_| {
                    let reset_width = reset_sidebar_width();
                    let note_width = *note_list_width.read();
                    sidebar_width.set(reset_width);
                    schedule_layout_widths_save(
                        services.read().settings(),
                        reset_width,
                        note_width,
                        layout_save_generation,
                        preference_persistence_lock.read().clone(),
                        status,
                    );
                },
                onkeydown: move |event| {
                    let next = match event.key() {
                        Key::ArrowLeft => {
                            Some(resize_sidebar_with_keyboard(effective_sidebar_width, -1))
                        }
                        Key::ArrowRight => {
                            Some(resize_sidebar_with_keyboard(effective_sidebar_width, 1))
                        }
                        Key::Home => Some(DEFAULT_SIDEBAR_WIDTH),
                        _ => None,
                    };
                    if let Some(next) = next {
                        event.prevent_default();
                        let next = sidebar_width_for_viewport(
                            next,
                            *viewport_width.read(),
                            effective_note_list_width,
                        );
                        sidebar_width.set(next);
                        schedule_layout_widths_save(
                            services.read().settings(),
                            next,
                            *note_list_width.read(),
                            layout_save_generation,
                            preference_persistence_lock.read().clone(),
                            status,
                        );
                    }
                },
                onpointerdown: move |event| {
                    event.prevent_default();
                    let pointer_id = event.pointer_id();
                    resize_state.set(ResizeState::begin_sidebar(
                        pointer_id,
                        effective_note_list_width,
                    ));
                    capture_resize_pointer("sidebar-splitter", pointer_id);
                }
            }

            NotesListPanel {
                view: NotesListPanelView {
                    services,
                    library_state,
                    notebooks,
                    tags,
                    sidebar_overlay_open,
                    compact_pane,
                    list_popover,
                    tag_filter_query,
                    show_shortcuts,
                }
            }

            div {
                id: "notes-splitter",
                class: "column-splitter notes-splitter",
                role: "separator",
                tabindex: "0",
                aria_label: "Resize note list",
                aria_orientation: "vertical",
                aria_valuemin: "{MIN_NOTE_LIST_WIDTH}",
                aria_valuemax: "{MAX_NOTE_LIST_WIDTH}",
                aria_valuenow: "{effective_note_list_width}",
                title: "Drag to resize note list; use arrow keys; double-click to reset",
                ondoubleclick: move |_| {
                    let reset_width = reset_note_list_width();
                    let side_width = *sidebar_width.read();
                    note_list_width.set(reset_width);
                    schedule_layout_widths_save(
                        services.read().settings(),
                        side_width,
                        reset_width,
                        layout_save_generation,
                        preference_persistence_lock.read().clone(),
                        status,
                    );
                },
                onkeydown: move |event| {
                    let next = match event.key() {
                        Key::ArrowLeft => {
                            Some(resize_note_list_with_keyboard(effective_note_list_width, -1))
                        }
                        Key::ArrowRight => {
                            Some(resize_note_list_with_keyboard(effective_note_list_width, 1))
                        }
                        Key::Home => Some(DEFAULT_NOTE_LIST_WIDTH),
                        _ => None,
                    };
                    if let Some(next) = next {
                        event.prevent_default();
                        let viewport_width = *viewport_width.read();
                        let next = note_list_width_for_viewport(
                            note_list_left + next,
                            note_list_left,
                            viewport_width,
                        );
                        note_list_width.set(next);
                        schedule_layout_widths_save(
                            services.read().settings(),
                            *sidebar_width.read(),
                            next,
                            layout_save_generation,
                            preference_persistence_lock.read().clone(),
                            status,
                        );
                    }
                },
                onpointerdown: move |event| {
                    event.prevent_default();
                    let pointer_id = event.pointer_id();
                    resize_state.set(ResizeState::begin_note_list(
                        pointer_id,
                        effective_sidebar_width,
                    ));
                    begin_note_list_resize(pointer_id, note_list_left, resize_state);
                }
            }

            EditorPane {
                view: EditorPaneView {
                    services,
                    notebooks,
                    library_state,
                    compact_pane,
                    note_find_request,
                    save_status,
                    persistence_generation,
                    privacy_epoch,
                }
            }

            if !editing_available {
                div {
                    class: "editing-pause-shield",
                    aria_hidden: "true",
                }
            }

            BackgroundOperationViewport { status: background_operations }

            if let Some(current_toast) = toast.read().clone()
                && current_toast.is_publishable()
            {
                ToastViewport {
                    message: current_toast,
                    ondismiss: move |_| toast.set(None),
                }
            }

            if *show_settings.read() {
                SettingsOverlay {
                    view: SettingsOverlayView {
                        services,
                        notebooks,
                        tags,
                        library_state,
                        compact_pane,
                        show_settings,
                        show_shortcuts,
                        dark_mode,
                        appearance_save_generation,
                    }
                }
            }

            if *show_shortcuts.read() {
                ShortcutsDialog { onclose: move |_| show_shortcuts.set(false) }
            }

            if close_durability_view.is_active() {
                CloseDurabilityOverlay {
                    state: close_durability_view,
                    onretry: move |_| {
                        begin_close_durability(CloseDurabilityRequest {
                            editor: retry_close_services.editor(),
                            runtime: retry_close_services.runtime(),
                            selected_note,
                            editor_session,
                            privacy_epoch,
                            privacy: retry_close_services.privacy(),
                            privacy_operations: retry_close_services.privacy_operations(),
                            executor: retry_close_background.clone(),
                            list_generation,
                            note_selection_generation,
                            state: close_durability,
                            desktop: retry_close_desktop.clone(),
                        });
                    },
                    oncancel: move |_| close_durability.set(CloseDurabilityState::Idle),
                }
            }
        }
    }
}

fn application_editing_availability_script(editing_available: bool) -> String {
    let paused = !editing_available;
    format!(
        r#"
const root = document.getElementById('rustnote-application');
if (!root) return false;
const exemptSelector = '.background-operation-status, #operation-toast, .close-durability-backdrop, .editing-pause-shield';
for (const child of root.children) {{
  const exempt = child.matches(exemptSelector);
  child.inert = {paused} && !exempt;
}}
const active = document.activeElement;
if ({paused} && active instanceof HTMLElement && root.contains(active) && !active.closest(exemptSelector)) {{
  active.blur();
}}
return true;
"#
    )
}

