use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    build_info::{APP_VERSION, BUILD_ID},
    domain::NoteFilters,
};

use super::{Storage, StorageError, StorageResult, unix_timestamp};

const TRASH_RETENTION_KEY: &str = "trash.retention_days";
const EDITOR_AUTO_INDENT_KEY: &str = "editor.auto_indent";
const EDITOR_AUTO_FIRST_LINE_INDENT_KEY: &str = "editor.auto_first_line_indent";
const EDITOR_DEFAULT_LINE_SPACING_KEY: &str = "editor.default_line_spacing";
const EDITOR_SPELLCHECK_KEY: &str = "editor.spellcheck";
pub const DEFAULT_TRASH_RETENTION_DAYS: u32 = 30;
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

pub(super) fn record_build_diagnostics(connection: &Connection) -> StorageResult<()> {
    connection.execute(
        "INSERT INTO settings(key, value) VALUES ('last_opened_by_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value
         WHERE settings.value <> excluded.value",
        [APP_VERSION],
    )?;
    connection.execute(
        "INSERT INTO settings(key, value) VALUES ('last_opened_by_build', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value
         WHERE settings.value <> excluded.value",
        [BUILD_ID],
    )?;
    Ok(())
}

impl Storage {
    pub fn trash_retention_days(&self) -> StorageResult<Option<u32>> {
        let Some(value) = self.setting(TRASH_RETENTION_KEY)? else {
            return Ok(Some(DEFAULT_TRASH_RETENTION_DAYS));
        };
        if value == "never" {
            return Ok(None);
        }
        let days = value.parse::<u32>().map_err(|_| {
            StorageError::Validation("trash retention setting is invalid".to_string())
        })?;
        if days == 0 {
            return Err(StorageError::Validation(
                "trash retention must be at least one day or disabled".to_string(),
            ));
        }
        Ok(Some(days))
    }

    pub fn trash_deletion_deadline(&self) -> StorageResult<Option<i64>> {
        Ok(self
            .trash_retention_days()?
            .map(|days| trash_deadline_at(unix_timestamp(), days)))
    }

    /// Persist the global Trash policy and apply it to every note currently in
    /// Trash. Reapplying the policy from "now" keeps the setting predictable
    /// without requiring a separate trashed-at column or migration.
    pub fn set_trash_retention_days(&self, days: Option<u32>) -> StorageResult<usize> {
        if matches!(days, Some(0)) {
            return Err(StorageError::Validation(
                "trash retention must be at least one day or disabled".to_string(),
            ));
        }

        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored_value = days
            .map(|value| value.to_string())
            .unwrap_or_else(|| "never".to_string());
        transaction.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)\n             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![TRASH_RETENTION_KEY, stored_value],
        )?;
        let updated = match days {
            Some(days) => transaction.execute(
                "UPDATE notes SET purge_after = ?1 WHERE trashed = 1",
                [trash_deadline_at(unix_timestamp(), days)],
            )?,
            None => transaction.execute(
                "UPDATE notes SET purge_after = NULL WHERE trashed = 1 AND purge_after IS NOT NULL",
                [],
            )?,
        };
        transaction.commit()?;
        Ok(updated)
    }

    pub(super) fn reconcile_trash_retention_unlocked(&self) -> StorageResult<usize> {
        let retention = self.trash_retention_days()?;
        let connection = self.connection()?;
        match retention {
            Some(days) => Ok(connection.execute(
                "UPDATE notes SET purge_after = ?1\n                 WHERE trashed = 1 AND purge_after IS NULL",
                [trash_deadline_at(unix_timestamp(), days)],
            )?),
            None => Ok(connection.execute(
                "UPDATE notes SET purge_after = NULL\n                 WHERE trashed = 1 AND purge_after IS NOT NULL",
                [],
            )?),
        }
    }

    pub fn editor_preferences(&self) -> StorageResult<(bool, Option<u32>, Option<String>, bool)> {
        let auto_indent = self.setting(EDITOR_AUTO_INDENT_KEY)?.as_deref() == Some("true");
        let auto_first_line_indent = self
            .setting(EDITOR_AUTO_FIRST_LINE_INDENT_KEY)?
            .and_then(|value| value.parse::<u32>().ok());
        let default_line_spacing = self
            .setting(EDITOR_DEFAULT_LINE_SPACING_KEY)?
            .filter(|value| value != "auto");
        let spellcheck = self.setting(EDITOR_SPELLCHECK_KEY)?.as_deref() != Some("false");
        Ok((auto_indent, auto_first_line_indent, default_line_spacing, spellcheck))
    }

    pub fn set_editor_preferences(
        &self,
        auto_indent: bool,
        auto_first_line_indent: u32,
        default_line_spacing: Option<&str>,
        spellcheck: bool,
    ) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (key, value) in [
            (
                EDITOR_AUTO_INDENT_KEY,
                if auto_indent { "true" } else { "false" }.to_string(),
            ),
            (
                EDITOR_AUTO_FIRST_LINE_INDENT_KEY,
                auto_first_line_indent.to_string(),
            ),
            (
                EDITOR_DEFAULT_LINE_SPACING_KEY,
                default_line_spacing.unwrap_or("auto").to_string(),
            ),
            (
                EDITOR_SPELLCHECK_KEY,
                if spellcheck { "true" } else { "false" }.to_string(),
            ),
        ] {
            transaction.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn dark_mode(&self) -> StorageResult<bool> {
        Ok(self.setting("dark_mode")?.as_deref() == Some("true"))
    }
    pub fn set_dark_mode(&self, enabled: bool) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        self.set_setting("dark_mode", if enabled { "true" } else { "false" })
    }
    pub fn layout_widths(&self) -> StorageResult<(i32, i32)> {
        let sidebar = self
            .setting("layout_sidebar_width")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(238);
        let note_list = self
            .setting("layout_note_list_width")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(340);
        Ok((sidebar.clamp(190, 380), note_list.clamp(280, 620)))
    }
    pub fn set_layout_widths(&self, sidebar: i32, note_list: i32) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO settings(key, value) VALUES ('layout_sidebar_width', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![sidebar.clamp(190, 380).to_string()],
        )?;
        transaction.execute(
            "INSERT INTO settings(key, value) VALUES ('layout_note_list_width', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![note_list.clamp(280, 620).to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn right_panel_width(&self) -> StorageResult<i32> {
        let width = self
            .setting("layout_right_panel_width")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(420);
        Ok(width.clamp(280, 720))
    }

    pub fn set_right_panel_width(&self, width: i32) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        self.set_setting(
            "layout_right_panel_width",
            &width.clamp(280, 720).to_string(),
        )
    }

    pub fn note_list_preferences(&self) -> StorageResult<(String, String)> {
        let sort = self
            .setting("note_list_sort")?
            .unwrap_or_else(|| "updated-newest".to_string());
        let filter = self
            .setting("note_list_filter")?
            .unwrap_or_else(|| "all".to_string());
        Ok((sort, filter))
    }
    pub fn set_note_list_preferences(&self, sort: &str, filter: &str) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let filter = sanitize_note_list_filter_value(&transaction, filter)?;
        transaction.execute(
            "INSERT INTO settings(key, value) VALUES ('note_list_sort', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [sort],
        )?;
        transaction.execute(
            "INSERT INTO settings(key, value) VALUES ('note_list_filter', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [filter],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn build_diagnostics(&self) -> StorageResult<(Option<String>, Option<String>)> {
        Ok((
            self.setting("last_opened_by_version")?,
            self.setting("last_opened_by_build")?,
        ))
    }
    pub(super) fn setting(&self, key: &str) -> StorageResult<Option<String>> {
        let connection = self.connection()?;
        Ok(connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }
    pub(super) fn set_setting(&self, key: &str, value: &str) -> StorageResult<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

pub(super) fn scrub_persisted_note_list_filter_tags(
    transaction: &Transaction<'_>,
) -> StorageResult<bool> {
    let Some(current) = transaction
        .query_row(
            "SELECT value FROM settings WHERE key = 'note_list_filter'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    else {
        return Ok(false);
    };
    let sanitized = sanitize_note_list_filter_value(transaction, &current)?;
    if sanitized == current {
        return Ok(false);
    }
    transaction.execute(
        "UPDATE settings SET value = ?1 WHERE key = 'note_list_filter'",
        [sanitized],
    )?;
    Ok(true)
}

fn sanitize_note_list_filter_value(
    transaction: &Transaction<'_>,
    value: &str,
) -> StorageResult<String> {
    let mut filters = NoteFilters::from_storage(value);
    if filters.tags.is_empty() {
        return Ok(filters.to_storage());
    }

    let mut statement = transaction.prepare(
        "SELECT DISTINCT t.name
         FROM tags t
         JOIN note_tags nt ON nt.tag_id = t.id",
    )?;
    let public_tags = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|tag| tag.to_lowercase())
        .collect::<HashSet<_>>();
    filters
        .tags
        .retain(|tag| public_tags.contains(&tag.to_lowercase()));
    Ok(filters.normalized().to_storage())
}

pub(super) fn trash_deadline_at(now: i64, days: u32) -> i64 {
    now.saturating_add(i64::from(days).saturating_mul(SECONDS_PER_DAY))
}
