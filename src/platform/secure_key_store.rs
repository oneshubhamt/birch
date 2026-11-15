use std::fmt;

#[cfg(target_os = "android")]
use rustnote_vault::BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES;
use rustnote_vault::{BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES, BACKUP_REPOSITORY_KEYSET_V1_BYTES};
use zeroize::Zeroizing;

pub(super) const BACKUP_REPOSITORY_KEYSET_BYTES: usize = BACKUP_REPOSITORY_KEYSET_V1_BYTES;
const APPLE_ENVELOPE_BYTES: usize = BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum BackupProtectorKind {
    AppleKeychainRootV1 = 1,
    AndroidKeystoreRootV1 = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum BackupWrapAlgorithm {
    Aegis256FullTag = 1,
    Aes256Gcm = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceBackupRootStatus {
    Missing,
    Present,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceBackupRootCreation {
    Created,
    AlreadyPresent,
}

/// Secret bytes returned from an OS secure-store operation. Debug output is
/// redacted and the complete owned allocation is overwritten on drop.
pub struct SecureKeyMaterial(Zeroizing<Vec<u8>>);

impl SecureKeyMaterial {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SecureKeyMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecureKeyMaterial([REDACTED])")
    }
}

#[derive(Debug)]
pub enum SecureKeyStoreError {
    UnsupportedPlatform,
    Unavailable(String),
    MissingRootKey,
    CorruptRootKey,
    AuthenticationFailed,
    InvalidEnvelope,
}

impl fmt::Display for SecureKeyStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter
                .write_str("Rustnote managed encrypted backups are unavailable on this platform"),
            Self::Unavailable(message) => {
                write!(formatter, "OS secure key store is unavailable: {message}")
            }
            Self::MissingRootKey => formatter.write_str("device backup root key is missing"),
            Self::CorruptRootKey => formatter.write_str("device backup root key is corrupt"),
            Self::AuthenticationFailed => {
                formatter.write_str("repository key envelope authentication failed")
            }
            Self::InvalidEnvelope => formatter.write_str("repository key envelope is invalid"),
        }
    }
}

impl std::error::Error for SecureKeyStoreError {}

pub type SecureKeyStoreResult<T> = Result<T, SecureKeyStoreError>;

pub(crate) trait DeviceBackupKeyProtector: Send + Sync {
    fn protector_kind(&self) -> BackupProtectorKind;
    fn wrap_algorithm(&self) -> BackupWrapAlgorithm;
    fn envelope_len(&self) -> usize;

    fn root_status(&self) -> SecureKeyStoreResult<DeviceBackupRootStatus>;

    /// Create the process/device root only for a caller that has already proved
    /// it is initializing fresh managed-backup state. Existing roots are never
    /// overwritten.
    fn create_root_if_absent_for_fresh_state(
        &self,
    ) -> SecureKeyStoreResult<DeviceBackupRootCreation>;

    fn seal_repository_keyset(
        &self,
        aad: &[u8],
        keyset: &[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES],
    ) -> SecureKeyStoreResult<Vec<u8>>;

    fn open_repository_keyset(
        &self,
        aad: &[u8],
        envelope: &[u8],
    ) -> SecureKeyStoreResult<SecureKeyMaterial>;
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use core_foundation::{
        base::TCFType, data::CFData, dictionary::CFDictionary, string::CFString,
    };
    use rustnote_vault::{
        BACKUP_DEVICE_ROOT_KEY_BYTES, BackupDeviceRootKey, BackupKeyEnvelopeService,
    };
    use security_framework::{
        access_control::{ProtectionMode, SecAccessControl},
        passwords::{PasswordOptions, generic_password},
        random::SecRandom,
    };
    use security_framework_sys::{
        base::{errSecDuplicateItem, errSecItemNotFound, errSecSuccess},
        item::kSecValueData,
        keychain_item::SecItemAdd,
    };

    const KEYCHAIN_SERVICE: &str = "org.rustnote.backup-device-root";
    const KEYCHAIN_ACCOUNT: &str = "device-backup-root-v1";

    pub(super) struct MacosDeviceBackupKeyProtector;

    fn keychain_lookup_options() -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT);
        options.use_protected_keychain();
        options.set_access_synchronized(Some(false));
        options
    }

    fn keychain_create_options() -> SecureKeyStoreResult<PasswordOptions> {
        let mut options = keychain_lookup_options();
        let access_control = SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleAfterFirstUnlockThisDeviceOnly),
            0,
        )
        .map_err(|error| SecureKeyStoreError::Unavailable(error.to_string()))?;
        options.set_access_control(access_control);
        options.set_label("Rustnote managed-backup device key");
        options.set_description("Rustnote device-local managed-backup wrapping key");
        Ok(options)
    }

    fn load_root_material() -> SecureKeyStoreResult<SecureKeyMaterial> {
        match generic_password(keychain_lookup_options()) {
            Ok(bytes) if bytes.len() == BACKUP_DEVICE_ROOT_KEY_BYTES => {
                Ok(SecureKeyMaterial::new(bytes))
            }
            Ok(bytes) => {
                drop(Zeroizing::new(bytes));
                Err(SecureKeyStoreError::CorruptRootKey)
            }
            Err(error) if error.code() == errSecItemNotFound => {
                Err(SecureKeyStoreError::MissingRootKey)
            }
            Err(error) => Err(SecureKeyStoreError::Unavailable(error.to_string())),
        }
    }

    fn root_key() -> SecureKeyStoreResult<BackupDeviceRootKey> {
        let material = load_root_material()?;
        let mut bytes = Zeroizing::new([0u8; BACKUP_DEVICE_ROOT_KEY_BYTES]);
        bytes.copy_from_slice(material.as_bytes());
        Ok(BackupDeviceRootKey::from_zeroizing(bytes))
    }

    fn add_root_atomically(
        bytes: &[u8; BACKUP_DEVICE_ROOT_KEY_BYTES],
    ) -> SecureKeyStoreResult<DeviceBackupRootCreation> {
        let mut options = keychain_create_options()?;
        #[allow(deprecated)]
        options.query.push((
            unsafe { CFString::wrap_under_get_rule(kSecValueData) },
            CFData::from_buffer(bytes).into_CFType(),
        ));
        #[allow(deprecated)]
        let dictionary = CFDictionary::from_CFType_pairs(&options.query);
        let status = unsafe { SecItemAdd(dictionary.as_concrete_TypeRef(), std::ptr::null_mut()) };
        if status == errSecSuccess {
            Ok(DeviceBackupRootCreation::Created)
        } else if status == errSecDuplicateItem {
            Ok(DeviceBackupRootCreation::AlreadyPresent)
        } else {
            Err(SecureKeyStoreError::Unavailable(format!(
                "Security.framework SecItemAdd failed with status {status}"
            )))
        }
    }

    impl DeviceBackupKeyProtector for MacosDeviceBackupKeyProtector {
        fn protector_kind(&self) -> BackupProtectorKind {
            BackupProtectorKind::AppleKeychainRootV1
        }

        fn wrap_algorithm(&self) -> BackupWrapAlgorithm {
            BackupWrapAlgorithm::Aegis256FullTag
        }

        fn envelope_len(&self) -> usize {
            APPLE_ENVELOPE_BYTES
        }

        fn root_status(&self) -> SecureKeyStoreResult<DeviceBackupRootStatus> {
            match load_root_material() {
                Ok(_) => Ok(DeviceBackupRootStatus::Present),
                Err(SecureKeyStoreError::MissingRootKey) => Ok(DeviceBackupRootStatus::Missing),
                Err(error) => Err(error),
            }
        }

        fn create_root_if_absent_for_fresh_state(
            &self,
        ) -> SecureKeyStoreResult<DeviceBackupRootCreation> {
            match self.root_status()? {
                DeviceBackupRootStatus::Present => {
                    return Ok(DeviceBackupRootCreation::AlreadyPresent);
                }
                DeviceBackupRootStatus::Missing => {}
            }

            let mut bytes = Zeroizing::new([0u8; BACKUP_DEVICE_ROOT_KEY_BYTES]);
            SecRandom::default()
                .copy_bytes(bytes.as_mut())
                .map_err(|error| SecureKeyStoreError::Unavailable(error.to_string()))?;
            let add_result = add_root_atomically(&bytes);
            match add_result? {
                DeviceBackupRootCreation::Created => Ok(DeviceBackupRootCreation::Created),
                DeviceBackupRootCreation::AlreadyPresent => match self.root_status()? {
                    DeviceBackupRootStatus::Present => Ok(DeviceBackupRootCreation::AlreadyPresent),
                    DeviceBackupRootStatus::Missing => Err(SecureKeyStoreError::Unavailable(
                        "device backup root creation raced but no root key is readable".to_string(),
                    )),
                },
            }
        }

        fn seal_repository_keyset(
            &self,
            aad: &[u8],
            keyset: &[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES],
        ) -> SecureKeyStoreResult<Vec<u8>> {
            let root = root_key()?;
            BackupKeyEnvelopeService::new()
                .seal_apple_keyset_v1(&root, aad, keyset)
                .map(|encoded| encoded.to_vec())
                .map_err(map_vault_error)
        }

        fn open_repository_keyset(
            &self,
            aad: &[u8],
            envelope: &[u8],
        ) -> SecureKeyStoreResult<SecureKeyMaterial> {
            if envelope.len() != APPLE_ENVELOPE_BYTES {
                return Err(SecureKeyStoreError::InvalidEnvelope);
            }
            let root = root_key()?;
            BackupKeyEnvelopeService::new()
                .open_apple_keyset_v1(&root, aad, envelope)
                .map(|plaintext| SecureKeyMaterial::new(plaintext.to_vec()))
                .map_err(map_vault_error)
        }
    }

    fn map_vault_error(error: rustnote_vault::VaultError) -> SecureKeyStoreError {
        match error {
            rustnote_vault::VaultError::AuthenticationFailed => {
                SecureKeyStoreError::AuthenticationFailed
            }
            rustnote_vault::VaultError::LengthMismatch
            | rustnote_vault::VaultError::InvalidFormat(_)
            | rustnote_vault::VaultError::UnsupportedVersion(_)
            | rustnote_vault::VaultError::UnsupportedAlgorithm(_) => {
                SecureKeyStoreError::InvalidEnvelope
            }
            other => SecureKeyStoreError::Unavailable(other.to_string()),
        }
    }

    pub(super) static SYSTEM: MacosDeviceBackupKeyProtector = MacosDeviceBackupKeyProtector;
}

#[cfg(target_os = "android")]
mod android {
    use super::*;

    pub(super) struct AndroidDeviceBackupKeyProtector;

    impl DeviceBackupKeyProtector for AndroidDeviceBackupKeyProtector {
        fn protector_kind(&self) -> BackupProtectorKind {
            BackupProtectorKind::AndroidKeystoreRootV1
        }

        fn wrap_algorithm(&self) -> BackupWrapAlgorithm {
            BackupWrapAlgorithm::Aes256Gcm
        }

        fn envelope_len(&self) -> usize {
            BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES
        }

        fn root_status(&self) -> SecureKeyStoreResult<DeviceBackupRootStatus> {
            super::super::mobile::adapter()
                .ok_or(SecureKeyStoreError::UnsupportedPlatform)?
                .backup_root_status()
        }

        fn create_root_if_absent_for_fresh_state(
            &self,
        ) -> SecureKeyStoreResult<DeviceBackupRootCreation> {
            super::super::mobile::adapter()
                .ok_or(SecureKeyStoreError::UnsupportedPlatform)?
                .create_backup_root_if_absent_for_fresh_state()
        }

        fn seal_repository_keyset(
            &self,
            aad: &[u8],
            keyset: &[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES],
        ) -> SecureKeyStoreResult<Vec<u8>> {
            let encoded = super::super::mobile::adapter()
                .ok_or(SecureKeyStoreError::UnsupportedPlatform)?
                .seal_backup_repository_keyset(aad, keyset)?;
            if encoded.len() != BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES {
                return Err(SecureKeyStoreError::InvalidEnvelope);
            }
            Ok(encoded)
        }

        fn open_repository_keyset(
            &self,
            aad: &[u8],
            envelope: &[u8],
        ) -> SecureKeyStoreResult<SecureKeyMaterial> {
            if envelope.len() != BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES {
                return Err(SecureKeyStoreError::InvalidEnvelope);
            }
            let plaintext = super::super::mobile::adapter()
                .ok_or(SecureKeyStoreError::UnsupportedPlatform)?
                .open_backup_repository_keyset(aad, envelope)?;
            if plaintext.as_bytes().len() != BACKUP_REPOSITORY_KEYSET_V1_BYTES {
                return Err(SecureKeyStoreError::InvalidEnvelope);
            }
            Ok(plaintext)
        }
    }

    pub(super) static SYSTEM: AndroidDeviceBackupKeyProtector = AndroidDeviceBackupKeyProtector;
}

#[cfg(not(any(target_os = "macos", target_os = "android")))]
mod unsupported {
    use super::*;

    pub(super) struct UnsupportedDeviceBackupKeyProtector;

    impl DeviceBackupKeyProtector for UnsupportedDeviceBackupKeyProtector {
        fn protector_kind(&self) -> BackupProtectorKind {
            BackupProtectorKind::AppleKeychainRootV1
        }

        fn wrap_algorithm(&self) -> BackupWrapAlgorithm {
            BackupWrapAlgorithm::Aegis256FullTag
        }

        fn envelope_len(&self) -> usize {
            APPLE_ENVELOPE_BYTES
        }

        fn root_status(&self) -> SecureKeyStoreResult<DeviceBackupRootStatus> {
            Err(SecureKeyStoreError::UnsupportedPlatform)
        }

        fn create_root_if_absent_for_fresh_state(
            &self,
        ) -> SecureKeyStoreResult<DeviceBackupRootCreation> {
            Err(SecureKeyStoreError::UnsupportedPlatform)
        }

        fn seal_repository_keyset(
            &self,
            _aad: &[u8],
            _keyset: &[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES],
        ) -> SecureKeyStoreResult<Vec<u8>> {
            Err(SecureKeyStoreError::UnsupportedPlatform)
        }

        fn open_repository_keyset(
            &self,
            _aad: &[u8],
            _envelope: &[u8],
        ) -> SecureKeyStoreResult<SecureKeyMaterial> {
            Err(SecureKeyStoreError::UnsupportedPlatform)
        }
    }

    pub(super) static SYSTEM: UnsupportedDeviceBackupKeyProtector =
        UnsupportedDeviceBackupKeyProtector;
}

pub(crate) fn system_device_backup_key_protector() -> &'static dyn DeviceBackupKeyProtector {
    #[cfg(target_os = "macos")]
    {
        &macos::SYSTEM
    }
    #[cfg(target_os = "android")]
    {
        &android::SYSTEM
    }
    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    {
        &unsupported::SYSTEM
    }
}
