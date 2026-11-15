//! Rustnote's canonical cryptographic vault boundary.
//!
//! This crate deliberately knows nothing about notes, SQLite, FTS, Dioxus, or
//! cloud providers. Callers provide an authenticated context and receive or
//! supply opaque encrypted bytes. Storage/application integration belongs in
//! later Rustnote passes.

use std::{
    fmt,
    io::{self, Read, Write},
};

use aegis::aegis256::Aegis256;
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 32;
const TAG_BYTES: usize = 32;
const SALT_BYTES: usize = 16;
const _: [(); 32] = [(); KEY_BYTES];
const _: [(); 32] = [(); NONCE_BYTES];
const _: [(); 32] = [(); TAG_BYTES]; // Rustnote v1 requires the full 256-bit AEGIS-256 tag.
const _: [(); 16] = [(); SALT_BYTES]; // RFC 9106-recommended 128-bit salt.

const OBJECT_MAGIC: &[u8; 8] = b"RNVOBJ01";
const KEY_MAGIC: &[u8; 8] = b"RNVKEY01";
const BLOB_MAGIC: &[u8; 8] = b"RNVBLB01";
const FORMAT_VERSION: u16 = 1;
const ALGORITHM_AEGIS_256: u8 = 1;
const KDF_ARGON2ID: u8 = 1;

const OBJECT_HEADER_BYTES: usize = 8 + 2 + 1 + 1 + NONCE_BYTES + 8;
const KEY_HEADER_BYTES: usize = 8 + 2 + 1 + 1 + 4 + 4 + 4 + SALT_BYTES + NONCE_BYTES;
const BLOB_HEADER_BYTES: usize = 8 + 2 + 1 + 1 + 4 + 8 + BLOB_NONCE_PREFIX_BYTES;

const V1_ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const V1_ARGON2_TIME_COST: u32 = 3;
const V1_ARGON2_LANES: u32 = 4;

// Untrusted key envelopes must never be able to demand more resources than
// Rustnote itself uses for the current format. This bounds attacker-controlled
// unlock cost on memory-constrained clients while preserving the exact v1
// profile. A future stronger profile can deliberately raise these limits.
const MAX_ARGON2_MEMORY_KIB: u32 = V1_ARGON2_MEMORY_KIB;
const MAX_ARGON2_TIME_COST: u32 = V1_ARGON2_TIME_COST;
const MAX_ARGON2_LANES: u32 = V1_ARGON2_LANES;

const BLOB_NONCE_PREFIX_BYTES: usize = 24;
const BLOB_NONCE_COUNTER_BYTES: usize = 8;
const _: [(); NONCE_BYTES] = [(); BLOB_NONCE_PREFIX_BYTES + BLOB_NONCE_COUNTER_BYTES];

pub const DEFAULT_PROTECTED_BLOB_CHUNK_BYTES: usize = 1024 * 1024;
pub const MIN_PROTECTED_BLOB_CHUNK_BYTES: usize = 4 * 1024;
pub const MAX_PROTECTED_BLOB_CHUNK_BYTES: usize = 8 * 1024 * 1024;

/// 2b.43 managed-backup macOS root-key size.
pub const BACKUP_DEVICE_ROOT_KEY_BYTES: usize = 32;
/// Exact `RepositoryKeysetV1` plaintext size from the 2b.43.2 contract.
pub const BACKUP_REPOSITORY_KEYSET_V1_BYTES: usize = 204;
/// Exact Apple/Keychain AEGIS envelope size from the 2b.43.2 contract.
pub const BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES: usize = 268;
/// Exact Android Keystore AES-256-GCM envelope size from the 2b.43.2 contract.
pub const BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES: usize = 232;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum VaultDomain {
    /// Reserved historical domain value. Production protected-note persistence
    /// uses the granular object domains below; keeping the numeric namespace
    /// stable does not retain a legacy persistence representation.
    Note = 1,
    Notebook = 2,
    Attachment = 3,
    Revision = 4,
    KeyEnvelope = 5,
    /// 2b granular protected-note root/manifest object.
    Manifest = 6,
    /// 2b high-frequency current note state.
    NotePrivateState = 7,
    /// 2b lower-frequency tags/links/sources/annotation metadata.
    NoteMetadata = 8,
    /// 2b attachment metadata and encrypted-blob reference catalog.
    AttachmentIndex = 9,
    /// 2b compact encrypted retained-revision membership/summary index.
    RevisionIndex = 10,
    Generic = 255,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VaultContext<'a> {
    domain: VaultDomain,
    object_id: &'a [u8],
}

impl<'a> VaultContext<'a> {
    pub const fn new(domain: VaultDomain, object_id: &'a [u8]) -> Self {
        Self { domain, object_id }
    }

    pub const fn domain(self) -> VaultDomain {
        self.domain
    }

    pub const fn object_id(self) -> &'a [u8] {
        self.object_id
    }
}

/// A random 256-bit Rustnote scope key.
///
/// It is intentionally non-`Clone` and redacts `Debug`. Its owned bytes are
/// zeroized on drop. Callers should keep the lifetime of this value bounded to
/// the unlocked scope/session.
pub struct VaultKey(Zeroizing<[u8; KEY_BYTES]>);

impl VaultKey {
    fn from_zeroizing(bytes: Zeroizing<[u8; KEY_BYTES]>) -> Self {
        Self(bytes)
    }

    fn bytes(&self) -> &[u8; KEY_BYTES] {
        &self.0
    }

    #[cfg(test)]
    fn test_bytes(&self) -> [u8; KEY_BYTES] {
        *self.bytes()
    }
}

impl fmt::Debug for VaultKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultKey([REDACTED])")
    }
}

/// Device-local managed-backup wrapping key material for platforms whose secure
/// store returns symmetric bytes to Rust (currently macOS Keychain).
///
/// Android deliberately does not use this type: its Keystore wrapping key stays
/// non-exportable and the platform shell performs AES-GCM seal/open operations.
pub struct BackupDeviceRootKey(Zeroizing<[u8; BACKUP_DEVICE_ROOT_KEY_BYTES]>);

impl BackupDeviceRootKey {
    /// Accept already-zeroizing material so callers never need an ordinary
    /// secret-bearing stack array merely to cross the crypto boundary.
    pub fn from_zeroizing(bytes: Zeroizing<[u8; BACKUP_DEVICE_ROOT_KEY_BYTES]>) -> Self {
        Self(bytes)
    }

    fn bytes(&self) -> &[u8; BACKUP_DEVICE_ROOT_KEY_BYTES] {
        &self.0
    }
}

impl fmt::Debug for BackupDeviceRootKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BackupDeviceRootKey([REDACTED])")
    }
}

/// Narrow AEGIS-256 helper for the macOS managed-backup repository-key envelope.
/// The caller supplies the exact 2b.43.2 AAD. No backup repository/storage policy
/// lives in this cryptographic crate.
#[derive(Clone, Copy, Debug, Default)]
pub struct BackupKeyEnvelopeService;

impl BackupKeyEnvelopeService {
    pub const fn new() -> Self {
        Self
    }

    pub fn seal_apple_keyset_v1(
        &self,
        root_key: &BackupDeviceRootKey,
        aad: &[u8],
        plaintext: &[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES],
    ) -> Result<[u8; BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES], VaultError> {
        self.seal_apple_keyset_v1_with_nonce(root_key, aad, plaintext, random_nonce()?)
    }

    fn seal_apple_keyset_v1_with_nonce(
        &self,
        root_key: &BackupDeviceRootKey,
        aad: &[u8],
        plaintext: &[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES],
        nonce: [u8; NONCE_BYTES],
    ) -> Result<[u8; BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES], VaultError> {
        let (ciphertext, tag) =
            Aegis256::<TAG_BYTES>::new(root_key.bytes(), &nonce).encrypt(plaintext, aad);
        if ciphertext.len() != BACKUP_REPOSITORY_KEYSET_V1_BYTES {
            return Err(VaultError::LengthMismatch);
        }

        let mut encoded = [0u8; BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES];
        encoded[..NONCE_BYTES].copy_from_slice(&nonce);
        encoded[NONCE_BYTES..NONCE_BYTES + BACKUP_REPOSITORY_KEYSET_V1_BYTES]
            .copy_from_slice(&ciphertext);
        encoded[NONCE_BYTES + BACKUP_REPOSITORY_KEYSET_V1_BYTES..].copy_from_slice(&tag);
        Ok(encoded)
    }

    pub fn open_apple_keyset_v1(
        &self,
        root_key: &BackupDeviceRootKey,
        aad: &[u8],
        encoded: &[u8],
    ) -> Result<Zeroizing<[u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES]>, VaultError> {
        if encoded.len() != BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES {
            return Err(VaultError::LengthMismatch);
        }

        let mut nonce = [0u8; NONCE_BYTES];
        nonce.copy_from_slice(&encoded[..NONCE_BYTES]);
        let ciphertext = &encoded[NONCE_BYTES..NONCE_BYTES + BACKUP_REPOSITORY_KEYSET_V1_BYTES];
        let mut tag = [0u8; TAG_BYTES];
        tag.copy_from_slice(&encoded[NONCE_BYTES + BACKUP_REPOSITORY_KEYSET_V1_BYTES..]);
        let plaintext = Zeroizing::new(
            Aegis256::<TAG_BYTES>::new(root_key.bytes(), &nonce)
                .decrypt(ciphertext, &tag, aad)
                .map_err(|_| VaultError::AuthenticationFailed)?,
        );
        if plaintext.len() != BACKUP_REPOSITORY_KEYSET_V1_BYTES {
            return Err(VaultError::LengthMismatch);
        }

        let mut keyset = Zeroizing::new([0u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES]);
        keyset.copy_from_slice(&plaintext);
        Ok(keyset)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Argon2idParams {
    memory_kib: u32,
    time_cost: u32,
    lanes: u32,
}

impl Argon2idParams {
    /// Rustnote v1 interactive password-wrapping profile.
    ///
    /// This is RFC 9106's lower-memory Argon2id recommendation (64 MiB, three
    /// passes, four lanes). The parameters are serialized in every envelope, so
    /// future versions may raise/lower interactive cost without invalidating
    /// existing protected notes.
    pub const V1_INTERACTIVE: Self = Self {
        memory_kib: V1_ARGON2_MEMORY_KIB,
        time_cost: V1_ARGON2_TIME_COST,
        lanes: V1_ARGON2_LANES,
    };

    fn build_argon2(self) -> Result<Argon2<'static>, VaultError> {
        if self.memory_kib > MAX_ARGON2_MEMORY_KIB
            || self.time_cost > MAX_ARGON2_TIME_COST
            || self.lanes > MAX_ARGON2_LANES
        {
            return Err(VaultError::InvalidKdfParameters);
        }
        let params = Params::new(self.memory_kib, self.time_cost, self.lanes, Some(KEY_BYTES))
            .map_err(|_| VaultError::InvalidKdfParameters)?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

#[derive(Debug)]
pub enum VaultError {
    AuthenticationFailed,
    InvalidFormat(&'static str),
    UnsupportedVersion(u16),
    UnsupportedAlgorithm(u8),
    UnsupportedKdf(u8),
    InvalidKdfParameters,
    KeyDerivationFailed,
    RandomSource,
    LengthOverflow,
    LengthMismatch,
    Io(io::Error),
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationFailed => formatter.write_str("vault authentication failed"),
            Self::InvalidFormat(reason) => write!(formatter, "invalid vault format: {reason}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported vault format version: {version}")
            }
            Self::UnsupportedAlgorithm(algorithm) => {
                write!(formatter, "unsupported vault algorithm: {algorithm}")
            }
            Self::UnsupportedKdf(kdf) => write!(formatter, "unsupported vault KDF: {kdf}"),
            Self::InvalidKdfParameters => formatter.write_str("invalid Argon2id parameters"),
            Self::KeyDerivationFailed => formatter.write_str("Argon2id key derivation failed"),
            Self::RandomSource => formatter.write_str("secure random source failed"),
            Self::LengthOverflow => formatter.write_str("vault payload length is too large"),
            Self::LengthMismatch => formatter.write_str("vault payload length does not match"),
            Self::Io(error) => write!(formatter, "vault I/O failed: {error}"),
        }
    }
}

impl std::error::Error for VaultError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for VaultError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VaultService;

impl VaultService {
    pub const fn new() -> Self {
        Self
    }

    pub fn generate_key(&self) -> Result<VaultKey, VaultError> {
        let mut bytes = Zeroizing::new([0u8; KEY_BYTES]);
        secure_random(bytes.as_mut())?;
        Ok(VaultKey::from_zeroizing(bytes))
    }

    /// Encrypt a bounded in-memory object with AEGIS-256 and a 256-bit tag.
    pub fn seal_object(
        &self,
        key: &VaultKey,
        context: VaultContext<'_>,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, VaultError> {
        let plaintext_len =
            u64::try_from(plaintext.len()).map_err(|_| VaultError::LengthOverflow)?;
        let nonce = random_nonce()?;
        let header = object_header(plaintext_len, &nonce);
        let aad = authenticated_context(b"object", &header, context)?;
        let (ciphertext, tag) =
            Aegis256::<TAG_BYTES>::new(key.bytes(), &nonce).encrypt(plaintext, &aad);

        let mut output = Vec::with_capacity(OBJECT_HEADER_BYTES + ciphertext.len() + TAG_BYTES);
        output.extend_from_slice(&header);
        output.extend_from_slice(&ciphertext);
        output.extend_from_slice(&tag);
        Ok(output)
    }

    /// Authenticate and decrypt an in-memory object.
    ///
    /// Returned plaintext is zeroized when the returned owner is dropped.
    pub fn open_object(
        &self,
        key: &VaultKey,
        context: VaultContext<'_>,
        encoded: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        let parsed = parse_object(encoded)?;
        let aad = authenticated_context(b"object", parsed.header, context)?;
        let plaintext = Aegis256::<TAG_BYTES>::new(key.bytes(), &parsed.nonce)
            .decrypt(parsed.ciphertext, &parsed.tag, &aad)
            .map_err(|_| VaultError::AuthenticationFailed)?;
        Ok(Zeroizing::new(plaintext))
    }

    /// Protect a random Rustnote scope key with a password-derived Argon2id key.
    /// The password-derived key wraps the random scope key; it never encrypts
    /// note or attachment payloads directly.
    pub fn wrap_key_with_password(
        &self,
        key: &VaultKey,
        password: &[u8],
        context: VaultContext<'_>,
        params: Argon2idParams,
    ) -> Result<Vec<u8>, VaultError> {
        let mut salt = [0u8; SALT_BYTES];
        secure_random(&mut salt)?;
        let wrapping_key = derive_password_key(password, &salt, params)?;
        let nonce = random_nonce()?;
        let header = key_header(params, &salt, &nonce);
        let aad = authenticated_context(b"key-envelope", &header, context)?;
        let (ciphertext, tag) =
            Aegis256::<TAG_BYTES>::new(&wrapping_key, &nonce).encrypt(key.bytes(), &aad);

        let mut output = Vec::with_capacity(KEY_HEADER_BYTES + KEY_BYTES + TAG_BYTES);
        output.extend_from_slice(&header);
        output.extend_from_slice(&ciphertext);
        output.extend_from_slice(&tag);
        Ok(output)
    }

    pub fn unwrap_key_with_password(
        &self,
        password: &[u8],
        context: VaultContext<'_>,
        encoded: &[u8],
    ) -> Result<VaultKey, VaultError> {
        let parsed = parse_key_envelope(encoded)?;
        let wrapping_key = derive_password_key(password, &parsed.salt, parsed.params)?;
        let aad = authenticated_context(b"key-envelope", parsed.header, context)?;
        let plaintext = Zeroizing::new(
            Aegis256::<TAG_BYTES>::new(&wrapping_key, &parsed.nonce)
                .decrypt(parsed.ciphertext, &parsed.tag, &aad)
                .map_err(|_| VaultError::AuthenticationFailed)?,
        );
        if plaintext.len() != KEY_BYTES {
            return Err(VaultError::InvalidFormat("wrapped key has wrong length"));
        }

        let mut bytes = Zeroizing::new([0u8; KEY_BYTES]);
        bytes.copy_from_slice(&plaintext);
        Ok(VaultKey::from_zeroizing(bytes))
    }

    /// Stream-encrypt an attachment-sized payload as independently
    /// authenticated AEGIS-256 records.
    ///
    /// `plaintext_len` must be known by the caller (normal attachment files have
    /// this metadata). Memory is bounded by `chunk_bytes`. Each chunk can be
    /// authenticated before it is written on decryption.
    pub fn seal_protected_blob<R: Read, W: Write>(
        &self,
        key: &VaultKey,
        context: VaultContext<'_>,
        mut reader: R,
        mut writer: W,
        plaintext_len: u64,
        chunk_bytes: usize,
    ) -> Result<(), VaultError> {
        validate_chunk_bytes(chunk_bytes)?;
        let chunk_bytes_u32 = u32::try_from(chunk_bytes).map_err(|_| VaultError::LengthOverflow)?;
        let mut nonce_prefix = [0u8; BLOB_NONCE_PREFIX_BYTES];
        secure_random(&mut nonce_prefix)?;
        let header = blob_header(chunk_bytes_u32, plaintext_len, &nonce_prefix);
        let header_aad = authenticated_context(b"blob-header", &header, context)?;
        let header_nonce = blob_nonce(&nonce_prefix, u64::MAX);
        let (_, header_tag) =
            Aegis256::<TAG_BYTES>::new(key.bytes(), &header_nonce).encrypt(&[], &header_aad);

        writer.write_all(&header)?;
        writer.write_all(&header_tag)?;

        let total_chunks = chunk_count(plaintext_len, chunk_bytes)?;
        let mut buffer = Zeroizing::new(vec![0u8; chunk_bytes]);
        for index in 0..total_chunks {
            let expected = chunk_plaintext_len(plaintext_len, chunk_bytes, index)?;
            read_exact_payload(&mut reader, &mut buffer[..expected])?;
            let nonce = blob_nonce(&nonce_prefix, index);
            let aad = blob_chunk_aad(&header, context, index, expected)?;
            let (ciphertext, tag) =
                Aegis256::<TAG_BYTES>::new(key.bytes(), &nonce).encrypt(&buffer[..expected], &aad);
            writer.write_all(&ciphertext)?;
            writer.write_all(&tag)?;
        }

        let mut extra = [0u8; 1];
        if reader.read(&mut extra)? != 0 {
            return Err(VaultError::LengthMismatch);
        }
        writer.flush()?;
        Ok(())
    }

    /// Stream-decrypt a protected blob.
    ///
    /// On error the writer may already contain a prefix made only of chunks
    /// that authenticated successfully. Integration code must therefore write
    /// to an unpublished temporary file/buffer and expose it only after this
    /// function returns `Ok(())`.
    pub fn open_protected_blob<R: Read, W: Write>(
        &self,
        key: &VaultKey,
        context: VaultContext<'_>,
        mut reader: R,
        mut writer: W,
    ) -> Result<ProtectedBlobInfo, VaultError> {
        let mut header = [0u8; BLOB_HEADER_BYTES];
        read_exact_format(&mut reader, &mut header)?;
        let parsed = parse_blob_header(&header)?;
        let mut header_tag = [0u8; TAG_BYTES];
        read_exact_format(&mut reader, &mut header_tag)?;

        let header_aad = authenticated_context(b"blob-header", &header, context)?;
        let header_nonce = blob_nonce(&parsed.nonce_prefix, u64::MAX);
        Aegis256::<TAG_BYTES>::new(key.bytes(), &header_nonce)
            .decrypt(&[], &header_tag, &header_aad)
            .map_err(|_| VaultError::AuthenticationFailed)?;

        let chunk_bytes =
            usize::try_from(parsed.chunk_bytes).map_err(|_| VaultError::LengthOverflow)?;
        let total_chunks = chunk_count(parsed.plaintext_len, chunk_bytes)?;
        for index in 0..total_chunks {
            let expected = chunk_plaintext_len(parsed.plaintext_len, chunk_bytes, index)?;
            let mut ciphertext = Zeroizing::new(vec![0u8; expected]);
            read_exact_format(&mut reader, &mut ciphertext)?;
            let mut tag = [0u8; TAG_BYTES];
            read_exact_format(&mut reader, &mut tag)?;
            let nonce = blob_nonce(&parsed.nonce_prefix, index);
            let aad = blob_chunk_aad(&header, context, index, expected)?;
            let plaintext = Zeroizing::new(
                Aegis256::<TAG_BYTES>::new(key.bytes(), &nonce)
                    .decrypt(&ciphertext, &tag, &aad)
                    .map_err(|_| VaultError::AuthenticationFailed)?,
            );
            writer.write_all(&plaintext)?;
        }

        let mut trailing = [0u8; 1];
        if reader.read(&mut trailing)? != 0 {
            return Err(VaultError::InvalidFormat(
                "protected blob has trailing data",
            ));
        }
        writer.flush()?;
        Ok(ProtectedBlobInfo {
            plaintext_len: parsed.plaintext_len,
            chunk_bytes: parsed.chunk_bytes,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtectedBlobInfo {
    pub plaintext_len: u64,
    pub chunk_bytes: u32,
}

fn secure_random(output: &mut [u8]) -> Result<(), VaultError> {
    getrandom::fill(output).map_err(|_| VaultError::RandomSource)
}

fn random_nonce() -> Result<[u8; NONCE_BYTES], VaultError> {
    let mut nonce = [0u8; NONCE_BYTES];
    secure_random(&mut nonce)?;
    Ok(nonce)
}

fn derive_password_key(
    password: &[u8],
    salt: &[u8; SALT_BYTES],
    params: Argon2idParams,
) -> Result<Zeroizing<[u8; KEY_BYTES]>, VaultError> {
    let mut output = Zeroizing::new([0u8; KEY_BYTES]);
    params
        .build_argon2()?
        .hash_password_into(password, salt, &mut *output)
        .map_err(|_| VaultError::KeyDerivationFailed)?;
    Ok(output)
}

fn object_header(plaintext_len: u64, nonce: &[u8; NONCE_BYTES]) -> [u8; OBJECT_HEADER_BYTES] {
    let mut header = [0u8; OBJECT_HEADER_BYTES];
    header[..8].copy_from_slice(OBJECT_MAGIC);
    header[8..10].copy_from_slice(&FORMAT_VERSION.to_be_bytes());
    header[10] = ALGORITHM_AEGIS_256;
    header[11] = 0;
    header[12..44].copy_from_slice(nonce);
    header[44..52].copy_from_slice(&plaintext_len.to_be_bytes());
    header
}

fn key_header(
    params: Argon2idParams,
    salt: &[u8; SALT_BYTES],
    nonce: &[u8; NONCE_BYTES],
) -> [u8; KEY_HEADER_BYTES] {
    let mut header = [0u8; KEY_HEADER_BYTES];
    header[..8].copy_from_slice(KEY_MAGIC);
    header[8..10].copy_from_slice(&FORMAT_VERSION.to_be_bytes());
    header[10] = ALGORITHM_AEGIS_256;
    header[11] = KDF_ARGON2ID;
    header[12..16].copy_from_slice(&params.memory_kib.to_be_bytes());
    header[16..20].copy_from_slice(&params.time_cost.to_be_bytes());
    header[20..24].copy_from_slice(&params.lanes.to_be_bytes());
    header[24..40].copy_from_slice(salt);
    header[40..72].copy_from_slice(nonce);
    header
}

fn blob_header(
    chunk_bytes: u32,
    plaintext_len: u64,
    nonce_prefix: &[u8; BLOB_NONCE_PREFIX_BYTES],
) -> [u8; BLOB_HEADER_BYTES] {
    let mut header = [0u8; BLOB_HEADER_BYTES];
    header[..8].copy_from_slice(BLOB_MAGIC);
    header[8..10].copy_from_slice(&FORMAT_VERSION.to_be_bytes());
    header[10] = ALGORITHM_AEGIS_256;
    header[11] = 0;
    header[12..16].copy_from_slice(&chunk_bytes.to_be_bytes());
    header[16..24].copy_from_slice(&plaintext_len.to_be_bytes());
    header[24..24 + BLOB_NONCE_PREFIX_BYTES].copy_from_slice(nonce_prefix);
    header
}

fn authenticated_context(
    purpose: &[u8],
    header: &[u8],
    context: VaultContext<'_>,
) -> Result<Vec<u8>, VaultError> {
    let purpose_len = u16::try_from(purpose.len()).map_err(|_| VaultError::LengthOverflow)?;
    let object_id_len =
        u32::try_from(context.object_id.len()).map_err(|_| VaultError::LengthOverflow)?;
    let mut aad = Vec::with_capacity(32 + purpose.len() + header.len() + context.object_id.len());
    aad.extend_from_slice(b"rustnote-vault-aad-v1");
    aad.extend_from_slice(&purpose_len.to_be_bytes());
    aad.extend_from_slice(purpose);
    aad.push(context.domain as u8);
    aad.extend_from_slice(&object_id_len.to_be_bytes());
    aad.extend_from_slice(context.object_id);
    aad.extend_from_slice(header);
    Ok(aad)
}

fn blob_chunk_aad(
    header: &[u8; BLOB_HEADER_BYTES],
    context: VaultContext<'_>,
    index: u64,
    plaintext_len: usize,
) -> Result<Vec<u8>, VaultError> {
    let mut aad = authenticated_context(b"blob-chunk", header, context)?;
    let plaintext_len = u32::try_from(plaintext_len).map_err(|_| VaultError::LengthOverflow)?;
    aad.extend_from_slice(&index.to_be_bytes());
    aad.extend_from_slice(&plaintext_len.to_be_bytes());
    Ok(aad)
}

fn blob_nonce(prefix: &[u8; BLOB_NONCE_PREFIX_BYTES], index: u64) -> [u8; NONCE_BYTES] {
    let mut nonce = [0u8; NONCE_BYTES];
    nonce[..BLOB_NONCE_PREFIX_BYTES].copy_from_slice(prefix);
    nonce[BLOB_NONCE_PREFIX_BYTES..].copy_from_slice(&index.to_be_bytes());
    nonce
}

fn validate_format_prefix(magic: &[u8; 8], encoded_magic: &[u8]) -> Result<(), VaultError> {
    if encoded_magic != magic {
        return Err(VaultError::InvalidFormat("magic does not match"));
    }
    Ok(())
}

fn validate_version_and_algorithm(version: u16, algorithm: u8) -> Result<(), VaultError> {
    if version != FORMAT_VERSION {
        return Err(VaultError::UnsupportedVersion(version));
    }
    if algorithm != ALGORITHM_AEGIS_256 {
        return Err(VaultError::UnsupportedAlgorithm(algorithm));
    }
    Ok(())
}

struct ParsedObject<'a> {
    header: &'a [u8],
    nonce: [u8; NONCE_BYTES],
    ciphertext: &'a [u8],
    tag: [u8; TAG_BYTES],
}

fn parse_object(encoded: &[u8]) -> Result<ParsedObject<'_>, VaultError> {
    if encoded.len() < OBJECT_HEADER_BYTES + TAG_BYTES {
        return Err(VaultError::InvalidFormat("encrypted object is truncated"));
    }
    validate_format_prefix(OBJECT_MAGIC, &encoded[..8])?;
    let version = u16::from_be_bytes(encoded[8..10].try_into().expect("two-byte version"));
    validate_version_and_algorithm(version, encoded[10])?;
    if encoded[11] != 0 {
        return Err(VaultError::InvalidFormat(
            "reserved object flags are non-zero",
        ));
    }
    let mut nonce = [0u8; NONCE_BYTES];
    nonce.copy_from_slice(&encoded[12..44]);
    let plaintext_len = u64::from_be_bytes(encoded[44..52].try_into().expect("eight-byte length"));
    let ciphertext_len = usize::try_from(plaintext_len).map_err(|_| VaultError::LengthOverflow)?;
    let expected_len = OBJECT_HEADER_BYTES
        .checked_add(ciphertext_len)
        .and_then(|length| length.checked_add(TAG_BYTES))
        .ok_or(VaultError::LengthOverflow)?;
    if encoded.len() != expected_len {
        return Err(VaultError::InvalidFormat(
            "encrypted object length does not match header",
        ));
    }
    let ciphertext_end = OBJECT_HEADER_BYTES + ciphertext_len;
    let mut tag = [0u8; TAG_BYTES];
    tag.copy_from_slice(&encoded[ciphertext_end..]);
    Ok(ParsedObject {
        header: &encoded[..OBJECT_HEADER_BYTES],
        nonce,
        ciphertext: &encoded[OBJECT_HEADER_BYTES..ciphertext_end],
        tag,
    })
}

struct ParsedKeyEnvelope<'a> {
    header: &'a [u8],
    params: Argon2idParams,
    salt: [u8; SALT_BYTES],
    nonce: [u8; NONCE_BYTES],
    ciphertext: &'a [u8],
    tag: [u8; TAG_BYTES],
}

fn parse_key_envelope(encoded: &[u8]) -> Result<ParsedKeyEnvelope<'_>, VaultError> {
    let expected_len = KEY_HEADER_BYTES + KEY_BYTES + TAG_BYTES;
    if encoded.len() != expected_len {
        return Err(VaultError::InvalidFormat("key envelope has wrong length"));
    }
    validate_format_prefix(KEY_MAGIC, &encoded[..8])?;
    let version = u16::from_be_bytes(encoded[8..10].try_into().expect("two-byte version"));
    validate_version_and_algorithm(version, encoded[10])?;
    if encoded[11] != KDF_ARGON2ID {
        return Err(VaultError::UnsupportedKdf(encoded[11]));
    }
    let params = Argon2idParams {
        memory_kib: u32::from_be_bytes(encoded[12..16].try_into().expect("four-byte memory")),
        time_cost: u32::from_be_bytes(encoded[16..20].try_into().expect("four-byte time")),
        lanes: u32::from_be_bytes(encoded[20..24].try_into().expect("four-byte lanes")),
    };
    params.build_argon2()?;
    let mut salt = [0u8; SALT_BYTES];
    salt.copy_from_slice(&encoded[24..40]);
    let mut nonce = [0u8; NONCE_BYTES];
    nonce.copy_from_slice(&encoded[40..72]);
    let ciphertext_end = KEY_HEADER_BYTES + KEY_BYTES;
    let mut tag = [0u8; TAG_BYTES];
    tag.copy_from_slice(&encoded[ciphertext_end..]);
    Ok(ParsedKeyEnvelope {
        header: &encoded[..KEY_HEADER_BYTES],
        params,
        salt,
        nonce,
        ciphertext: &encoded[KEY_HEADER_BYTES..ciphertext_end],
        tag,
    })
}

struct ParsedBlobHeader {
    chunk_bytes: u32,
    plaintext_len: u64,
    nonce_prefix: [u8; BLOB_NONCE_PREFIX_BYTES],
}

fn parse_blob_header(header: &[u8; BLOB_HEADER_BYTES]) -> Result<ParsedBlobHeader, VaultError> {
    validate_format_prefix(BLOB_MAGIC, &header[..8])?;
    let version = u16::from_be_bytes(header[8..10].try_into().expect("two-byte version"));
    validate_version_and_algorithm(version, header[10])?;
    if header[11] != 0 {
        return Err(VaultError::InvalidFormat(
            "reserved blob flags are non-zero",
        ));
    }
    let chunk_bytes = u32::from_be_bytes(header[12..16].try_into().expect("four-byte chunk size"));
    validate_chunk_bytes(usize::try_from(chunk_bytes).map_err(|_| VaultError::LengthOverflow)?)?;
    let plaintext_len = u64::from_be_bytes(header[16..24].try_into().expect("eight-byte length"));
    let mut nonce_prefix = [0u8; BLOB_NONCE_PREFIX_BYTES];
    nonce_prefix.copy_from_slice(&header[24..24 + BLOB_NONCE_PREFIX_BYTES]);
    Ok(ParsedBlobHeader {
        chunk_bytes,
        plaintext_len,
        nonce_prefix,
    })
}

fn validate_chunk_bytes(chunk_bytes: usize) -> Result<(), VaultError> {
    if !(MIN_PROTECTED_BLOB_CHUNK_BYTES..=MAX_PROTECTED_BLOB_CHUNK_BYTES).contains(&chunk_bytes) {
        return Err(VaultError::InvalidFormat(
            "protected blob chunk size is outside allowed bounds",
        ));
    }
    Ok(())
}

fn chunk_count(plaintext_len: u64, chunk_bytes: usize) -> Result<u64, VaultError> {
    if plaintext_len == 0 {
        return Ok(0);
    }
    let chunk_bytes = u64::try_from(chunk_bytes).map_err(|_| VaultError::LengthOverflow)?;
    Ok(plaintext_len.div_ceil(chunk_bytes))
}

fn chunk_plaintext_len(
    plaintext_len: u64,
    chunk_bytes: usize,
    index: u64,
) -> Result<usize, VaultError> {
    let chunk_bytes_u64 = u64::try_from(chunk_bytes).map_err(|_| VaultError::LengthOverflow)?;
    let offset = index
        .checked_mul(chunk_bytes_u64)
        .ok_or(VaultError::LengthOverflow)?;
    if offset >= plaintext_len {
        return Err(VaultError::LengthMismatch);
    }
    let remaining = plaintext_len - offset;
    usize::try_from(remaining.min(chunk_bytes_u64)).map_err(|_| VaultError::LengthOverflow)
}

fn read_exact_payload(reader: &mut impl Read, buffer: &mut [u8]) -> Result<(), VaultError> {
    reader.read_exact(buffer).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            VaultError::LengthMismatch
        } else {
            VaultError::Io(error)
        }
    })
}

fn read_exact_format(reader: &mut impl Read, buffer: &mut [u8]) -> Result<(), VaultError> {
    reader.read_exact(buffer).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            VaultError::InvalidFormat("protected blob is truncated")
        } else {
            VaultError::Io(error)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const NOTE_ID: &[u8] = b"note-123";
    const FAST_TEST_PARAMS: Argon2idParams = Argon2idParams {
        memory_kib: 32,
        time_cost: 1,
        lanes: 1,
    };

    fn note_context() -> VaultContext<'static> {
        VaultContext::new(VaultDomain::Note, NOTE_ID)
    }

    #[test]
    fn aegis256_v1_uses_full_256_bit_authentication_tag() {
        assert_eq!(TAG_BYTES, 32);

        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let plaintext = b"tag-size-check";
        let encoded = service
            .seal_object(&key, note_context(), plaintext)
            .unwrap();
        assert_eq!(encoded.len(), OBJECT_HEADER_BYTES + plaintext.len() + 32);
    }

    #[test]
    fn v1_argon2_profile_is_the_untrusted_envelope_resource_ceiling() {
        assert_eq!(
            Argon2idParams::V1_INTERACTIVE.memory_kib,
            MAX_ARGON2_MEMORY_KIB
        );
        assert_eq!(
            Argon2idParams::V1_INTERACTIVE.time_cost,
            MAX_ARGON2_TIME_COST
        );
        assert_eq!(Argon2idParams::V1_INTERACTIVE.lanes, MAX_ARGON2_LANES);
        assert_eq!(SALT_BYTES, 16);
        assert_eq!(KEY_BYTES, 32);
    }

    #[test]
    fn generated_keys_are_distinct_and_redacted() {
        let service = VaultService::new();
        let first = service.generate_key().unwrap();
        let second = service.generate_key().unwrap();
        assert_ne!(first.test_bytes(), second.test_bytes());
        assert_eq!(format!("{first:?}"), "VaultKey([REDACTED])");
    }

    #[test]
    fn backup_apple_keyset_envelope_round_trips_with_exact_v1_size() {
        let service = BackupKeyEnvelopeService::new();
        let root = BackupDeviceRootKey::from_zeroizing(Zeroizing::new(
            [0xA5; BACKUP_DEVICE_ROOT_KEY_BYTES],
        ));
        let aad = b"rustnote-backup-key-envelope-aad-v1-test";
        let mut plaintext = [0u8; BACKUP_REPOSITORY_KEYSET_V1_BYTES];
        for (index, byte) in plaintext.iter_mut().enumerate() {
            *byte = (index % 251) as u8;
        }

        let encoded = service
            .seal_apple_keyset_v1(&root, aad, &plaintext)
            .unwrap();
        assert_eq!(encoded.len(), BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES);
        assert_eq!(
            BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES,
            NONCE_BYTES + BACKUP_REPOSITORY_KEYSET_V1_BYTES + TAG_BYTES
        );
        assert_eq!(
            *service.open_apple_keyset_v1(&root, aad, &encoded).unwrap(),
            plaintext
        );
        assert_eq!(format!("{root:?}"), "BackupDeviceRootKey([REDACTED])");
    }

    #[test]
    fn backup_apple_keyset_test_fixture_uses_deterministic_nonce_without_production_api() {
        let service = BackupKeyEnvelopeService::new();
        let root = BackupDeviceRootKey::from_zeroizing(Zeroizing::new(
            [0x17; BACKUP_DEVICE_ROOT_KEY_BYTES],
        ));
        let aad = b"fixture-aad";
        let plaintext = [0x29; BACKUP_REPOSITORY_KEYSET_V1_BYTES];
        let nonce = [0x3B; NONCE_BYTES];

        let first = service
            .seal_apple_keyset_v1_with_nonce(&root, aad, &plaintext, nonce)
            .unwrap();
        let second = service
            .seal_apple_keyset_v1_with_nonce(&root, aad, &plaintext, nonce)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(&first[..NONCE_BYTES], &nonce);
        assert_eq!(
            *service.open_apple_keyset_v1(&root, aad, &first).unwrap(),
            plaintext
        );
    }

    #[test]
    fn backup_apple_keyset_envelope_authenticates_aad_key_and_ciphertext() {
        let service = BackupKeyEnvelopeService::new();
        let root = BackupDeviceRootKey::from_zeroizing(Zeroizing::new(
            [0x31; BACKUP_DEVICE_ROOT_KEY_BYTES],
        ));
        let wrong_root = BackupDeviceRootKey::from_zeroizing(Zeroizing::new(
            [0x32; BACKUP_DEVICE_ROOT_KEY_BYTES],
        ));
        let aad = b"repository-header-aad";
        let plaintext = [0x4D; BACKUP_REPOSITORY_KEYSET_V1_BYTES];
        let encoded = service
            .seal_apple_keyset_v1(&root, aad, &plaintext)
            .unwrap();

        assert!(matches!(
            service.open_apple_keyset_v1(&root, b"wrong-aad", &encoded),
            Err(VaultError::AuthenticationFailed)
        ));
        assert!(matches!(
            service.open_apple_keyset_v1(&wrong_root, aad, &encoded),
            Err(VaultError::AuthenticationFailed)
        ));

        let mut tampered = encoded;
        tampered[NONCE_BYTES + 17] ^= 0x80;
        assert!(matches!(
            service.open_apple_keyset_v1(&root, aad, &tampered),
            Err(VaultError::AuthenticationFailed)
        ));
        assert!(matches!(
            service.open_apple_keyset_v1(&root, aad, &tampered[..tampered.len() - 1]),
            Err(VaultError::LengthMismatch)
        ));
    }

    #[test]
    fn object_round_trip_and_random_nonce() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let plaintext = b"private Rustnote text";
        let first = service
            .seal_object(&key, note_context(), plaintext)
            .unwrap();
        let second = service
            .seal_object(&key, note_context(), plaintext)
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(
            &*service.open_object(&key, note_context(), &first).unwrap(),
            plaintext
        );
    }

    #[test]
    fn object_rejects_wrong_key_context_and_tampering() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let wrong_key = service.generate_key().unwrap();
        let mut encoded = service
            .seal_object(&key, note_context(), b"secret")
            .unwrap();

        assert!(matches!(
            service.open_object(&wrong_key, note_context(), &encoded),
            Err(VaultError::AuthenticationFailed)
        ));
        assert!(matches!(
            service.open_object(
                &key,
                VaultContext::new(VaultDomain::Note, b"note-456"),
                &encoded
            ),
            Err(VaultError::AuthenticationFailed)
        ));

        let ciphertext_index = OBJECT_HEADER_BYTES;
        encoded[ciphertext_index] ^= 0x80;
        assert!(matches!(
            service.open_object(&key, note_context(), &encoded),
            Err(VaultError::AuthenticationFailed)
        ));
    }

    #[test]
    fn object_rejects_header_version_and_algorithm_changes() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let encoded = service
            .seal_object(&key, note_context(), b"secret")
            .unwrap();

        let mut wrong_version = encoded.clone();
        wrong_version[9] = 2;
        assert!(matches!(
            service.open_object(&key, note_context(), &wrong_version),
            Err(VaultError::UnsupportedVersion(_))
        ));

        let mut wrong_algorithm = encoded;
        wrong_algorithm[10] = 99;
        assert!(matches!(
            service.open_object(&key, note_context(), &wrong_algorithm),
            Err(VaultError::UnsupportedAlgorithm(99))
        ));
    }

    #[test]
    fn password_envelope_wraps_random_key_and_rejects_wrong_password_or_context() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let original = key.test_bytes();
        let envelope = service
            .wrap_key_with_password(
                &key,
                b"correct horse battery staple",
                note_context(),
                FAST_TEST_PARAMS,
            )
            .unwrap();

        let unwrapped = service
            .unwrap_key_with_password(b"correct horse battery staple", note_context(), &envelope)
            .unwrap();
        assert_eq!(unwrapped.test_bytes(), original);
        assert!(matches!(
            service.unwrap_key_with_password(b"wrong password", note_context(), &envelope),
            Err(VaultError::AuthenticationFailed)
        ));
        assert!(matches!(
            service.unwrap_key_with_password(
                b"correct horse battery staple",
                VaultContext::new(VaultDomain::Notebook, NOTE_ID),
                &envelope
            ),
            Err(VaultError::AuthenticationFailed)
        ));
    }

    #[test]
    fn password_envelope_rejects_tampered_kdf_parameters() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let mut envelope = service
            .wrap_key_with_password(&key, b"password", note_context(), FAST_TEST_PARAMS)
            .unwrap();
        envelope[12..16].copy_from_slice(&0u32.to_be_bytes());
        assert!(matches!(
            service.unwrap_key_with_password(b"password", note_context(), &envelope),
            Err(VaultError::InvalidKdfParameters)
        ));
    }

    #[test]
    fn password_envelope_rejects_ciphertext_tampering() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let mut envelope = service
            .wrap_key_with_password(&key, b"password", note_context(), FAST_TEST_PARAMS)
            .unwrap();
        envelope[KEY_HEADER_BYTES] ^= 0x40;

        assert!(matches!(
            service.unwrap_key_with_password(b"password", note_context(), &envelope),
            Err(VaultError::AuthenticationFailed)
        ));
    }

    #[test]
    fn password_envelope_rejects_resource_abuse_parameters_before_derivation() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let mut envelope = service
            .wrap_key_with_password(&key, b"password", note_context(), FAST_TEST_PARAMS)
            .unwrap();
        envelope[12..16].copy_from_slice(&(MAX_ARGON2_MEMORY_KIB + 1).to_be_bytes());

        assert!(matches!(
            service.unwrap_key_with_password(b"password", note_context(), &envelope),
            Err(VaultError::InvalidKdfParameters)
        ));
    }

    #[test]
    fn protected_blob_round_trips_empty_partial_and_multiple_chunks() {
        for plaintext in [
            Vec::new(),
            vec![0x42; MIN_PROTECTED_BLOB_CHUNK_BYTES - 7],
            vec![0xA5; MIN_PROTECTED_BLOB_CHUNK_BYTES * 3 + 17],
        ] {
            let service = VaultService::new();
            let key = service.generate_key().unwrap();
            let context = VaultContext::new(VaultDomain::Attachment, b"attachment-1");
            let mut encrypted = Vec::new();
            service
                .seal_protected_blob(
                    &key,
                    context,
                    Cursor::new(&plaintext),
                    &mut encrypted,
                    plaintext.len() as u64,
                    MIN_PROTECTED_BLOB_CHUNK_BYTES,
                )
                .unwrap();

            let mut decrypted = Vec::new();
            let info = service
                .open_protected_blob(&key, context, Cursor::new(&encrypted), &mut decrypted)
                .unwrap();
            assert_eq!(decrypted, plaintext);
            assert_eq!(info.plaintext_len, plaintext.len() as u64);
            assert_eq!(info.chunk_bytes, MIN_PROTECTED_BLOB_CHUNK_BYTES as u32);
        }
    }

    #[test]
    fn protected_blob_rejects_wrong_context_tampering_truncation_and_trailing_data() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let context = VaultContext::new(VaultDomain::Attachment, b"attachment-1");
        let plaintext = vec![0x7B; MIN_PROTECTED_BLOB_CHUNK_BYTES + 37];
        let mut encrypted = Vec::new();
        service
            .seal_protected_blob(
                &key,
                context,
                Cursor::new(&plaintext),
                &mut encrypted,
                plaintext.len() as u64,
                MIN_PROTECTED_BLOB_CHUNK_BYTES,
            )
            .unwrap();

        let mut sink = Vec::new();
        assert!(matches!(
            service.open_protected_blob(
                &key,
                VaultContext::new(VaultDomain::Attachment, b"attachment-2"),
                Cursor::new(&encrypted),
                &mut sink
            ),
            Err(VaultError::AuthenticationFailed)
        ));

        let mut tampered = encrypted.clone();
        tampered[BLOB_HEADER_BYTES + TAG_BYTES] ^= 0x01;
        assert!(matches!(
            service.open_protected_blob(&key, context, Cursor::new(tampered), Vec::new()),
            Err(VaultError::AuthenticationFailed)
        ));

        let truncated = &encrypted[..encrypted.len() - 1];
        assert!(matches!(
            service.open_protected_blob(&key, context, Cursor::new(truncated), Vec::new()),
            Err(VaultError::InvalidFormat(_))
        ));

        let mut trailing = encrypted;
        trailing.push(0);
        assert!(matches!(
            service.open_protected_blob(&key, context, Cursor::new(trailing), Vec::new()),
            Err(VaultError::InvalidFormat(_))
        ));
    }

    #[test]
    fn protected_blob_rejects_authenticated_header_tampering() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let context = VaultContext::new(VaultDomain::Attachment, b"attachment-1");
        let plaintext = vec![0x55; MIN_PROTECTED_BLOB_CHUNK_BYTES];
        let mut encrypted = Vec::new();
        service
            .seal_protected_blob(
                &key,
                context,
                Cursor::new(&plaintext),
                &mut encrypted,
                plaintext.len() as u64,
                MIN_PROTECTED_BLOB_CHUNK_BYTES,
            )
            .unwrap();

        encrypted[24] ^= 0x01;
        assert!(matches!(
            service.open_protected_blob(&key, context, Cursor::new(encrypted), Vec::new()),
            Err(VaultError::AuthenticationFailed)
        ));
    }

    #[test]
    fn protected_blob_rejects_reordered_equal_sized_records() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let context = VaultContext::new(VaultDomain::Attachment, b"attachment-1");
        let plaintext = vec![0x3C; MIN_PROTECTED_BLOB_CHUNK_BYTES * 2];
        let mut encrypted = Vec::new();
        service
            .seal_protected_blob(
                &key,
                context,
                Cursor::new(&plaintext),
                &mut encrypted,
                plaintext.len() as u64,
                MIN_PROTECTED_BLOB_CHUNK_BYTES,
            )
            .unwrap();

        let prefix_len = BLOB_HEADER_BYTES + TAG_BYTES;
        let record_len = MIN_PROTECTED_BLOB_CHUNK_BYTES + TAG_BYTES;
        let first = encrypted[prefix_len..prefix_len + record_len].to_vec();
        let second = encrypted[prefix_len + record_len..prefix_len + record_len * 2].to_vec();
        encrypted[prefix_len..prefix_len + record_len].copy_from_slice(&second);
        encrypted[prefix_len + record_len..prefix_len + record_len * 2].copy_from_slice(&first);

        assert!(matches!(
            service.open_protected_blob(&key, context, Cursor::new(encrypted), Vec::new()),
            Err(VaultError::AuthenticationFailed)
        ));
    }

    #[test]
    fn protected_blob_rejects_declared_plaintext_length_mismatch_on_seal() {
        let service = VaultService::new();
        let key = service.generate_key().unwrap();
        let context = VaultContext::new(VaultDomain::Attachment, b"attachment-1");
        let plaintext = vec![1u8; 10];

        assert!(matches!(
            service.seal_protected_blob(
                &key,
                context,
                Cursor::new(&plaintext),
                Vec::new(),
                9,
                MIN_PROTECTED_BLOB_CHUNK_BYTES
            ),
            Err(VaultError::LengthMismatch)
        ));
        assert!(matches!(
            service.seal_protected_blob(
                &key,
                context,
                Cursor::new(&plaintext[..9]),
                Vec::new(),
                10,
                MIN_PROTECTED_BLOB_CHUNK_BYTES
            ),
            Err(VaultError::LengthMismatch)
        ));
    }
}
