use crate::application::executor::ApplicationExecutor;
use crate::autosave::{AutosaveController, AutosaveError};
use crate::library::{LibraryContext, LibraryRuntime, LibraryRuntimeError};
use crate::storage::StorageError;

use super::{
    attachments::AttachmentService,
    editor::EditorService,
    editor_workflows::EditorWorkflowService,
    export::ExportService,
    library::LibraryService,
    notes::NoteService,
    portability::PortabilityService,
    privacy::{PrivacyOperationService, PrivacyPublicationService},
    runtime::RuntimeService,
    settings::SettingsService,
    vault::VaultProtectionService,
};

#[derive(Debug)]
pub(crate) enum ApplicationOpenError {
    Storage(StorageError),
    Autosave(AutosaveError),
    LibraryRuntime(LibraryRuntimeError),
}

impl std::fmt::Display for ApplicationOpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => std::fmt::Display::fmt(error, formatter),
            Self::Autosave(error) => std::fmt::Display::fmt(error, formatter),
            Self::LibraryRuntime(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ApplicationOpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Autosave(error) => Some(error),
            Self::LibraryRuntime(error) => Some(error),
        }
    }
}

impl From<StorageError> for ApplicationOpenError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<AutosaveError> for ApplicationOpenError {
    fn from(error: AutosaveError) -> Self {
        Self::Autosave(error)
    }
}

impl From<LibraryRuntimeError> for ApplicationOpenError {
    fn from(error: LibraryRuntimeError) -> Self {
        Self::LibraryRuntime(error)
    }
}

/// Concrete application capabilities for one opened library.
///
/// UI code may clone this lightweight handle, but cannot obtain unrestricted `Storage`.
/// Consumers should take/use the narrow service matching the workflow they perform.
#[derive(Clone, PartialEq)]
pub(crate) struct ApplicationServices {
    library: LibraryService,
    notes: NoteService,
    settings: SettingsService,
    runtime: RuntimeService,
    editor: EditorService,
    attachments: AttachmentService,
    export: ExportService,
    portability: PortabilityService,
    privacy: PrivacyPublicationService,
    privacy_operations: PrivacyOperationService,
    vault: VaultProtectionService,
}

impl ApplicationServices {
    pub(crate) fn open(libraries: &LibraryRuntime) -> Result<Self, ApplicationOpenError> {
        let library = LibraryContext::open()?;
        let storage = library.storage().clone();
        let autosave = AutosaveController::try_new(storage.clone())?;
        let services = Self::new(&library, autosave.clone());
        services.runtime.cleanup_library_transients();
        libraries.activate(library)?;
        Ok(services)
    }

    fn new(library: &LibraryContext, autosave: AutosaveController) -> Self {
        let storage = library.storage().clone();
        Self {
            library: LibraryService::new(storage.library_store()),
            notes: NoteService::new(storage.note_store()),
            settings: SettingsService::new(storage.settings_store()),
            runtime: RuntimeService::new(storage.runtime_store(), library.paths().clone()),
            editor: EditorService::new(storage.editor_store(), autosave.clone()),
            attachments: AttachmentService::new(storage.attachment_store(), autosave.clone()),
            export: ExportService::new(storage.export_store()),
            portability: PortabilityService::new(storage.library_store()),
            privacy: PrivacyPublicationService::new(storage.privacy_publication_store()),
            privacy_operations: PrivacyOperationService::new(storage.privacy_operation_store()),
            vault: VaultProtectionService::new(storage.vault_store(), autosave),
        }
    }

    pub(crate) fn library(&self) -> LibraryService {
        self.library.clone()
    }

    pub(crate) fn notes(&self) -> NoteService {
        self.notes.clone()
    }

    pub(crate) fn settings(&self) -> SettingsService {
        self.settings.clone()
    }

    pub(crate) fn runtime(&self) -> RuntimeService {
        self.runtime.clone()
    }

    pub(crate) fn editor(&self) -> EditorService {
        self.editor.clone()
    }

    pub(crate) fn attachments(&self) -> AttachmentService {
        self.attachments.clone()
    }

    pub(crate) fn export(&self) -> ExportService {
        self.export.clone()
    }

    pub(crate) fn portability(&self) -> PortabilityService {
        self.portability.clone()
    }

    pub(crate) fn privacy(&self) -> PrivacyPublicationService {
        self.privacy.clone()
    }

    pub(crate) fn privacy_operations(&self) -> PrivacyOperationService {
        self.privacy_operations.clone()
    }

    pub(crate) fn vault(&self) -> VaultProtectionService {
        self.vault.clone()
    }

    pub(crate) fn editor_workflows(&self, executor: ApplicationExecutor) -> EditorWorkflowService {
        EditorWorkflowService::new(
            self.editor.clone(),
            self.attachments.clone(),
            self.export.clone(),
            executor,
        )
    }
}
