//! Shared entity-action registry for contextual UI surfaces.
//!
//! The registry describes *what* actions are available for an entity. It does
//! not perform mutations; toolbar buttons, context menus, keyboard entry points,
//! and future command-palette surfaces can all dispatch the same actions to the
//! existing business-operation layer.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EntitySurface {
    Note,
    Notebook,
    Tag,
    Attachment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EntityAction {
    Open,
    Rename,
    TogglePin,
    EditTags,
    DuplicateNote,
    EncryptNote,
    UnlockNote,
    LockNote,
    RemoveNoteEncryption,
    MoveNoteTo(String),
    ToggleArchive,
    MoveToTrash,
    RestoreFromTrash,
    DeletePermanently,
    CreateChildNotebook,
    MoveNotebookTo(Option<String>),
    DeleteNotebook,
    RemoveTagFromNote,
    DeleteTag,
    SaveAttachmentAs,
    RevealAttachment,
    RemoveAttachmentFromNote,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EntityActionState {
    pub pinned: bool,
    pub archived: bool,
    pub trashed: bool,
    pub protected: bool,
    pub unlocked: bool,
    pub read_only: bool,
    pub busy: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EntityActionContext {
    pub surface: EntitySurface,
    pub entity_id: String,
    pub state: EntityActionState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActionTarget {
    pub id: Option<String>,
    pub label: String,
}

impl ActionTarget {
    pub(super) fn entity(id: String, label: String) -> Self {
        Self {
            id: Some(id),
            label,
        }
    }

    pub(super) fn root(label: impl Into<String>) -> Self {
        Self {
            id: None,
            label: label.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EntityActionTargets {
    pub move_targets: Vec<ActionTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EntityActionEntry {
    pub label: String,
    pub symbol: &'static str,
    pub action: Option<EntityAction>,
    pub enabled: bool,
    pub danger: bool,
    pub separator_before: bool,
    pub confirm_label: Option<String>,
    pub confirm_hint: Option<String>,
    pub disabled_hint: Option<String>,
    pub children: Vec<EntityActionEntry>,
}

impl EntityActionEntry {
    fn action(label: impl Into<String>, symbol: &'static str, action: EntityAction) -> Self {
        Self {
            label: label.into(),
            symbol,
            action: Some(action),
            enabled: true,
            danger: false,
            separator_before: false,
            confirm_label: None,
            confirm_hint: None,
            disabled_hint: None,
            children: Vec::new(),
        }
    }

    fn submenu(
        label: impl Into<String>,
        symbol: &'static str,
        children: Vec<EntityActionEntry>,
    ) -> Self {
        Self {
            label: label.into(),
            symbol,
            action: None,
            enabled: !children.is_empty(),
            danger: false,
            separator_before: false,
            confirm_label: None,
            confirm_hint: None,
            disabled_hint: None,
            children,
        }
    }

    fn separated(mut self) -> Self {
        self.separator_before = true;
        self
    }

    fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    fn disabled(mut self, hint: impl Into<String>) -> Self {
        self.enabled = false;
        self.disabled_hint = Some(hint.into());
        self
    }

    fn confirm(mut self, label: impl Into<String>, hint: impl Into<String>) -> Self {
        self.confirm_label = Some(label.into());
        self.confirm_hint = Some(hint.into());
        self
    }
}

pub(super) struct EntityActionRegistry;

impl EntityActionRegistry {
    pub(super) fn entries(
        context: &EntityActionContext,
        targets: &EntityActionTargets,
    ) -> Vec<EntityActionEntry> {
        if context.entity_id.trim().is_empty() {
            return Vec::new();
        }
        match context.surface {
            EntitySurface::Note => note_entries(context, targets),
            EntitySurface::Notebook => notebook_entries(context, targets),
            EntitySurface::Tag => tag_entries(context),
            EntitySurface::Attachment => attachment_entries(context),
        }
    }
}

fn note_entries(
    context: &EntityActionContext,
    targets: &EntityActionTargets,
) -> Vec<EntityActionEntry> {
    if context.state.trashed {
        return vec![
            EntityActionEntry::action("Restore", "↥", EntityAction::RestoreFromTrash),
            EntityActionEntry::action("Delete permanently", "×", EntityAction::DeletePermanently)
                .separated()
                .danger()
                .confirm("Confirm permanent delete", "This cannot be undone."),
        ];
    }

    let move_children = targets
        .move_targets
        .iter()
        .filter_map(|target| {
            target.id.as_ref().map(|id| {
                EntityActionEntry::action(
                    target.label.clone(),
                    "",
                    EntityAction::MoveNoteTo(id.clone()),
                )
            })
        })
        .collect();

    let mut entries = vec![EntityActionEntry::action("Open", "↗", EntityAction::Open)];

    if context.state.protected {
        entries.push(if context.state.unlocked {
            EntityActionEntry::action("Lock note", "🔒", EntityAction::LockNote)
        } else {
            EntityActionEntry::action("Unlock note…", "🔓", EntityAction::UnlockNote)
        });
        if context.state.unlocked {
            entries.push(
                EntityActionEntry::action(
                    "Remove encryption…",
                    "◇",
                    EntityAction::RemoveNoteEncryption,
                )
                .separated(),
            );
        }
    } else {
        entries.push(EntityActionEntry::action("Rename", "Aa", EntityAction::Rename));
        entries.push(EntityActionEntry::action("Tags…", "#", EntityAction::EditTags));
        entries.push(EntityActionEntry::action(
            "Duplicate",
            "⧉",
            EntityAction::DuplicateNote,
        ));
        entries.push(
            EntityActionEntry::action("Encrypt note…", "🔒", EntityAction::EncryptNote)
                .separated(),
        );
    }

    if !context.state.archived {
        entries.push(EntityActionEntry::action(
            if context.state.pinned { "Unpin" } else { "Pin" },
            "◆",
            EntityAction::TogglePin,
        ));
    }

    let move_entry = EntityActionEntry::submenu("Move to notebook", "▱", move_children);
    entries.push(if move_entry.enabled {
        move_entry
    } else {
        move_entry.disabled("No other notebook is available.")
    });
    entries.push(
        EntityActionEntry::action(
            if context.state.archived {
                "Restore from archive"
            } else {
                "Archive"
            },
            "▣",
            EntityAction::ToggleArchive,
        )
        .separated(),
    );
    entries.push(
        EntityActionEntry::action("Move to Trash", "◇", EntityAction::MoveToTrash)
            .separated()
            .danger(),
    );
    entries
}

fn notebook_entries(
    context: &EntityActionContext,
    targets: &EntityActionTargets,
) -> Vec<EntityActionEntry> {
    let move_children = targets
        .move_targets
        .iter()
        .map(|target| {
            EntityActionEntry::action(
                target.label.clone(),
                "",
                EntityAction::MoveNotebookTo(target.id.clone()),
            )
        })
        .collect::<Vec<_>>();

    let move_entry = EntityActionEntry::submenu("Move…", "↳", move_children);
    let move_entry = if context.state.protected {
        move_entry.disabled("The built-in Notes notebook stays at the top level.")
    } else {
        move_entry
    };

    let delete_entry =
        EntityActionEntry::action("Delete notebook", "×", EntityAction::DeleteNotebook)
            .separated()
            .danger()
            .confirm(
                "Confirm delete notebook",
                "Notes and child notebooks will be preserved.",
            );
    let delete_entry = if context.state.protected {
        delete_entry.disabled("The built-in Notes notebook cannot be deleted.")
    } else {
        delete_entry
    };

    vec![
        EntityActionEntry::action("Rename", "Aa", EntityAction::Rename),
        EntityActionEntry::action(
            "New notebook inside…",
            "+",
            EntityAction::CreateChildNotebook,
        ),
        move_entry,
        delete_entry,
    ]
}

fn tag_entries(context: &EntityActionContext) -> Vec<EntityActionEntry> {
    let locked = context.state.read_only || context.state.busy;
    let reason = if context.state.read_only {
        "Tags cannot be changed while this note is in Trash."
    } else {
        "Another metadata operation is still finishing."
    };

    let open = EntityActionEntry::action("Open tag", "↗", EntityAction::Open);
    let rename = EntityActionEntry::action("Rename tag", "Aa", EntityAction::Rename);
    let remove = EntityActionEntry::action(
        "Remove from this note",
        "−",
        EntityAction::RemoveTagFromNote,
    );
    let delete = EntityActionEntry::action("Delete tag everywhere", "×", EntityAction::DeleteTag)
        .separated()
        .danger()
        .confirm(
            "Confirm delete tag",
            "The tag will be removed from every note that uses it.",
        );

    if locked {
        vec![
            open,
            rename.disabled(reason),
            remove.disabled(reason),
            delete.disabled(reason),
        ]
    } else {
        vec![open, rename, remove, delete]
    }
}

fn attachment_entries(context: &EntityActionContext) -> Vec<EntityActionEntry> {
    let mut open = EntityActionEntry::action("Open", "↗", EntityAction::Open);
    let mut save =
        EntityActionEntry::action("Save as…", "⇩", EntityAction::SaveAttachmentAs);
    let mut reveal =
        EntityActionEntry::action("Reveal in Finder", "⌖", EntityAction::RevealAttachment);
    let mut remove = EntityActionEntry::action(
        "Remove from note",
        "×",
        EntityAction::RemoveAttachmentFromNote,
    )
    .separated()
    .danger();

    if context.state.busy {
        let hint = "Another attachment operation is still finishing.";
        open = open.disabled(hint);
        save = save.disabled(hint);
        reveal = reveal.disabled(hint);
        remove = remove.disabled(hint);
    } else if context.state.read_only {
        remove = remove.disabled("Recover this note from Trash before changing its contents.");
    }

    vec![open, save, reveal, remove]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trashed_note_exposes_only_restore_and_permanent_delete() {
        let context = EntityActionContext {
            surface: EntitySurface::Note,
            entity_id: "note_test".to_string(),
            state: EntityActionState {
                trashed: true,
                ..EntityActionState::default()
            },
        };
        let entries = EntityActionRegistry::entries(&context, &EntityActionTargets::default());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, Some(EntityAction::RestoreFromTrash));
        assert_eq!(entries[1].action, Some(EntityAction::DeletePermanently));
        assert!(entries[1].danger);
        assert!(entries[1].confirm_label.is_some());
    }

    #[test]
    fn archived_note_hides_pin_and_exposes_restore() {
        let context = EntityActionContext {
            surface: EntitySurface::Note,
            entity_id: "note_test".to_string(),
            state: EntityActionState {
                pinned: true,
                archived: true,
                ..EntityActionState::default()
            },
        };
        let entries = EntityActionRegistry::entries(&context, &EntityActionTargets::default());
        assert!(!entries.iter().any(|entry| entry.label == "Unpin"));
        assert!(
            entries
                .iter()
                .any(|entry| entry.label == "Restore from archive")
        );
    }

    #[test]
    fn normal_note_actions_follow_state_and_targets() {
        let context = EntityActionContext {
            surface: EntitySurface::Note,
            entity_id: "note_test".to_string(),
            state: EntityActionState {
                pinned: true,
                ..EntityActionState::default()
            },
        };
        let targets = EntityActionTargets {
            move_targets: vec![ActionTarget::entity(
                "book_target".to_string(),
                "Research".to_string(),
            )],
        };
        let entries = EntityActionRegistry::entries(&context, &targets);
        assert!(entries.iter().any(|entry| entry.label == "Unpin"));
        assert!(entries.iter().any(|entry| entry.action == Some(EntityAction::EditTags)));
        assert!(entries.iter().any(|entry| entry.action == Some(EntityAction::DuplicateNote)));
        assert!(entries.iter().any(|entry| entry.action == Some(EntityAction::EncryptNote)));
        let move_entry = entries
            .iter()
            .find(|entry| entry.label == "Move to notebook")
            .expect("move action should be present");
        assert!(move_entry.enabled);
        assert_eq!(
            move_entry.children[0].action,
            Some(EntityAction::MoveNoteTo("book_target".to_string()))
        );
    }

    #[test]
    fn locked_note_exposes_unlock_without_plaintext_mutations() {
        let context = EntityActionContext {
            surface: EntitySurface::Note,
            entity_id: "note_test".to_string(),
            state: EntityActionState {
                protected: true,
                ..EntityActionState::default()
            },
        };
        let entries = EntityActionRegistry::entries(&context, &EntityActionTargets::default());
        assert!(entries.iter().any(|entry| entry.action == Some(EntityAction::UnlockNote)));
        assert!(!entries.iter().any(|entry| entry.action == Some(EntityAction::Rename)));
        assert!(!entries.iter().any(|entry| entry.action == Some(EntityAction::EditTags)));
        assert!(!entries.iter().any(|entry| entry.action == Some(EntityAction::DuplicateNote)));
        assert!(!entries.iter().any(|entry| entry.action == Some(EntityAction::RemoveNoteEncryption)));
    }

    #[test]
    fn unlocked_protected_note_exposes_lock_and_remove_encryption() {
        let context = EntityActionContext {
            surface: EntitySurface::Note,
            entity_id: "note_test".to_string(),
            state: EntityActionState {
                protected: true,
                unlocked: true,
                ..EntityActionState::default()
            },
        };
        let entries = EntityActionRegistry::entries(&context, &EntityActionTargets::default());
        assert!(entries.iter().any(|entry| entry.action == Some(EntityAction::LockNote)));
        assert!(entries
            .iter()
            .any(|entry| entry.action == Some(EntityAction::RemoveNoteEncryption)));
        assert!(!entries.iter().any(|entry| entry.action == Some(EntityAction::Rename)));
    }


    #[test]
    fn note_move_stays_discoverable_but_disabled_without_other_notebooks() {
        let context = EntityActionContext {
            surface: EntitySurface::Note,
            entity_id: "note_test".to_string(),
            state: EntityActionState::default(),
        };
        let entries = EntityActionRegistry::entries(&context, &EntityActionTargets::default());
        let move_entry = entries
            .iter()
            .find(|entry| entry.label == "Move to notebook")
            .expect("move action should be present");
        assert!(!move_entry.enabled);
        assert_eq!(
            move_entry.disabled_hint.as_deref(),
            Some("No other notebook is available.")
        );
    }

    #[test]
    fn protected_notebook_keeps_discoverable_move_and_delete_disabled() {
        let context = EntityActionContext {
            surface: EntitySurface::Notebook,
            entity_id: "notebook_notes".to_string(),
            state: EntityActionState {
                protected: true,
                ..EntityActionState::default()
            },
        };
        let targets = EntityActionTargets {
            move_targets: vec![ActionTarget::root("Top level")],
        };
        let entries = EntityActionRegistry::entries(&context, &targets);
        let move_entry = entries
            .iter()
            .find(|entry| entry.label == "Move…")
            .expect("move action should be present");
        let delete_entry = entries
            .iter()
            .find(|entry| entry.label == "Delete notebook")
            .expect("delete action should be present");
        assert!(!move_entry.enabled);
        assert!(!delete_entry.enabled);
        assert!(move_entry.disabled_hint.is_some());
        assert!(delete_entry.disabled_hint.is_some());
    }

    #[test]
    fn read_only_tag_actions_remain_discoverable_but_disabled() {
        let context = EntityActionContext {
            surface: EntitySurface::Tag,
            entity_id: "example".to_string(),
            state: EntityActionState {
                read_only: true,
                ..EntityActionState::default()
            },
        };
        let entries = EntityActionRegistry::entries(&context, &EntityActionTargets::default());
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].action, Some(EntityAction::Open));
        assert!(entries[0].enabled);
        assert!(entries.iter().skip(1).all(|entry| !entry.enabled));
    }

    #[test]
    fn attachment_remove_is_disabled_but_non_destructive_actions_stay_enabled_in_trash() {
        let context = EntityActionContext {
            surface: EntitySurface::Attachment,
            entity_id: "att_test".to_string(),
            state: EntityActionState {
                read_only: true,
                ..EntityActionState::default()
            },
        };
        let entries = EntityActionRegistry::entries(&context, &EntityActionTargets::default());
        assert!(entries.iter().take(3).all(|entry| entry.enabled));
        let remove = entries
            .iter()
            .find(|entry| entry.action == Some(EntityAction::RemoveAttachmentFromNote))
            .expect("remove action should be present");
        assert!(!remove.enabled);
    }
}
