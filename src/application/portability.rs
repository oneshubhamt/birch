use std::path::{Path, PathBuf};

use crate::{
    portability::{ImportPlan, ImportSummary, ImportUndoToken, PortabilityFormat, PortabilityScope},
    storage::{LibraryStore, StorageError},
};

use super::{
    errors::PortabilityError,
    executor::{ApplicationExecutionTurn, ApplicationExecutor, ApplicationOperation},
    privacy::WholeLibraryOperationLease,
};

/// Application boundary for additive import/export workflows.
///
/// Storage owns parsing, canonical validation, attachment IO, and transactional
/// persistence. UI owns destination/source dialogs and presentation only.
#[derive(Clone, PartialEq)]
pub(crate) struct PortabilityService {
    storage: LibraryStore,
}

impl PortabilityService {
    pub(crate) fn new(storage: LibraryStore) -> Self {
        Self { storage }
    }

    pub(crate) async fn inspect_source(
        &self,
        source: PathBuf,
    ) -> Result<ImportPlan, PortabilityError> {
        let storage = self.storage.clone();
        ApplicationExecutor::read("portability inspection", move || {
            storage.prepare_portability_import(&source).map_err(PortabilityError::from)
        })
        .await
    }

    pub(crate) async fn import_plan(
        &self,
        executor: &ApplicationExecutor,
        plan: ImportPlan,
    ) -> Result<ImportSummary, PortabilityError> {
        let storage = self.storage.clone();
        executor
            .execute(ApplicationOperation::ImportPortability, move || {
                storage.import_portability_plan(&plan).map_err(PortabilityError::from)
            })
            .await
    }

    pub(crate) async fn undo_import(
        &self,
        executor: &ApplicationExecutor,
        token: ImportUndoToken,
    ) -> Result<(), PortabilityError> {
        let storage = self.storage.clone();
        executor
            .execute(ApplicationOperation::UndoPortabilityImport, move || {
                storage.undo_portability_import(&token).map_err(PortabilityError::from)
            })
            .await
    }

    pub(crate) async fn export_scope(
        &self,
        executor: &ApplicationExecutor,
        scope: PortabilityScope,
        format: PortabilityFormat,
        destination: PathBuf,
    ) -> Result<PathBuf, PortabilityError> {
        let storage = self.storage.clone();
        match scope {
            PortabilityScope::Note(note_id) => executor
                .execute(ApplicationOperation::ExportPortability, move || {
                    storage
                        .export_portable_note_scope(&note_id, format, &destination)
                        .map_err(PortabilityError::from)
                })
                .await,
            PortabilityScope::Notebook(notebook_id) => executor
                .execute(ApplicationOperation::ExportPortability, move || {
                    storage
                        .export_portable_notebook_scope(&notebook_id, format, &destination)
                        .map_err(PortabilityError::from)
                })
                .await,
            PortabilityScope::Library => Err(PortabilityError::from(StorageError::Validation(
                "library portability export requires whole-library privacy authority".to_string(),
            ))),
        }
    }

    pub(crate) async fn export_library_scope(
        &self,
        operation_turn: ApplicationExecutionTurn,
        _privacy_lease: &WholeLibraryOperationLease,
        format: PortabilityFormat,
        destination: PathBuf,
    ) -> Result<PathBuf, PortabilityError> {
        let storage = self.storage.clone();
        operation_turn
            .execute(move || {
                storage
                    .export_portable_library_scope(format, &destination)
                    .map_err(PortabilityError::from)
            })
            .await
    }
}

pub(crate) fn suggested_portable_note_file_name(title: &str, format: PortabilityFormat) -> String {
    super::export::suggested_note_file_name(title, format.extension())
}

pub(crate) fn suggested_collection_directory_name(label: &str, format: PortabilityFormat) -> String {
    let mut stem = label
        .trim()
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '\0' => '-',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();
    stem = stem.split_whitespace().collect::<Vec<_>>().join(" ");
    let stem = stem.trim_matches(|character| character == '.' || character == ' ');
    let stem = if stem.is_empty() { "Rustnote export" } else { stem };
    format!("{stem} ({})", format.label())
}

pub(crate) fn portable_format_filter(format: PortabilityFormat) -> (&'static str, &'static [&'static str]) {
    match format {
        PortabilityFormat::PlainText => ("Plain text", &["txt"]),
        PortabilityFormat::Markdown => ("Markdown", &["md", "markdown"]),
        PortabilityFormat::Html => ("HTML", &["html", "htm"]),
        PortabilityFormat::RustnoteBundle => ("Rustnote bundle", &["rustnote"]),
    }
}

pub(crate) fn destination_for_collection(
    parent: &Path,
    label: &str,
    format: PortabilityFormat,
) -> PathBuf {
    let base = suggested_collection_directory_name(label, format);
    let candidate = parent.join(&base);
    if !candidate.exists() {
        return candidate;
    }
    for index in 2..10_000 {
        let candidate = parent.join(format!("{base} {index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{base} {}", crate::application::persistence::unix_timestamp()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_filters_match_portability_extensions() {
        for format in PortabilityFormat::ALL {
            let (_, extensions) = portable_format_filter(format);
            assert!(extensions.iter().any(|value| value.eq_ignore_ascii_case(format.extension())));
        }
    }
}
