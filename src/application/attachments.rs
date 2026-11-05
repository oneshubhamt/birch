use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use rfd::AsyncFileDialog;

use crate::application::errors::AttachmentError;
use crate::application::executor::ApplicationExecutor;
use crate::attachment_media::{self, AttachmentMediaKind};
use crate::autosave::AutosaveController;
use crate::domain::{Attachment, Note, NoteId};
use crate::storage::{AttachmentStore, AuthorizedDerivedImageAttachment, NoteSaveScope};

const MAX_TRANSIENT_IMAGE_IMPORT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachmentRuntimeMediaCandidate {
    pub(crate) profile: String,
    pub(crate) mime_type: String,
    pub(crate) preview_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachmentRuntimeDescriptorData {
    pub(crate) kind: String,
    pub(crate) mime_type: Option<String>,
    pub(crate) compatible_playback_candidates: Vec<AttachmentRuntimeMediaCandidate>,
    pub(crate) available: bool,
    pub(crate) preview_url: Option<String>,
    pub(crate) compatible_preview_url: Option<String>,
    pub(crate) screen_preview_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachmentImportRecoveryGroup {
    pub(crate) request_id: String,
    pub(crate) attachment_ids: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct PreparedAttachmentImport {
    pub(crate) ingest_id: String,
    pub(crate) attachment_id: String,
    pub(crate) file_name: String,
    pub(crate) mime_type: Option<String>,
    storage: crate::storage::PreparedAttachmentImport,
}

impl PreparedAttachmentImport {
    fn from_storage(storage: crate::storage::PreparedAttachmentImport) -> Self {
        Self {
            ingest_id: storage.ingest_id.clone(),
            attachment_id: storage.attachment_id.clone(),
            file_name: storage.file_name.clone(),
            mime_type: storage.mime_type.clone(),
            storage,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct AttachmentService {
    storage: AttachmentStore,
    autosave: AutosaveController,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttachmentPickerCategory {
    Image,
    Audio,
    Video,
    Pdf,
}

impl AttachmentPickerCategory {
    fn media_kind(self) -> AttachmentMediaKind {
        match self {
            Self::Image => AttachmentMediaKind::Image,
            Self::Audio => AttachmentMediaKind::Audio,
            Self::Video => AttachmentMediaKind::Video,
            Self::Pdf => AttachmentMediaKind::Pdf,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttachmentFileCommand {
    Open,
    SaveAs,
    Reveal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttachmentFileCommandOutcome {
    Completed,
    Cancelled,
}

impl AttachmentFileCommand {
    pub(crate) const fn worker_label(self) -> &'static str {
        match self {
            Self::Open => "attachment open",
            Self::SaveAs => "attachment save",
            Self::Reveal => "attachment reveal",
        }
    }

    pub(crate) const fn success_message(self) -> &'static str {
        match self {
            Self::Open => "Attachment opened",
            Self::SaveAs => "Attachment saved",
            Self::Reveal => "Attachment revealed in Finder",
        }
    }

    pub(crate) const fn failure_verb(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::SaveAs => "save",
            Self::Reveal => "reveal",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ImageEditRequest {
    pub(crate) request_id: String,
    pub(crate) attachment_id: String,
    pub(crate) source_attachment_id: String,
    pub(crate) stage_token: String,
    pub(crate) file_name: String,
    pub(crate) mime_type: String,
    pub(crate) edit_recipe: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ImageImportRequest {
    pub(crate) stage_token: String,
    pub(crate) file_name: String,
    pub(crate) mime_type: String,
}

#[derive(Debug)]
pub(crate) struct ImageEditResult {
    pub(crate) added: Attachment,
    pub(crate) attachments: Vec<Attachment>,
}

#[derive(Debug)]
pub(crate) struct ImageImportResult {
    pub(crate) added: Vec<Attachment>,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) failure: Option<String>,
}

#[derive(Debug)]
pub(crate) struct PreparedAttachmentBatchResult {
    pub(crate) prepared: Vec<PreparedAttachmentImport>,
    pub(crate) failure: Option<String>,
}

#[derive(Debug)]
pub(crate) struct AttachmentBatchResult {
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) request_attachment_ids: Vec<String>,
    pub(crate) failure: Option<String>,
}

#[derive(Debug)]
pub(crate) struct AttachmentRecoveryResult {
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) unresolved: usize,
}

impl AttachmentService {
    pub(crate) fn new(storage: AttachmentStore, autosave: AutosaveController) -> Self {
        Self { storage, autosave }
    }

    /// Resolve the editor image-import staging endpoint without exposing the
    /// native media server implementation to UI code.
    pub(crate) fn image_import_stage_url(&self) -> Option<String> {
        crate::media_server::image_import_stage_url()
    }

    pub(crate) fn list(&self, note_id: &NoteId) -> Result<Vec<Attachment>, AttachmentError> {
        self.storage
            .list_attachments(note_id)
            .map_err(AttachmentError::from)
    }

    /// Resolve the runtime-facing attachment facts without exposing storage/media
    /// implementation modules to the UI adapter.
    pub(crate) fn runtime_descriptor(
        &self,
        attachment: &Attachment,
    ) -> AttachmentRuntimeDescriptorData {
        let file_resource = self.storage.attachment_file_resource(attachment).ok();
        let available = file_resource
            .as_ref()
            .is_some_and(|resource| resource.path().is_file());
        let media_kind = attachment_media::kind(
            &attachment.file_name,
            attachment.mime_type.as_deref(),
        );
        let compatible_playback_candidates = if available
            && matches!(media_kind, AttachmentMediaKind::Audio | AttachmentMediaKind::Video)
            && crate::media_compat::is_compatibility_candidate(
                &attachment.file_name,
                attachment.mime_type.as_deref(),
            )
        {
            file_resource
                .as_ref()
                .and_then(|resource| crate::media_compat::probe_path(resource.path()).ok())
                .map(|probe| {
                    probe
                        .candidates
                        .into_iter()
                        .filter_map(|candidate| {
                            crate::media_server::compatible_preview_url(
                                &attachment.id,
                                candidate.profile,
                            )
                            .map(|preview_url| AttachmentRuntimeMediaCandidate {
                                profile: candidate.profile.as_str().to_string(),
                                mime_type: candidate.mime_type,
                                preview_url,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let preview_url = available.then(|| {
            if matches!(media_kind, AttachmentMediaKind::Audio | AttachmentMediaKind::Video)
                && let Some(url) = crate::media_server::preview_url(
                    &attachment.id,
                    &attachment.file_name,
                    attachment.mime_type.as_deref(),
                )
            {
                return url;
            }
            let suffix = attachment_media::preview_extension(
                &attachment.file_name,
                attachment.mime_type.as_deref(),
            )
            .map(|extension| format!(".{extension}"))
            .unwrap_or_default();
            format!(
                "/rustnoteattachment/{}/preview{}",
                attachment.id.as_str(), suffix
            )
        });
        let compatible_preview_url =
            (available && media_kind == AttachmentMediaKind::Image).then(|| {
                format!("/rustnoteattachment/{}/compatible", attachment.id.as_str())
            });
        let screen_preview_url = (available
            && media_kind == AttachmentMediaKind::Image
            && crate::image_rendition::needs_screen_rendition(
                attachment.image_pixel_width,
                attachment.image_pixel_height,
                attachment.image_format.as_deref(),
                attachment.image_animated,
            ))
        .then(|| format!("/rustnoteattachment/{}/screen", attachment.id.as_str()));

        AttachmentRuntimeDescriptorData {
            kind: media_kind.as_str().to_string(),
            mime_type: attachment_media::resolved_mime_type(
                &attachment.file_name,
                attachment.mime_type.as_deref(),
            ),
            compatible_playback_candidates,
            available,
            preview_url,
            compatible_preview_url,
            screen_preview_url,
        }
    }

    pub(crate) fn runtime_kind_for_path(path: &Path) -> String {
        attachment_media::kind_from_path(path).as_str().to_string()
    }

    pub(crate) fn runtime_kind(file_name: &str, mime_type: Option<&str>) -> String {
        attachment_media::kind(file_name, mime_type).as_str().to_string()
    }

    pub(crate) fn display_kind(attachment: &Attachment) -> &'static str {
        attachment_media::kind(&attachment.file_name, attachment.mime_type.as_deref()).label()
    }

    pub(crate) fn picker_extensions(category: AttachmentPickerCategory) -> Vec<&'static str> {
        attachment_media::extensions_for_kind(category.media_kind())
    }

    pub(crate) fn path_matches_picker_category(
        path: &Path,
        category: AttachmentPickerCategory,
    ) -> bool {
        attachment_media::kind_from_path(path) == category.media_kind()
    }

    pub(crate) fn import_recovery_groups(
        &self,
        note_id: &NoteId,
    ) -> Result<Vec<AttachmentImportRecoveryGroup>, AttachmentError> {
        let intents = self
            .storage
            .attachment_ingest_intents(note_id)
            .map_err(AttachmentError::from)?;
        let mut groups = Vec::<AttachmentImportRecoveryGroup>::new();
        for intent in intents {
            if let Some(group) = groups
                .iter_mut()
                .find(|group| group.request_id == intent.request_id)
            {
                group.attachment_ids.push(intent.attachment_id);
            } else {
                groups.push(AttachmentImportRecoveryGroup {
                    request_id: intent.request_id,
                    attachment_ids: vec![intent.attachment_id],
                });
            }
        }
        Ok(groups)
    }

    pub(crate) fn save_insertion_anchor(
        &self,
        note: Note,
    ) -> Result<(), AttachmentError> {
        self.autosave
            .flush_live_note_scoped(note, NoteSaveScope::DOCUMENT)
            .map(|_| ())
            .map_err(|error| AttachmentError::Autosave(error.context("could not save the attachment insertion point")))
    }

    pub(crate) fn prepare_import_batch(
        &self,
        current: Note,
        request_id: String,
        paths: Vec<PathBuf>,
    ) -> Result<PreparedAttachmentBatchResult, AttachmentError> {
        let note_id = current.id.clone();
        self.autosave
            .flush_note_scoped(current, NoteSaveScope::NONE)
            .map_err(|error| AttachmentError::Autosave(error.context("Could not finish saving")))?;

        let mut prepared = Vec::new();
        let mut failure = None;
        for path in paths {
            match self.storage.prepare_attachment_import_for_request_profiled(
                &note_id,
                Some(&request_id),
                &path,
            ) {
                Ok(result) => {
                    log_attachment_import_profile("stage", &result.profile);
                    prepared.push(PreparedAttachmentImport::from_storage(result));
                }
                Err(error) => {
                    failure = Some(format!("Could not stage {}: {error}", path.display()));
                    break;
                }
            }
        }
        Ok(PreparedAttachmentBatchResult { prepared, failure })
    }

    pub(crate) fn finalize_import_batch(
        &self,
        prepared: Vec<PreparedAttachmentImport>,
    ) -> Result<AttachmentBatchResult, AttachmentError> {
        let note_id = prepared
            .first()
            .map(|item| item.storage.note_id.clone());
        let request_attachment_ids = prepared
            .iter()
            .map(|item| item.attachment_id.clone())
            .collect::<Vec<_>>();
        let mut failure = None;
        for item in prepared {
            match self
                .storage
                .finalize_prepared_attachment_import_profiled(item.storage)
            {
                Ok(result) => {
                    log_attachment_import_profile("final", &result.profile);
                }
                Err(error) => {
                    if failure.is_none() {
                        failure = Some(format!("Could not finalize attachment: {error}"));
                    }
                }
            }
        }
        let attachments = match note_id {
            Some(note_id) => self.storage.list_attachments(&note_id).map_err(|error| AttachmentError::Unavailable(format!(
                "Attachment finalization completed, but the list could not refresh: {error}"
            )))?,
            None => Vec::new(),
        };
        Ok(AttachmentBatchResult {
            attachments,
            request_attachment_ids,
            failure,
        })
    }


    pub(crate) fn recover_pending(&self, note_id: &NoteId) -> Result<AttachmentRecoveryResult, AttachmentError> {
        let (_replayed, unresolved) = self
            .storage
            .reconcile_pending_attachment_ingests_for_note(note_id)
            .map_err(AttachmentError::from)?;
        let attachments = self
            .storage
            .list_attachments(note_id)
            .map_err(|error| AttachmentError::Unavailable(format!("attachment recovery completed, but the list could not refresh: {error}")))?;
        Ok(AttachmentRecoveryResult { attachments, unresolved })
    }

    pub(crate) fn open_pending(&self, ingest_id: &str) -> Result<(), AttachmentError> {
        let initial = self
            .storage
            .pending_attachment_ingest_resource(ingest_id)
            .map_err(AttachmentError::from)?
            .ok_or_else(|| AttachmentError::Unavailable("pending attachment is no longer available".to_string()))?;
        // A pending ingest row blocks Encrypt, but finalization may remove that
        // row while this desktop handoff is still copying. Join the same
        // note-scoped derivative boundary and re-read the pending resource after
        // any transition wait so a stale pre-Encrypt path cannot publish a new
        // plaintext handoff after the Vault scrub.
        let plaintext_authorization = self
            .storage
            .plaintext_attachment_ingress_permit(&initial.note_id)
            .map_err(AttachmentError::from)?;
        let resource = self
            .storage
            .pending_attachment_ingest_resource_with_authorization(
                ingest_id,
                &plaintext_authorization,
            )
            .map_err(AttachmentError::from)?
            .filter(|resource| resource.note_id == initial.note_id)
            .ok_or_else(|| AttachmentError::Unavailable("pending attachment is no longer available".to_string()))?;
        let handoff_root = self.storage.attachment_handoff_directory();
        crate::native::handoff::open_managed_file_for_note(
            &handoff_root,
            &resource.path,
            &resource.file_name,
            resource.mime_type.as_deref(),
            resource.note_id.as_str(),
        )
        .map_err(AttachmentError::Native)
    }

    pub(crate) fn probe_media_compatibility(
        &self,
        attachment_id: &str,
        profile: &str,
    ) -> Result<String, AttachmentError> {
        let profile = crate::media_compat::parse_profile(profile)
            .ok_or_else(|| AttachmentError::InvalidRequest("unsupported media compatibility profile".to_string()))?;
        let transport = self.storage.transport_store();
        let cache_directory = self.storage.media_compat_directory();
        crate::media_compat::probe_attachment(
            &transport,
            &cache_directory,
            attachment_id,
            profile,
        )
        .map(|status| status.as_str().to_string())
        .map_err(AttachmentError::MediaCompatibility)
    }

    pub(crate) fn prepare_media_compatibility(
        &self,
        attachment_id: &str,
        profile: &str,
    ) -> Result<(), AttachmentError> {
        let profile = crate::media_compat::parse_profile(profile)
            .ok_or_else(|| AttachmentError::InvalidRequest("unsupported media compatibility profile".to_string()))?;
        let transport = self.storage.transport_store();
        let cache_directory = self.storage.media_compat_directory();
        crate::media_compat::prepare_attachment(
            &transport,
            &cache_directory,
            attachment_id,
            profile,
        )
        .map(|_| ())
        .map_err(AttachmentError::MediaCompatibility)
    }

    pub(crate) fn invalidate_media_compatibility(
        &self,
        attachment_id: &str,
        profile: &str,
    ) -> Result<(), AttachmentError> {
        let profile = crate::media_compat::parse_profile(profile)
            .ok_or_else(|| AttachmentError::InvalidRequest("unsupported media compatibility profile".to_string()))?;
        let transport = self.storage.transport_store();
        let cache_directory = self.storage.media_compat_directory();
        crate::media_compat::invalidate_attachment(
            &transport,
            &cache_directory,
            attachment_id,
            profile,
        )
        .map(|_| ())
        .map_err(AttachmentError::MediaCompatibility)
    }

    pub(crate) async fn execute_file_command_by_id(
        &self,
        note_id: NoteId,
        attachment_id: String,
        command: AttachmentFileCommand,
    ) -> Result<AttachmentFileCommandOutcome, AttachmentError> {
        let service = self.clone();
        let resource = ApplicationExecutor::read("attachment command prepare", move || {
            service
                .storage
                .attachment_file_command_resource(&note_id, &attachment_id)
                .map_err(AttachmentError::from)
        })
        .await?;
        self.execute_prepared_file_command(resource, command).await
    }

    pub(crate) async fn execute_file_command(
        &self,
        attachment: Attachment,
        command: AttachmentFileCommand,
    ) -> Result<AttachmentFileCommandOutcome, AttachmentError> {
        // Do not trust/carry caller-owned protected metadata across an async command.
        // Re-resolve it under the command lifetime permit so the filename, path and
        // Vault session all belong to one authorization window.
        self.execute_file_command_by_id(attachment.note_id, attachment.id, command)
            .await
    }

    async fn execute_prepared_file_command(
        &self,
        resource: crate::storage::AttachmentFileCommandResource,
        command: AttachmentFileCommand,
    ) -> Result<AttachmentFileCommandOutcome, AttachmentError> {
        let destination = if command == AttachmentFileCommand::SaveAs {
            let selected = AsyncFileDialog::new()
                .set_file_name(&resource.attachment().file_name)
                .save_file()
                .await;
            if !resource.is_publishable() {
                return Err(AttachmentError::Unavailable(
                    "attachment authorization expired while the file dialog was open".to_string(),
                ));
            }
            let Some(selected) = selected else {
                // Dropping `resource` releases both the derivative permit and, for
                // protected attachments, the Vault permit only after the dialog is
                // closed/cancelled.
                return Ok(AttachmentFileCommandOutcome::Cancelled);
            };
            Some(selected.path().to_path_buf())
        } else {
            None
        };

        ApplicationExecutor::read(command.worker_label(), move || {
            Self::execute_file_command_blocking(&resource, command, destination)
        })
        .await?;
        Ok(AttachmentFileCommandOutcome::Completed)
    }

    fn execute_file_command_blocking(
        resource: &crate::storage::AttachmentFileCommandResource,
        command: AttachmentFileCommand,
        destination: Option<PathBuf>,
    ) -> Result<(), AttachmentError> {
        if !resource.is_publishable() {
            return Err(AttachmentError::Unavailable(
                "attachment authorization expired before native handoff".to_string(),
            ));
        }
        let attachment = resource.attachment();
        let path = resource.path();
        match command {
            AttachmentFileCommand::Open => crate::native::handoff::open_managed_file(
                resource.handoff_directory(),
                path,
                &attachment.file_name,
                attachment.mime_type.as_deref(),
            )
            .map_err(|error| {
                // Do not carry protected filenames into an error that can be
                // published after the command-lifetime Vault permit is released.
                AttachmentError::Native(error.context("Could not open attachment"))
            }),
            AttachmentFileCommand::SaveAs => {
                let destination = destination.as_deref().ok_or_else(|| {
                    AttachmentError::InvalidRequest("save destination is missing".to_string())
                })?;
                crate::native::handoff::save_managed_file(path, destination).map_err(|error| {
                    AttachmentError::Native(error.context("Could not save attachment"))
                })
            }
            AttachmentFileCommand::Reveal => crate::native::handoff::reveal_managed_file(
                resource.handoff_directory(),
                path,
                &attachment.file_name,
                attachment.mime_type.as_deref(),
            )
            .map_err(|error| {
                AttachmentError::Native(error.context("Could not reveal attachment"))
            }),
        }
    }

    pub(crate) fn commit_image_edit(
        &self,
        note_id: NoteId,
        payload: ImageEditRequest,
    ) -> Result<ImageEditResult, AttachmentError> {
        if payload.request_id.trim().is_empty() {
            return Err(AttachmentError::InvalidRequest(
                "image edit request is missing its identifier".to_string(),
            ));
        }
        if payload.stage_token.trim().is_empty() || payload.stage_token.len() > 128 {
            return Err(AttachmentError::InvalidRequest("image edit staging token is invalid".to_string()));
        }
        if payload.edit_recipe.len() > 524_288 {
            return Err(AttachmentError::InvalidRequest("image edit recipe is too large".to_string()));
        }
        if attachment_media::kind(&payload.file_name, Some(&payload.mime_type))
            != AttachmentMediaKind::Image
        {
            return Err(AttachmentError::InvalidRequest("edited rendition is not a supported image".to_string()));
        }
        if !matches!(payload.mime_type.as_str(), "image/png" | "image/jpeg") {
            return Err(AttachmentError::InvalidRequest("edited rendition must be PNG or JPEG".to_string()));
        }
        let plaintext_authorization = self
            .storage
            .plaintext_attachment_ingress_permit(&note_id)
            .map_err(AttachmentError::from)?;
        let current = self
            .storage
            .list_attachments_with_authorization(&note_id, &plaintext_authorization)
            .map_err(AttachmentError::from)?
            .into_iter()
            .find(|attachment| attachment.id == payload.attachment_id)
            .ok_or_else(|| AttachmentError::Unavailable("the image being edited no longer exists".to_string()))?;
        if attachment_media::kind(&current.file_name, current.mime_type.as_deref())
            != AttachmentMediaKind::Image
        {
            return Err(AttachmentError::Unavailable("the selected attachment is no longer an image".to_string()));
        }
        let expected_source = current
            .source_attachment_id
            .as_deref()
            .unwrap_or(current.id.as_str());
        if payload.source_attachment_id != expected_source {
            return Err(AttachmentError::Unavailable("image edit source changed while the editor was open".to_string()));
        }

        let staged_path = crate::native::staging::claim_staged_image_edit(&payload.stage_token, &payload.mime_type, &payload.attachment_id)
            .map_err(AttachmentError::Native)?;
        let add_result = self.storage.add_derived_image_attachment_file_authorized(
            AuthorizedDerivedImageAttachment::new(
                &note_id,
                expected_source,
                &payload.file_name,
                Some(&payload.mime_type),
                &staged_path,
                &payload.edit_recipe,
                &plaintext_authorization,
            ),
        );
        let cleanup_result = crate::native::staging::discard_staged_image_edit(&payload.stage_token);
        let added = add_result.map_err(AttachmentError::from)?;
        cleanup_result.map_err(AttachmentError::Native)?;
        let attachments = self
            .storage
            .list_attachments_with_authorization(&note_id, &plaintext_authorization)
            .map_err(|error| {
            AttachmentError::Unavailable(format!(
                "Edited image was stored, but attachments could not refresh: {error}"
            ))
        })?;
        Ok(ImageEditResult { added, attachments })
    }

    pub(crate) fn import_images(
        &self,
        note_id: NoteId,
        images: Vec<ImageImportRequest>,
    ) -> Result<ImageImportResult, AttachmentError> {
        if images.is_empty() {
            return Err(AttachmentError::InvalidRequest("no images were supplied".to_string()));
        }
        // Hold one note-scoped transition permit across the whole batch. Encrypt
        // either waits until every staged image is committed/cleaned, or this
        // call waits for Encrypt and then fails the post-transition authorization
        // before claiming any plaintext staging.
        let plaintext_authorization = self
            .storage
            .plaintext_attachment_ingress_permit(&note_id)
            .map_err(AttachmentError::from)?;
        let mut added = Vec::new();
        let mut failure = None;
        let mut total_bytes = 0usize;
        let mut images: VecDeque<_> = images.into();

        while let Some(image) = images.pop_front() {
            if attachment_media::kind(&image.file_name, Some(&image.mime_type))
                != AttachmentMediaKind::Image
            {
                failure = Some(Self::fail_image_import(
                    format!("{} is not an image", image.file_name),
                    &image,
                    &mut images,
                ));
                break;
            }
            if image.stage_token.trim().is_empty() || image.stage_token.len() > 128 {
                failure = Some(Self::fail_image_import(
                    format!("{} has an invalid staging token", image.file_name),
                    &image,
                    &mut images,
                ));
                break;
            }

            let staged_path = match crate::native::staging::claim_staged_image_import(
                &image.stage_token,
                note_id.as_str(),
            ) {
                Ok(path) => path,
                Err(error) => {
                    failure = Some(Self::fail_image_import(
                        format!("Could not claim {} for import: {error}", image.file_name),
                        &image,
                        &mut images,
                    ));
                    break;
                }
            };
            let staged_len = match std::fs::metadata(&staged_path) {
                Ok(metadata) => metadata.len() as usize,
                Err(error) => {
                    let cleanup_error = crate::native::staging::discard_staged_image_import(&image.stage_token).err();
                    Self::discard_image_import_stages(images.drain(..));
                    failure = Some(match cleanup_error {
                        Some(cleanup_error) => format!(
                            "Could not inspect {}: {error}; staged plaintext cleanup also failed: {cleanup_error}",
                            image.file_name
                        ),
                        None => format!("Could not inspect {}: {error}", image.file_name),
                    });
                    break;
                }
            };
            if total_bytes.saturating_add(staged_len) > MAX_TRANSIENT_IMAGE_IMPORT_BYTES {
                let cleanup_error = crate::native::staging::discard_staged_image_import(&image.stage_token).err();
                Self::discard_image_import_stages(images.drain(..));
                failure = Some(match cleanup_error {
                    Some(cleanup_error) => format!(
                        "Images exceed the 64 MB browser import limit; staged plaintext cleanup also failed: {cleanup_error}"
                    ),
                    None => "Images exceed the 64 MB browser import limit".to_string(),
                });
                break;
            }
            total_bytes += staged_len;

            let add_result = self.storage.add_attachment_file_as_authorized(
                &note_id,
                &image.file_name,
                Some(&image.mime_type),
                &staged_path,
                &plaintext_authorization,
            );
            let cleanup_result = crate::native::staging::discard_staged_image_import(&image.stage_token);
            match (add_result, cleanup_result) {
                (Ok(attachment), Ok(())) => added.push(attachment),
                (Ok(attachment), Err(cleanup_error)) => {
                    added.push(attachment);
                    Self::discard_image_import_stages(images.drain(..));
                    failure = Some(format!(
                        "{} was imported, but staged plaintext cleanup failed: {cleanup_error}",
                        image.file_name
                    ));
                    break;
                }
                (Err(error), cleanup_result) => {
                    Self::discard_image_import_stages(images.drain(..));
                    failure = Some(match cleanup_result {
                        Ok(()) => format!("Could not import {}: {error}", image.file_name),
                        Err(cleanup_error) => format!(
                            "Could not import {}: {error}; staged plaintext cleanup also failed: {cleanup_error}",
                            image.file_name
                        ),
                    });
                    break;
                }
            }
        }

        let attachments = self
            .storage
            .list_attachments_with_authorization(&note_id, &plaintext_authorization)
            .map_err(|error| {
            AttachmentError::Unavailable(format!(
                "Images were imported, but attachments could not refresh: {error}"
            ))
        })?;
        Ok(ImageImportResult {
            added,
            attachments,
            failure,
        })
    }

    pub(crate) fn discard_image_edit_stage(stage_token: &str) {
        let _ = crate::native::staging::discard_staged_image_edit(stage_token);
    }

    pub(crate) fn discard_image_import_stage(stage_token: &str) {
        let _ = crate::native::staging::discard_staged_image_import(stage_token);
    }

    pub(crate) fn discard_image_import_stages(
        images: impl IntoIterator<Item = ImageImportRequest>,
    ) {
        for image in images {
            Self::discard_image_import_stage(&image.stage_token);
        }
    }

    fn fail_image_import(
        message: String,
        current: &ImageImportRequest,
        remaining: &mut VecDeque<ImageImportRequest>,
    ) -> String {
        let _ = crate::native::staging::discard_staged_image_import(&current.stage_token);
        Self::discard_image_import_stages(remaining.drain(..));
        message
    }
}

fn log_attachment_import_profile(
    phase: &str,
    profile: &crate::storage::AttachmentImportProfile,
) {
    fn milliseconds(duration: std::time::Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }

    const LARGE_IMPORT_PROFILE_BYTES: u64 = 256 * 1024 * 1024;
    if cfg!(debug_assertions) || profile.byte_size >= LARGE_IMPORT_PROFILE_BYTES {
        eprintln!(
            "[rustnote][attachment-import] phase={} {:.1} MiB total={:.1}ms critical-stage={:.1}ms stage-mode={} sync={:.1}ms hash-background={:.1}ms probe={:.1}ms lock={:.1}ms prepare-db={:.1}ms materialize={:.1}ms record-db={:.1}ms finalize={:.1}ms",
            phase,
            profile.byte_size as f64 / (1024.0 * 1024.0),
            milliseconds(profile.total),
            milliseconds(profile.managed_stage),
            profile.staging_mode.as_str(),
            milliseconds(profile.staging_sync),
            milliseconds(profile.hash_finalize),
            milliseconds(profile.image_probe),
            milliseconds(profile.operation_lock_wait),
            milliseconds(profile.prepare_database),
            milliseconds(profile.materialize),
            milliseconds(profile.record_database),
            milliseconds(profile.finalize_total),
        );
    }
}
