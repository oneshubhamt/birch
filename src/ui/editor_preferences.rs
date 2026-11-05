use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

use dioxus::prelude::*;

use crate::application::{
    executor::ApplicationExecutor,
    settings::{EditorPreferences, SettingsService},
};

use super::{live_editor::live_editor_preferences_script, task_error::DataTaskError};

pub(super) fn apply_editor_preferences(preferences: EditorPreferences) {
    let script = live_editor_preferences_script(preferences.normalized());
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
}

static EDITOR_PREFERENCE_SAVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static EDITOR_PREFERENCE_SAVE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

pub(super) fn persist_editor_preferences(
    settings: SettingsService,
    preferences: EditorPreferences,
    onstatus: EventHandler<String>,
) {
    let preferences = preferences.normalized();
    let generation = EDITOR_PREFERENCE_SAVE_GENERATION
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);
    spawn(async move {
        let persistence_lock =
            EDITOR_PREFERENCE_SAVE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
        let _persistence_guard = persistence_lock.lock().await;
        if EDITOR_PREFERENCE_SAVE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let result = ApplicationExecutor::read("editor preferences", move || {
            settings
                .set_editor_preferences(preferences)
                .map_err(DataTaskError::from)
        })
        .await;
        if EDITOR_PREFERENCE_SAVE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Err(error) = result {
            onstatus.call(format!("Could not save editor preferences: {error}"));
        }
    });
}

pub(super) fn commit_editor_preferences(
    signal: &mut Signal<EditorPreferences>,
    settings: SettingsService,
    preferences: EditorPreferences,
    onstatus: EventHandler<String>,
) {
    let preferences = preferences.normalized();
    signal.set(preferences);
    apply_editor_preferences(preferences);
    persist_editor_preferences(settings, preferences, onstatus);
}
