use std::path::Path;

use rfd::AsyncFileDialog;

use crate::application::errors::ExportError;
use crate::application::executor::ApplicationExecutor;
use crate::application::privacy::WholeLibraryOperationLease;
use crate::domain::NoteId;
use crate::storage::ExportStore;

/// Canonical application-level export service. UI surfaces select a scope and
/// destination; storage owns durable, verified serialization and attachment IO.
#[derive(Clone, PartialEq)]
pub(crate) struct ExportService {
    storage: ExportStore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PdfExportOutcome {
    Saved,
    Cancelled,
}

/// Retains Rustnote's note-scoped authorization for the complete native export
/// interaction. Protect/Unprotect/Delete cannot cross the derivative lifetime;
/// a protected note additionally retains its exact Vault read permit so Lock
/// cannot complete while a chooser/print dialog still displays plaintext.
#[derive(Debug)]
pub(crate) struct NoteExportLifetime {
    resource: crate::storage::NoteExportResource,
}

impl NoteExportLifetime {
    pub(crate) fn is_publishable(&self) -> bool {
        self.resource.is_publishable()
    }
}

impl ExportService {
    pub(crate) fn new(storage: ExportStore) -> Self {
        Self { storage }
    }

    pub(crate) async fn begin_note_export_lifetime(
        &self,
        note_id: NoteId,
    ) -> Result<NoteExportLifetime, ExportError> {
        let storage = self.storage.clone();
        ApplicationExecutor::read("note export authorization", move || {
            storage
                .note_export_resource(&note_id)
                .map(|resource| NoteExportLifetime { resource })
                .map_err(ExportError::from)
        })
        .await
    }

    pub(crate) fn export_archive(
        &self,
        _privacy_lease: &WholeLibraryOperationLease,
        destination: &Path,
    ) -> Result<(), ExportError> {
        self.storage
            .export_complete_archive(destination)
            .map_err(ExportError::from)
    }

    /// Save a PDF staged by the embedded viewer. The application service owns lookup,
    /// destination selection, staged-file cleanup, and the final native handoff.
    pub(crate) async fn save_staged_pdf_export(
        &self,
        note_id: NoteId,
        attachment_id: String,
        export_token: String,
    ) -> Result<PdfExportOutcome, ExportError> {
        let prepare_storage = self.storage.clone();
        let prepare_note_id = note_id.clone();
        let prepare_attachment_id = attachment_id.clone();
        let resource = match ApplicationExecutor::read("PDF export authorization", move || {
            prepare_storage
                .pdf_export_command_resource(&prepare_note_id, &prepare_attachment_id)
                .map_err(ExportError::from)
        })
        .await
        {
            Ok(resource) => resource,
            Err(error) => {
                let _ = crate::native::staging::discard_staged_pdf_export(&export_token);
                return Err(error);
            }
        };

        // The command resource retains derivative authorization and, for an
        // unlocked protected note, its exact Vault read permit across the full
        // chooser/native handoff lifetime. Granular persistence therefore does
        // not need a separate protected-note export prohibition.
        if !resource.is_publishable() {
            let _ = crate::native::staging::discard_staged_pdf_export(&export_token);
            return Err(ExportError::from(crate::storage::StorageError::Validation(
                "annotated PDF export authorization expired".to_string(),
            )));
        }

        let attachment = resource.attachment();
        let stem = Path::new(&attachment.file_name)
            .file_stem()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("document");
        let file_name = format!("{stem} annotated.pdf");
        let selected = AsyncFileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(&file_name)
            .save_file()
            .await;
        if !resource.is_publishable() {
            let _ = crate::native::staging::discard_staged_pdf_export(&export_token);
            return Err(ExportError::from(crate::storage::StorageError::Validation(
                "annotated PDF export authorization expired while the file dialog was open"
                    .to_string(),
            )));
        }
        let Some(destination) = selected.map(|handle| handle.path().to_path_buf()) else {
            let _ = crate::native::staging::discard_staged_pdf_export(&export_token);
            return Ok(PdfExportOutcome::Cancelled);
        };

        let token = export_token.clone();
        let attachment_id_for_save = resource.attachment().id.clone();
        let result = ApplicationExecutor::read("PDF export save", move || {
            if !resource.is_publishable() {
                return Err(ExportError::from(crate::storage::StorageError::Validation(
                    "annotated PDF export authorization expired before native handoff".to_string(),
                )));
            }
            crate::native::staging::save_staged_pdf_export(
                &token,
                &attachment_id_for_save,
                &destination,
            )
            .map_err(ExportError::from)
        })
        .await;
        if let Err(error) = result {
            let _ = crate::native::staging::discard_staged_pdf_export(&export_token);
            return Err(error);
        }
        Ok(PdfExportOutcome::Saved)
    }
}

pub(crate) fn suggested_note_file_name(title: &str, extension: &str) -> String {
    let stem = sanitize_file_stem(title);
    format!("{stem}.{extension}")
}

fn sanitize_file_stem(title: &str) -> String {
    let mut value = title
        .trim()
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '\0' => '-',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();
    value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let value = value.trim_matches(|character| character == '.' || character == ' ');
    if value.is_empty() {
        return "Untitled note".to_string();
    }

    const MAX_STEM_BYTES: usize = 180;
    let mut end = 0;
    for (index, character) in value.char_indices() {
        let next = index + character.len_utf8();
        if next > MAX_STEM_BYTES {
            break;
        }
        end = next;
    }
    value[..end].to_string()
}

pub(crate) fn prepare_note_pdf_print_script(title: &str) -> String {
    let title = serde_json::to_string(if title.trim().is_empty() {
        "Untitled note"
    } else {
        title.trim()
    })
    .unwrap_or_else(|_| "\"Rustnote note\"".to_string());
    format!(
        r#"
const stateKey = '__rustnoteNotePdfPrintState';
const previousState = window[stateKey];
if (previousState && typeof previousState.cleanup === 'function') {{
    previousState.cleanup();
}}
const previousTitle = document.title;
document.title = {title};
let cleaned = false;
let resolveDone = null;
const done = new Promise((resolve) => {{ resolveDone = resolve; }});
const cleanupRustnotePrint = () => {{
    if (cleaned) return;
    cleaned = true;
    window.removeEventListener('afterprint', cleanupRustnotePrint);
    document.title = previousTitle;
    if (window[stateKey]?.cleanup === cleanupRustnotePrint) delete window[stateKey];
    if (resolveDone) resolveDone(true);
}};
window[stateKey] = {{ cleanup: cleanupRustnotePrint, done }};
window.addEventListener('afterprint', cleanupRustnotePrint, {{ once: true }});
return true;
"#
    )
}

pub(crate) fn wait_note_pdf_print_script() -> &'static str {
    r#"
const state = window.__rustnoteNotePdfPrintState;
if (!state || !state.done) return false;
return await state.done;
"#
}

pub(crate) fn cancel_note_pdf_print_script() -> &'static str {
    r#"
const state = window.__rustnoteNotePdfPrintState;
if (state && typeof state.cleanup === 'function') state.cleanup();
return true;
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_file_name_is_safe_and_stable() {
        assert_eq!(suggested_note_file_name("  A/B: C  ", "txt"), "A-B- C.txt");
        assert_eq!(suggested_note_file_name("...", "txt"), "Untitled note.txt");
        let unicode = suggested_note_file_name(&"🙂".repeat(100), "txt");
        assert!(unicode.len() <= 184);
        assert!(unicode.ends_with(".txt"));
    }

    #[test]
    fn print_script_json_escapes_title() {
        let script = prepare_note_pdf_print_script("A \"quoted\" note");
        assert!(script.contains("A \\\"quoted\\\" note"));
        assert!(script.contains("__rustnoteNotePdfPrintState"));
        assert!(script.contains("afterprint"));
        assert!(script.contains("done"));
        assert!(!script.contains("300000"));
        assert!(!script.contains("window.print()"));
        assert!(wait_note_pdf_print_script().contains("await state.done"));
    }
}
