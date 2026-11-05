use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use super::{StorageError, StorageResult};

/// Atomic file output backed by the process-wide crash-durable external output
/// authority. The hidden sibling is owned before creation and restart recovery
/// can reconcile every replace boundary.
pub(super) struct AtomicFileOutput {
    transaction: crate::native::output::ExternalFileOutputTransaction,
    writer: Option<BufWriter<File>>,
}

impl AtomicFileOutput {
    pub(super) fn create(destination: &Path) -> StorageResult<Self> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut transaction =
            crate::native::output::ExternalFileOutputTransaction::begin(destination)?;
        let file = transaction.create_file()?;
        Ok(Self {
            transaction,
            writer: Some(BufWriter::new(file)),
        })
    }

    pub(super) fn writer(&mut self) -> StorageResult<&mut BufWriter<File>> {
        self.writer.as_mut().ok_or_else(|| {
            StorageError::Integrity("atomic output writer is already finalized".to_string())
        })
    }

    pub(super) fn commit<T>(
        mut self,
        verify: impl FnOnce(&Path) -> StorageResult<T>,
    ) -> StorageResult<T> {
        let mut writer = self.writer.take().ok_or_else(|| {
            StorageError::Integrity("atomic output writer is already finalized".to_string())
        })?;
        writer.flush()?;
        let file = writer.into_inner().map_err(|error| error.into_error())?;
        file.sync_all()?;
        drop(file);

        let verified = verify(self.transaction.temporary_path())?;
        self.transaction.commit()?;
        Ok(verified)
    }
}

/// Atomic directory output. Its durable journal owns the complete temporary
/// directory namespace before the directory or any exported child is created.
pub(super) struct AtomicDirectoryOutput {
    transaction: crate::native::output::ExternalDirectoryOutputTransaction,
}

impl AtomicDirectoryOutput {
    pub(super) fn create(destination: &Path) -> StorageResult<Self> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let transaction = crate::native::output::ExternalDirectoryOutputTransaction::begin(destination)?;
        Ok(Self { transaction })
    }

    pub(super) fn path(&self) -> &Path {
        self.transaction.temporary_path()
    }

    pub(super) fn commit<T>(
        self,
        verify: impl FnOnce(&Path) -> StorageResult<T>,
    ) -> StorageResult<(PathBuf, T)> {
        let verified = verify(self.transaction.temporary_path())?;
        let destination = self.transaction.commit()?;
        Ok((destination, verified))
    }
}
