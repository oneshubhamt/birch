use std::fmt;

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::platform::{
    BackupProtectorKind, BackupWrapAlgorithm, DeviceBackupKeyProtector, DeviceBackupRootCreation,
    DeviceBackupRootStatus, SecureKeyStoreError, system_device_backup_key_protector,
};
use rustnote_vault::{
    BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES, BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES,
    BACKUP_REPOSITORY_KEYSET_V1_BYTES,
};
#[cfg(test)]
use rustnote_vault::{BackupDeviceRootKey, BackupKeyEnvelopeService};

pub(crate) const BACKUP_REPOSITORY_FORMAT_VERSION: u16 = 3;
pub(crate) const BACKUP_REPOSITORY_HEADER_VERSION: u16 = 1;
pub(crate) const BACKUP_KEYSET_VERSION: u16 = 1;
pub(crate) const BACKUP_PROTECTOR_SLOT_V1: u16 = 1;
pub(crate) const BACKUP_REPOSITORY_HEADER_PREFIX_BYTES: usize = 92;
pub(crate) const BACKUP_MAX_KEY_ENVELOPE_BYTES: usize = 4096;

const BACKUP_REPOSITORY_HEADER_MAGIC: &[u8; 8] = b"RNBHDR01";
const BACKUP_REPOSITORY_KEYSET_MAGIC: &[u8; 8] = b"RNBKEY01";
const BACKUP_LIBRARY_BINDING_DOMAIN: &[u8] = b"rustnote-backup-library-binding-v1";
const BACKUP_KEY_ENVELOPE_AAD_DOMAIN: &[u8] = b"rustnote-backup-key-envelope-aad-v1";
const REPOSITORY_ID_BYTES: usize = 32;
const LIBRARY_BINDING_BYTES: usize = 32;
const REPOSITORY_KEY_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackupRepositoryId([u8; REPOSITORY_ID_BYTES]);

impl BackupRepositoryId {
    pub(crate) fn generate() -> Result<Self, BackupKeyAuthorityError> {
        let mut bytes = [0u8; REPOSITORY_ID_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| BackupKeyAuthorityError::RandomSource)?;
        Ok(Self(bytes))
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; REPOSITORY_ID_BYTES] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackupLibraryBinding([u8; LIBRARY_BINDING_BYTES]);

impl BackupLibraryBinding {
    pub(crate) fn from_library_id(library_id: &str) -> Result<Self, BackupKeyAuthorityError> {
        let library_id = library_id.as_bytes();
        let length = u32::try_from(library_id.len())
            .map_err(|_| BackupKeyAuthorityError::LibraryIdTooLong)?;
        let mut hash = Sha256::new();
        hash.update(BACKUP_LIBRARY_BINDING_DOMAIN);
        hash.update(length.to_be_bytes());
        hash.update(library_id);
        Ok(Self(hash.finalize().into()))
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; LIBRARY_BINDING_BYTES] {
        &self.0
    }
}

/// Purpose-separated repository keys. Every secret field is zeroized on drop
/// and the type deliberately does not implement `Clone` or expose `Debug` data.
pub(crate) struct BackupRepositoryKeysetV1 {
    repository_id: BackupRepositoryId,
    library_binding: BackupLibraryBinding,
    snapshot_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>,
    blob_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>,
    metadata_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>,
    object_id_hmac_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>,
}

impl BackupRepositoryKeysetV1 {
    fn generate(
        repository_id: BackupRepositoryId,
        library_binding: BackupLibraryBinding,
    ) -> Result<Self, BackupKeyAuthorityError> {
        Ok(Self {
            repository_id,
            library_binding,
            snapshot_aead_key: random_repository_key()?,
            blob_aead_key: random_repository_key()?,
            metadata_aead_key: random_repository_key()?,
            object_id_hmac_key: random_repository_key()?,
        })
    }

    fn encode(&self) -> Zeroizing<[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES]> {
        let mut encoded = Zeroizing::new([0u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES]);
        encoded[..8].copy_from_slice(BACKUP_REPOSITORY_KEYSET_MAGIC);
        encoded[8..10].copy_from_slice(&BACKUP_KEYSET_VERSION.to_be_bytes());
        encoded[10..12].copy_from_slice(&0u16.to_be_bytes());
        encoded[12..44].copy_from_slice(self.repository_id.as_bytes());
        encoded[44..76].copy_from_slice(self.library_binding.as_bytes());
        encoded[76..108].copy_from_slice(&*self.snapshot_aead_key);
        encoded[108..140].copy_from_slice(&*self.blob_aead_key);
        encoded[140..172].copy_from_slice(&*self.metadata_aead_key);
        encoded[172..204].copy_from_slice(&*self.object_id_hmac_key);
        encoded
    }

    fn decode(
        encoded: &[u8],
        expected_repository_id: BackupRepositoryId,
        expected_library_binding: BackupLibraryBinding,
    ) -> Result<Self, BackupKeyAuthorityError> {
        if encoded.len() != BACKUP_REPOSITORY_KEYSET_V1_BYTES {
            return Err(BackupKeyAuthorityError::InvalidKeyset);
        }
        if &encoded[..8] != BACKUP_REPOSITORY_KEYSET_MAGIC {
            return Err(BackupKeyAuthorityError::InvalidKeyset);
        }
        if u16::from_be_bytes(encoded[8..10].try_into().expect("fixed keyset version slice"))
            != BACKUP_KEYSET_VERSION
            || encoded[10..12] != [0, 0]
        {
            return Err(BackupKeyAuthorityError::InvalidKeyset);
        }

        let repository_id = BackupRepositoryId(
            encoded[12..44]
                .try_into()
                .expect("fixed repository-id slice"),
        );
        let library_binding = BackupLibraryBinding(
            encoded[44..76]
                .try_into()
                .expect("fixed library-binding slice"),
        );
        if repository_id != expected_repository_id || library_binding != expected_library_binding {
            return Err(BackupKeyAuthorityError::ContextMismatch);
        }

        Ok(Self {
            repository_id,
            library_binding,
            snapshot_aead_key: Zeroizing::new(
                encoded[76..108]
                    .try_into()
                    .expect("fixed snapshot-key slice"),
            ),
            blob_aead_key: Zeroizing::new(
                encoded[108..140]
                    .try_into()
                    .expect("fixed blob-key slice"),
            ),
            metadata_aead_key: Zeroizing::new(
                encoded[140..172]
                    .try_into()
                    .expect("fixed metadata-key slice"),
            ),
            object_id_hmac_key: Zeroizing::new(
                encoded[172..204]
                    .try_into()
                    .expect("fixed object-id-key slice"),
            ),
        })
    }

    pub(crate) const fn repository_id(&self) -> BackupRepositoryId {
        self.repository_id
    }

    pub(crate) const fn library_binding(&self) -> BackupLibraryBinding {
        self.library_binding
    }

    pub(crate) fn snapshot_aead_key(&self) -> &[u8; REPOSITORY_KEY_BYTES] {
        &self.snapshot_aead_key
    }

    pub(crate) fn blob_aead_key(&self) -> &[u8; REPOSITORY_KEY_BYTES] {
        &self.blob_aead_key
    }

    pub(crate) fn metadata_aead_key(&self) -> &[u8; REPOSITORY_KEY_BYTES] {
        &self.metadata_aead_key
    }

    pub(crate) fn object_id_hmac_key(&self) -> &[u8; REPOSITORY_KEY_BYTES] {
        &self.object_id_hmac_key
    }
}

impl fmt::Debug for BackupRepositoryKeysetV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackupRepositoryKeysetV1")
            .field("repository_id", &self.repository_id)
            .field("library_binding", &self.library_binding)
            .field("keys", &"[REDACTED]")
            .finish()
    }
}

fn random_repository_key(
) -> Result<Zeroizing<[u8; REPOSITORY_KEY_BYTES]>, BackupKeyAuthorityError> {
    let mut key = Zeroizing::new([0u8; REPOSITORY_KEY_BYTES]);
    getrandom::fill(key.as_mut()).map_err(|_| BackupKeyAuthorityError::RandomSource)?;
    Ok(key)
}

#[derive(Debug)]
pub(crate) enum BackupKeyAuthorityError {
    UnsupportedPlatform,
    SecureStoreUnavailable(String),
    RootKeyMissing,
    RootKeyCorrupt,
    AuthenticationFailed,
    InvalidEnvelope,
    InvalidRepositoryHeader,
    InvalidKeyset,
    ContextMismatch,
    RandomSource,
    LibraryIdTooLong,
}

impl fmt::Display for BackupKeyAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str(
                "managed encrypted backups are unavailable on this platform",
            ),
            Self::SecureStoreUnavailable(message) => {
                write!(formatter, "OS secure key store is unavailable: {message}")
            }
            Self::RootKeyMissing => formatter.write_str("device backup root key is missing"),
            Self::RootKeyCorrupt => formatter.write_str("device backup root key is corrupt"),
            Self::AuthenticationFailed => {
                formatter.write_str("repository key envelope authentication failed")
            }
            Self::InvalidEnvelope => formatter.write_str("invalid repository key envelope"),
            Self::InvalidRepositoryHeader => {
                formatter.write_str("invalid backup repository header")
            }
            Self::InvalidKeyset => formatter.write_str("invalid backup repository keyset"),
            Self::ContextMismatch => formatter.write_str("backup repository key context mismatch"),
            Self::RandomSource => formatter.write_str("secure random source failed"),
            Self::LibraryIdTooLong => formatter.write_str("library identity is too long"),
        }
    }
}

impl std::error::Error for BackupKeyAuthorityError {}

impl From<SecureKeyStoreError> for BackupKeyAuthorityError {
    fn from(error: SecureKeyStoreError) -> Self {
        match error {
            SecureKeyStoreError::UnsupportedPlatform => Self::UnsupportedPlatform,
            SecureKeyStoreError::Unavailable(message) => Self::SecureStoreUnavailable(message),
            SecureKeyStoreError::MissingRootKey => Self::RootKeyMissing,
            SecureKeyStoreError::CorruptRootKey => Self::RootKeyCorrupt,
            SecureKeyStoreError::AuthenticationFailed => Self::AuthenticationFailed,
            SecureKeyStoreError::InvalidEnvelope => Self::InvalidEnvelope,
        }
    }
}

pub(crate) struct SealedBackupRepositoryKeysetV1 {
    header_prefix: [u8; BACKUP_REPOSITORY_HEADER_PREFIX_BYTES],
    envelope: Vec<u8>,
}

impl SealedBackupRepositoryKeysetV1 {
    pub(crate) fn repository_header_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.header_prefix.len() + self.envelope.len());
        bytes.extend_from_slice(&self.header_prefix);
        bytes.extend_from_slice(&self.envelope);
        bytes
    }

    pub(crate) fn header_prefix(&self) -> &[u8; BACKUP_REPOSITORY_HEADER_PREFIX_BYTES] {
        &self.header_prefix
    }

    pub(crate) fn envelope(&self) -> &[u8] {
        &self.envelope
    }
}

pub(crate) struct InitializedBackupRepositoryKeys {
    pub(crate) keyset: BackupRepositoryKeysetV1,
    pub(crate) sealed: SealedBackupRepositoryKeysetV1,
    pub(crate) root_creation: DeviceBackupRootCreation,
}

pub(crate) struct BackupKeyAuthority<'a> {
    protector: &'a dyn DeviceBackupKeyProtector,
}

impl BackupKeyAuthority<'static> {
    pub(crate) fn system() -> Self {
        Self {
            protector: system_device_backup_key_protector(),
        }
    }
}

impl<'a> BackupKeyAuthority<'a> {
    #[cfg(test)]
    fn with_protector(protector: &'a dyn DeviceBackupKeyProtector) -> Self {
        Self { protector }
    }

    pub(crate) fn root_status(
        &self,
    ) -> Result<DeviceBackupRootStatus, BackupKeyAuthorityError> {
        self.protector.root_status().map_err(Into::into)
    }

    /// Fresh-state-only initialization. This is intentionally separate from
    /// `open_repository_keyset`: opening an existing repository can never create
    /// or replace a missing device root key.
    pub(crate) fn initialize_fresh_repository(
        &self,
        library_id: &str,
    ) -> Result<InitializedBackupRepositoryKeys, BackupKeyAuthorityError> {
        let root_creation = self
            .protector
            .create_root_if_absent_for_fresh_state()
            .map_err(BackupKeyAuthorityError::from)?;
        let repository_id = BackupRepositoryId::generate()?;
        let library_binding = BackupLibraryBinding::from_library_id(library_id)?;
        let keyset = BackupRepositoryKeysetV1::generate(repository_id, library_binding)?;
        let header_prefix = repository_header_prefix(
            self.protector.protector_kind(),
            self.protector.wrap_algorithm(),
            repository_id,
            library_binding,
            self.protector.envelope_len(),
        )?;
        let aad = key_envelope_aad(&header_prefix);
        let encoded = keyset.encode();
        let envelope = self
            .protector
            .seal_repository_keyset(&aad, &encoded)
            .map_err(BackupKeyAuthorityError::from)?;
        if envelope.len() != self.protector.envelope_len() {
            return Err(BackupKeyAuthorityError::InvalidEnvelope);
        }
        Ok(InitializedBackupRepositoryKeys {
            keyset,
            sealed: SealedBackupRepositoryKeysetV1 {
                header_prefix,
                envelope,
            },
            root_creation,
        })
    }

    /// Open an existing repository header without ever creating a root key.
    /// Expected repository/library identity is supplied by the caller and must
    /// agree with both the plaintext header and authenticated keyset plaintext.
    pub(crate) fn open_repository_keyset(
        &self,
        expected_repository_id: BackupRepositoryId,
        expected_library_binding: BackupLibraryBinding,
        repository_header: &[u8],
    ) -> Result<BackupRepositoryKeysetV1, BackupKeyAuthorityError> {
        let parsed = parse_repository_header(repository_header)?;
        if parsed.repository_id != expected_repository_id
            || parsed.library_binding != expected_library_binding
        {
            return Err(BackupKeyAuthorityError::ContextMismatch);
        }
        if parsed.protector_kind != self.protector.protector_kind()
            || parsed.wrap_algorithm != self.protector.wrap_algorithm()
            || parsed.envelope.len() != self.protector.envelope_len()
        {
            return Err(BackupKeyAuthorityError::InvalidEnvelope);
        }
        let aad = key_envelope_aad(&parsed.header_prefix);
        let plaintext = self
            .protector
            .open_repository_keyset(&aad, parsed.envelope)
            .map_err(BackupKeyAuthorityError::from)?;
        BackupRepositoryKeysetV1::decode(
            plaintext.as_bytes(),
            expected_repository_id,
            expected_library_binding,
        )
    }
}

fn repository_header_prefix(
    protector_kind: BackupProtectorKind,
    wrap_algorithm: BackupWrapAlgorithm,
    repository_id: BackupRepositoryId,
    library_binding: BackupLibraryBinding,
    envelope_len: usize,
) -> Result<[u8; BACKUP_REPOSITORY_HEADER_PREFIX_BYTES], BackupKeyAuthorityError> {
    if envelope_len == 0 || envelope_len > BACKUP_MAX_KEY_ENVELOPE_BYTES {
        return Err(BackupKeyAuthorityError::InvalidEnvelope);
    }
    let envelope_len =
        u32::try_from(envelope_len).map_err(|_| BackupKeyAuthorityError::InvalidEnvelope)?;
    validate_protector_tuple(protector_kind, wrap_algorithm, envelope_len as usize)?;

    let mut header = [0u8; BACKUP_REPOSITORY_HEADER_PREFIX_BYTES];
    header[..8].copy_from_slice(BACKUP_REPOSITORY_HEADER_MAGIC);
    header[8..10].copy_from_slice(&BACKUP_REPOSITORY_FORMAT_VERSION.to_be_bytes());
    header[10..12].copy_from_slice(&BACKUP_REPOSITORY_HEADER_VERSION.to_be_bytes());
    header[12] = protector_kind as u8;
    header[13] = wrap_algorithm as u8;
    header[14..16].copy_from_slice(&0u16.to_be_bytes());
    header[16..48].copy_from_slice(repository_id.as_bytes());
    header[48..80].copy_from_slice(library_binding.as_bytes());
    header[80..82].copy_from_slice(&BACKUP_KEYSET_VERSION.to_be_bytes());
    header[82..84].copy_from_slice(&BACKUP_PROTECTOR_SLOT_V1.to_be_bytes());
    header[84..88].copy_from_slice(&envelope_len.to_be_bytes());
    header[88..92].copy_from_slice(&0u32.to_be_bytes());
    Ok(header)
}

fn key_envelope_aad(
    header_prefix: &[u8; BACKUP_REPOSITORY_HEADER_PREFIX_BYTES],
) -> Vec<u8> {
    let mut aad = Vec::with_capacity(BACKUP_KEY_ENVELOPE_AAD_DOMAIN.len() + header_prefix.len());
    aad.extend_from_slice(BACKUP_KEY_ENVELOPE_AAD_DOMAIN);
    aad.extend_from_slice(header_prefix);
    aad
}

struct ParsedRepositoryHeader<'a> {
    protector_kind: BackupProtectorKind,
    wrap_algorithm: BackupWrapAlgorithm,
    repository_id: BackupRepositoryId,
    library_binding: BackupLibraryBinding,
    header_prefix: [u8; BACKUP_REPOSITORY_HEADER_PREFIX_BYTES],
    envelope: &'a [u8],
}

fn parse_repository_header(
    encoded: &[u8],
) -> Result<ParsedRepositoryHeader<'_>, BackupKeyAuthorityError> {
    if encoded.len() < BACKUP_REPOSITORY_HEADER_PREFIX_BYTES {
        return Err(BackupKeyAuthorityError::InvalidRepositoryHeader);
    }
    let header_prefix: [u8; BACKUP_REPOSITORY_HEADER_PREFIX_BYTES] = encoded
        [..BACKUP_REPOSITORY_HEADER_PREFIX_BYTES]
        .try_into()
        .expect("fixed repository header prefix");
    if &header_prefix[..8] != BACKUP_REPOSITORY_HEADER_MAGIC
        || u16::from_be_bytes(header_prefix[8..10].try_into().expect("format version"))
            != BACKUP_REPOSITORY_FORMAT_VERSION
        || u16::from_be_bytes(header_prefix[10..12].try_into().expect("header version"))
            != BACKUP_REPOSITORY_HEADER_VERSION
        || header_prefix[14..16] != [0, 0]
        || u16::from_be_bytes(header_prefix[80..82].try_into().expect("keyset version"))
            != BACKUP_KEYSET_VERSION
        || u16::from_be_bytes(header_prefix[82..84].try_into().expect("protector slot"))
            != BACKUP_PROTECTOR_SLOT_V1
        || header_prefix[88..92] != [0, 0, 0, 0]
    {
        return Err(BackupKeyAuthorityError::InvalidRepositoryHeader);
    }

    let protector_kind = match header_prefix[12] {
        1 => BackupProtectorKind::AppleKeychainRootV1,
        2 => BackupProtectorKind::AndroidKeystoreRootV1,
        _ => return Err(BackupKeyAuthorityError::InvalidRepositoryHeader),
    };
    let wrap_algorithm = match header_prefix[13] {
        1 => BackupWrapAlgorithm::Aegis256FullTag,
        2 => BackupWrapAlgorithm::Aes256Gcm,
        _ => return Err(BackupKeyAuthorityError::InvalidRepositoryHeader),
    };
    let envelope_len = u32::from_be_bytes(
        header_prefix[84..88]
            .try_into()
            .expect("envelope length"),
    ) as usize;
    if envelope_len == 0
        || envelope_len > BACKUP_MAX_KEY_ENVELOPE_BYTES
        || encoded.len() != BACKUP_REPOSITORY_HEADER_PREFIX_BYTES + envelope_len
    {
        return Err(BackupKeyAuthorityError::InvalidRepositoryHeader);
    }
    validate_protector_tuple(protector_kind, wrap_algorithm, envelope_len)?;

    let repository_id = BackupRepositoryId(
        header_prefix[16..48]
            .try_into()
            .expect("repository id"),
    );
    let library_binding = BackupLibraryBinding(
        header_prefix[48..80]
            .try_into()
            .expect("library binding"),
    );

    Ok(ParsedRepositoryHeader {
        protector_kind,
        wrap_algorithm,
        repository_id,
        library_binding,
        header_prefix,
        envelope: &encoded[BACKUP_REPOSITORY_HEADER_PREFIX_BYTES..],
    })
}

fn validate_protector_tuple(
    protector_kind: BackupProtectorKind,
    wrap_algorithm: BackupWrapAlgorithm,
    envelope_len: usize,
) -> Result<(), BackupKeyAuthorityError> {
    let valid = matches!(
        (protector_kind, wrap_algorithm, envelope_len),
        (
            BackupProtectorKind::AppleKeychainRootV1,
            BackupWrapAlgorithm::Aegis256FullTag,
            BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES
        ) | (
            BackupProtectorKind::AndroidKeystoreRootV1,
            BackupWrapAlgorithm::Aes256Gcm,
            BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES
        )
    );
    if valid {
        Ok(())
    } else {
        Err(BackupKeyAuthorityError::InvalidEnvelope)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rustnote_vault::BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES;

    use super::*;
    use crate::platform::{SecureKeyMaterial, SecureKeyStoreResult};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FakeState {
        Missing,
        Present([u8; 32]),
        Unavailable,
        Corrupt,
    }

    struct FakeProtector {
        state: Mutex<FakeState>,
    }

    impl FakeProtector {
        fn new(state: FakeState) -> Self {
            Self {
                state: Mutex::new(state),
            }
        }
    }

    impl DeviceBackupKeyProtector for FakeProtector {
        fn protector_kind(&self) -> BackupProtectorKind {
            BackupProtectorKind::AppleKeychainRootV1
        }

        fn wrap_algorithm(&self) -> BackupWrapAlgorithm {
            BackupWrapAlgorithm::Aegis256FullTag
        }

        fn envelope_len(&self) -> usize {
            BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES
        }

        fn root_status(&self) -> SecureKeyStoreResult<DeviceBackupRootStatus> {
            match *self.state.lock().expect("fake protector mutex") {
                FakeState::Missing => Ok(DeviceBackupRootStatus::Missing),
                FakeState::Present(_) => Ok(DeviceBackupRootStatus::Present),
                FakeState::Unavailable => Err(SecureKeyStoreError::Unavailable("test".into())),
                FakeState::Corrupt => Err(SecureKeyStoreError::CorruptRootKey),
            }
        }

        fn create_root_if_absent_for_fresh_state(
            &self,
        ) -> SecureKeyStoreResult<DeviceBackupRootCreation> {
            let mut state = self.state.lock().expect("fake protector mutex");
            match *state {
                FakeState::Missing => {
                    *state = FakeState::Present([0x41; 32]);
                    Ok(DeviceBackupRootCreation::Created)
                }
                FakeState::Present(_) => Ok(DeviceBackupRootCreation::AlreadyPresent),
                FakeState::Unavailable => Err(SecureKeyStoreError::Unavailable("test".into())),
                FakeState::Corrupt => Err(SecureKeyStoreError::CorruptRootKey),
            }
        }

        fn seal_repository_keyset(
            &self,
            aad: &[u8],
            keyset: &[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES],
        ) -> SecureKeyStoreResult<Vec<u8>> {
            let key = match *self.state.lock().expect("fake protector mutex") {
                FakeState::Present(key) => key,
                FakeState::Missing => return Err(SecureKeyStoreError::MissingRootKey),
                FakeState::Unavailable => {
                    return Err(SecureKeyStoreError::Unavailable("test".into()));
                }
                FakeState::Corrupt => return Err(SecureKeyStoreError::CorruptRootKey),
            };
            BackupKeyEnvelopeService::new()
                .seal_apple_keyset_v1(
                    &BackupDeviceRootKey::from_zeroizing(Zeroizing::new(key)),
                    aad,
                    keyset,
                )
                .map(|value| value.to_vec())
                .map_err(|_| SecureKeyStoreError::AuthenticationFailed)
        }

        fn open_repository_keyset(
            &self,
            aad: &[u8],
            envelope: &[u8],
        ) -> SecureKeyStoreResult<SecureKeyMaterial> {
            let key = match *self.state.lock().expect("fake protector mutex") {
                FakeState::Present(key) => key,
                FakeState::Missing => return Err(SecureKeyStoreError::MissingRootKey),
                FakeState::Unavailable => {
                    return Err(SecureKeyStoreError::Unavailable("test".into()));
                }
                FakeState::Corrupt => return Err(SecureKeyStoreError::CorruptRootKey),
            };
            BackupKeyEnvelopeService::new()
                .open_apple_keyset_v1(
                    &BackupDeviceRootKey::from_zeroizing(Zeroizing::new(key)),
                    aad,
                    envelope,
                )
                .map(|value| SecureKeyMaterial::new(value.to_vec()))
                .map_err(|_| SecureKeyStoreError::AuthenticationFailed)
        }
    }

    #[test]
    fn library_binding_matches_2b43_2_canonical_vector() {
        let binding = BackupLibraryBinding::from_library_id("library_example_001")
            .expect("canonical library binding");
        assert_eq!(
            binding.as_bytes(),
            &[
                0xf0, 0x36, 0x3a, 0x84, 0x64, 0xe6, 0xa7, 0x22,
                0x23, 0x7f, 0xbd, 0xfd, 0xb9, 0x8a, 0x0f, 0x86,
                0x9e, 0xf6, 0x0c, 0x0b, 0xce, 0x8a, 0x50, 0x49,
                0xc8, 0x22, 0x67, 0x52, 0x5f, 0xf5, 0x4f, 0xdb,
            ]
        );
    }

    #[test]
    fn fresh_repository_header_matches_exact_2b43_2_apple_layout() {
        let protector = FakeProtector::new(FakeState::Missing);
        let initialized = BackupKeyAuthority::with_protector(&protector)
            .initialize_fresh_repository("library-A")
            .expect("fresh repository initialization");
        let header = initialized.sealed.repository_header_bytes();

        assert_eq!(header.len(), BACKUP_REPOSITORY_HEADER_PREFIX_BYTES + 268);
        assert_eq!(&header[..8], b"RNBHDR01");
        assert_eq!(&header[8..10], &3u16.to_be_bytes());
        assert_eq!(&header[10..12], &1u16.to_be_bytes());
        assert_eq!(header[12], BackupProtectorKind::AppleKeychainRootV1 as u8);
        assert_eq!(header[13], BackupWrapAlgorithm::Aegis256FullTag as u8);
        assert_eq!(&header[14..16], &[0, 0]);
        assert_eq!(&header[80..82], &1u16.to_be_bytes());
        assert_eq!(&header[82..84], &1u16.to_be_bytes());
        assert_eq!(&header[84..88], &268u32.to_be_bytes());
        assert_eq!(&header[88..92], &[0, 0, 0, 0]);
    }

    #[test]
    fn fresh_initialization_creates_root_and_round_trips_keyset() {
        let protector = FakeProtector::new(FakeState::Missing);
        let authority = BackupKeyAuthority::with_protector(&protector);
        let initialized = authority
            .initialize_fresh_repository("library-A")
            .expect("fresh repository initialization");
        assert_eq!(initialized.root_creation, DeviceBackupRootCreation::Created);
        let header = initialized.sealed.repository_header_bytes();
        let reopened = authority
            .open_repository_keyset(
                initialized.keyset.repository_id(),
                initialized.keyset.library_binding(),
                &header,
            )
            .expect("reopen repository keyset");
        assert_eq!(
            initialized.keyset.snapshot_aead_key(),
            reopened.snapshot_aead_key()
        );
        assert_eq!(initialized.keyset.blob_aead_key(), reopened.blob_aead_key());
        assert_eq!(
            initialized.keyset.metadata_aead_key(),
            reopened.metadata_aead_key()
        );
        assert_eq!(
            initialized.keyset.object_id_hmac_key(),
            reopened.object_id_hmac_key()
        );
    }

    #[test]
    fn existing_repository_open_never_recreates_missing_root() {
        let creator = FakeProtector::new(FakeState::Present([0x22; 32]));
        let creator_authority = BackupKeyAuthority::with_protector(&creator);
        let initialized = creator_authority
            .initialize_fresh_repository("library-A")
            .expect("repository initialization");
        let header = initialized.sealed.repository_header_bytes();

        let missing = FakeProtector::new(FakeState::Missing);
        let missing_authority = BackupKeyAuthority::with_protector(&missing);
        let error = missing_authority
            .open_repository_keyset(
                initialized.keyset.repository_id(),
                initialized.keyset.library_binding(),
                &header,
            )
            .expect_err("missing root must fail closed");
        assert!(matches!(error, BackupKeyAuthorityError::RootKeyMissing));
        assert_eq!(
            missing.root_status().expect("fake root status"),
            DeviceBackupRootStatus::Missing
        );
    }

    #[test]
    fn wrong_device_root_cannot_open_repository_keyset() {
        let creator = FakeProtector::new(FakeState::Present([0x11; 32]));
        let initialized = BackupKeyAuthority::with_protector(&creator)
            .initialize_fresh_repository("library-A")
            .expect("repository initialization");
        let header = initialized.sealed.repository_header_bytes();
        let other_device = FakeProtector::new(FakeState::Present([0x33; 32]));
        let error = BackupKeyAuthority::with_protector(&other_device)
            .open_repository_keyset(
                initialized.keyset.repository_id(),
                initialized.keyset.library_binding(),
                &header,
            )
            .expect_err("other device must not open repository");
        assert!(matches!(
            error,
            BackupKeyAuthorityError::AuthenticationFailed
        ));
    }

    #[test]
    fn unavailable_or_corrupt_secure_store_fails_closed() {
        for state in [FakeState::Unavailable, FakeState::Corrupt] {
            let protector = FakeProtector::new(state);
            let error = match BackupKeyAuthority::with_protector(&protector)
                .initialize_fresh_repository("library-A")
            {
                Ok(_) => panic!("secure store failure must stop initialization"),
                Err(error) => error,
            };
            assert!(matches!(
                error,
                BackupKeyAuthorityError::SecureStoreUnavailable(_)
                    | BackupKeyAuthorityError::RootKeyCorrupt
            ));
        }
    }

    #[test]
    fn repository_and_library_context_are_authenticated() {
        let protector = FakeProtector::new(FakeState::Present([0x55; 32]));
        let authority = BackupKeyAuthority::with_protector(&protector);
        let initialized = authority
            .initialize_fresh_repository("library-A")
            .expect("repository initialization");
        let mut header = initialized.sealed.repository_header_bytes();
        header[48] ^= 0x01;
        let error = authority
            .open_repository_keyset(
                initialized.keyset.repository_id(),
                initialized.keyset.library_binding(),
                &header,
            )
            .expect_err("tampered library binding must be rejected");
        assert!(matches!(error, BackupKeyAuthorityError::ContextMismatch));
    }

    #[test]
    fn present_device_root_is_reused_without_replacement() {
        let protector = FakeProtector::new(FakeState::Present([0x71; 32]));
        let initialized = BackupKeyAuthority::with_protector(&protector)
            .initialize_fresh_repository("library-A")
            .expect("fresh repository with existing device root");
        assert_eq!(
            initialized.root_creation,
            DeviceBackupRootCreation::AlreadyPresent
        );
        assert_eq!(
            protector.root_status().expect("root status"),
            DeviceBackupRootStatus::Present
        );
    }

    #[test]
    fn repository_header_rejects_reserved_unknown_and_noncanonical_envelope_fields() {
        let protector = FakeProtector::new(FakeState::Present([0x72; 32]));
        let authority = BackupKeyAuthority::with_protector(&protector);
        let initialized = authority
            .initialize_fresh_repository("library-A")
            .expect("repository initialization");
        let repository_id = initialized.keyset.repository_id();
        let library_binding = initialized.keyset.library_binding();
        let canonical = initialized.sealed.repository_header_bytes();

        for index in [14usize, 15, 88, 89, 90, 91] {
            let mut tampered = canonical.clone();
            tampered[index] = 1;
            assert!(matches!(
                authority.open_repository_keyset(repository_id, library_binding, &tampered),
                Err(BackupKeyAuthorityError::InvalidRepositoryHeader)
            ));
        }

        for (offset, value) in [(12usize, 0xffu8), (13usize, 0xffu8)] {
            let mut tampered = canonical.clone();
            tampered[offset] = value;
            assert!(matches!(
                authority.open_repository_keyset(repository_id, library_binding, &tampered),
                Err(BackupKeyAuthorityError::InvalidRepositoryHeader)
            ));
        }

        let mut wrong_tuple = canonical.clone();
        wrong_tuple[13] = BackupWrapAlgorithm::Aes256Gcm as u8;
        assert!(matches!(
            authority.open_repository_keyset(repository_id, library_binding, &wrong_tuple),
            Err(BackupKeyAuthorityError::InvalidEnvelope)
        ));

        let mut wrong_len = canonical.clone();
        wrong_len[84..88].copy_from_slice(&1u32.to_be_bytes());
        assert!(matches!(
            authority.open_repository_keyset(repository_id, library_binding, &wrong_len),
            Err(BackupKeyAuthorityError::InvalidRepositoryHeader)
                | Err(BackupKeyAuthorityError::InvalidEnvelope)
        ));

        let mut trailing = canonical.clone();
        trailing.push(0);
        assert!(matches!(
            authority.open_repository_keyset(repository_id, library_binding, &trailing),
            Err(BackupKeyAuthorityError::InvalidRepositoryHeader)
        ));
    }

    #[test]
    fn repository_keyset_encoding_is_exact_204_byte_canonical_layout() {
        let protector = FakeProtector::new(FakeState::Present([0x73; 32]));
        let initialized = BackupKeyAuthority::with_protector(&protector)
            .initialize_fresh_repository("library-A")
            .expect("repository initialization");
        let encoded = initialized.keyset.encode();
        assert_eq!(encoded.len(), BACKUP_REPOSITORY_KEYSET_V1_BYTES);
        assert_eq!(&encoded[..8], b"RNBKEY01");
        assert_eq!(&encoded[8..10], &BACKUP_KEYSET_VERSION.to_be_bytes());
        assert_eq!(&encoded[10..12], &[0, 0]);
        assert_eq!(&encoded[12..44], initialized.keyset.repository_id().as_bytes());
        assert_eq!(&encoded[44..76], initialized.keyset.library_binding().as_bytes());
        assert_eq!(&encoded[76..108], initialized.keyset.snapshot_aead_key());
        assert_eq!(&encoded[108..140], initialized.keyset.blob_aead_key());
        assert_eq!(&encoded[140..172], initialized.keyset.metadata_aead_key());
        assert_eq!(&encoded[172..204], initialized.keyset.object_id_hmac_key());
    }

    #[test]
    fn repository_keysets_are_independent_and_redacted() {
        let protector = FakeProtector::new(FakeState::Present([0x66; 32]));
        let authority = BackupKeyAuthority::with_protector(&protector);
        let first = authority
            .initialize_fresh_repository("library-A")
            .expect("first repository");
        let second = authority
            .initialize_fresh_repository("library-A")
            .expect("second repository");
        assert_ne!(first.keyset.repository_id(), second.keyset.repository_id());
        assert_ne!(first.keyset.snapshot_aead_key(), second.keyset.snapshot_aead_key());
        assert!(format!("{:?}", first.keyset).contains("[REDACTED]"));
    }
}
