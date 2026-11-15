use std::fmt::{self, Display, Formatter};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApplicationOperation {
    StartupMaintenance,
    TrashMaintenance,
    AutomaticBackup,
    RestoreNoteRevision,
    ExportArchive,
    ExportPortability,
    ImportPortability,
    UndoPortabilityImport,
    InspectArchive,
    RestoreArchive,
    ManageBackups,
    SaveHistoryLimits,
    SaveTrashRetention,
    SaveStorageLocations,
    PruneHistory,
    AddAttachments,
    DurabilityCheckpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecutionClass {
    Mutation,
    ExclusiveMutation,
    Maintenance,
}

impl ApplicationOperation {
    pub(crate) const fn progress_label(self) -> &'static str {
        match self {
            Self::StartupMaintenance => "Finishing storage maintenance…",
            Self::TrashMaintenance => "Cleaning expired Trash…",
            Self::AutomaticBackup => "Creating automatic backup…",
            Self::RestoreNoteRevision => "Restoring note revision…",
            Self::ExportArchive => "Exporting complete archive…",
            Self::ExportPortability => "Exporting notes…",
            Self::ImportPortability => "Importing notes…",
            Self::UndoPortabilityImport => "Undoing import…",
            Self::InspectArchive => "Inspecting backup…",
            Self::RestoreArchive => "Restoring backup…",
            Self::ManageBackups => "Applying backup retention…",
            Self::SaveHistoryLimits => "Applying version-history limits…",
            Self::SaveTrashRetention => "Applying Trash retention…",
            Self::SaveStorageLocations => "Saving storage locations…",
            Self::PruneHistory => "Pruning version history…",
            Self::AddAttachments => "Adding attachments…",
            Self::DurabilityCheckpoint => "Saving durable library…",
        }
    }

    const fn worker_label(self) -> &'static str {
        match self {
            Self::StartupMaintenance => "startup storage maintenance",
            Self::TrashMaintenance => "Trash maintenance",
            Self::AutomaticBackup => "automatic backup",
            Self::RestoreNoteRevision => "note revision restore",
            Self::ExportArchive => "complete archive export",
            Self::ExportPortability => "portable export",
            Self::ImportPortability => "portable import",
            Self::UndoPortabilityImport => "portable import undo",
            Self::InspectArchive => "backup inspection",
            Self::RestoreArchive => "backup restore",
            Self::ManageBackups => "backup repository maintenance",
            Self::SaveHistoryLimits => "version-history limit update",
            Self::SaveTrashRetention => "Trash retention update",
            Self::SaveStorageLocations => "storage-location update",
            Self::PruneHistory => "version-history prune",
            Self::AddAttachments => "attachment import",
            Self::DurabilityCheckpoint => "durability checkpoint",
        }
    }

    const fn class(self) -> ExecutionClass {
        match self {
            Self::RestoreNoteRevision | Self::RestoreArchive => ExecutionClass::ExclusiveMutation,
            Self::SaveHistoryLimits
            | Self::SaveTrashRetention
            | Self::SaveStorageLocations
            | Self::AddAttachments
            | Self::ImportPortability
            | Self::UndoPortabilityImport => ExecutionClass::Mutation,
            Self::StartupMaintenance
            | Self::TrashMaintenance
            | Self::AutomaticBackup
            | Self::ExportArchive
            | Self::ExportPortability
            | Self::InspectArchive
            | Self::ManageBackups
            | Self::PruneHistory
            | Self::DurabilityCheckpoint => ExecutionClass::Maintenance,
        }
    }

    const fn editing_available(self) -> bool {
        !matches!(self.class(), ExecutionClass::ExclusiveMutation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApplicationExecutionStatus {
    pub(crate) current: Option<ApplicationOperation>,
    pub(crate) queued: Option<ApplicationOperation>,
    pub(crate) progress: String,
    pub(crate) waiting: Option<String>,
    pub(crate) editing_available: bool,
    editing_locks: u32,
    current_ticket: Option<u64>,
    queued_ticket: Option<u64>,
}

impl Default for ApplicationExecutionStatus {
    fn default() -> Self {
        Self {
            current: None,
            queued: None,
            progress: String::new(),
            waiting: None,
            editing_available: true,
            editing_locks: 0,
            current_ticket: None,
            queued_ticket: None,
        }
    }
}

impl ApplicationExecutionStatus {
    pub(crate) fn is_active(&self) -> bool {
        self.current.is_some() || self.queued.is_some() || !self.editing_available
    }

    fn refresh_editing_availability(&mut self) {
        self.editing_available = self.editing_locks == 0
            && self
                .current
                .is_none_or(ApplicationOperation::editing_available);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ApplicationExecutionError {
    QueueFull,
    WorkerFailed {
        label: &'static str,
        message: String,
    },
    ExecutorStopped {
        label: &'static str,
    },
}

impl Display for ApplicationExecutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueueFull => write!(
                formatter,
                "another background operation is already queued; wait for it to finish"
            ),
            Self::WorkerFailed { label, message } => {
                write!(formatter, "{label} worker failed: {message}")
            }
            Self::ExecutorStopped { label } => {
                write!(
                    formatter,
                    "{label} executor stopped before returning a result"
                )
            }
        }
    }
}

impl std::error::Error for ApplicationExecutionError {}

/// Error values returned from executor work implement this local conversion so
/// the executor can preserve domain errors while still representing worker/runtime
/// failures without collapsing everything to `String`.
pub(crate) trait ExecutionFailure: Sized {
    fn from_execution_error(error: ApplicationExecutionError) -> Self;
}

impl ExecutionFailure for ApplicationExecutionError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        error
    }
}

#[derive(Default)]
struct ExecutionQueue {
    next_ticket: u64,
    serving: u64,
}

struct ApplicationExecutorInner {
    queue: Mutex<ExecutionQueue>,
    ready: Condvar,
    status: Mutex<ApplicationExecutionStatus>,
    updates: tokio::sync::watch::Sender<ApplicationExecutionStatus>,
}

/// Application-owned execution boundary for blocking jobs.
///
/// Reads run independently. Mutations, exclusive mutations and maintenance jobs
/// share one bounded FIFO so storage-heavy operations cannot stampede each other.
/// The executor owns no Dioxus state; detached Tokio tasks keep status cleanup
/// reliable even if the requesting component is unmounted.
#[derive(Clone)]
pub(crate) struct ApplicationExecutor {
    inner: Arc<ApplicationExecutorInner>,
}

/// An acquired position in the serialized application-operation queue.
///
/// Whole-library privacy workflows acquire this scheduling turn before they
/// synchronize the live editor or capture a privacy lease. That ordering keeps
/// a privacy lease from being parked behind an earlier serialized operation
/// while still allowing the source editor to be synchronized immediately before
/// the library-wide operation. Dropping an unused turn releases the queue and
/// status state through RAII.
pub(crate) struct ApplicationExecutionTurn {
    operation: ApplicationOperation,
    turn: Option<ExecutionTurn>,
    status_lease: Option<ExecutionStatusLease>,
}

impl ApplicationExecutor {
    pub(crate) fn new() -> Self {
        let status = ApplicationExecutionStatus::default();
        let (updates, _receiver) = tokio::sync::watch::channel(status.clone());
        Self {
            inner: Arc::new(ApplicationExecutorInner {
                queue: Mutex::new(ExecutionQueue::default()),
                ready: Condvar::new(),
                status: Mutex::new(status),
                updates,
            }),
        }
    }

    pub(crate) fn snapshot(&self) -> ApplicationExecutionStatus {
        lock_unpoisoned(&self.inner.status).clone()
    }

    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<ApplicationExecutionStatus> {
        self.inner.updates.subscribe()
    }

    pub(crate) fn editing_available(&self) -> bool {
        lock_unpoisoned(&self.inner.status).editing_available
    }

    /// Execute a non-serialized blocking read or isolated blocking task.
    pub(crate) async fn read<T, E, F>(label: &'static str, work: F) -> Result<T, E>
    where
        T: Send + 'static,
        E: ExecutionFailure + Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        tokio::task::spawn_blocking(work).await.map_err(|error| {
            E::from_execution_error(ApplicationExecutionError::WorkerFailed {
                label,
                message: error.to_string(),
            })
        })?
    }

    /// Start an isolated blocking task immediately and return a ticket that can be
    /// awaited later. This is used when the work must continue across intervening
    /// browser awaits without exposing `spawn_blocking` to UI code.
    pub(crate) fn start_read<T, E, F>(label: &'static str, work: F) -> ApplicationReadTicket<T, E>
    where
        T: Send + 'static,
        E: ExecutionFailure + Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = Self::read(label, work).await;
            let _ = sender.send(result);
        });
        ApplicationReadTicket { receiver, label }
    }

    pub(crate) async fn execute<T, E, F>(
        &self,
        operation: ApplicationOperation,
        work: F,
    ) -> Result<T, E>
    where
        T: Send + 'static,
        E: ExecutionFailure + Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        // All tracked operations share the same bounded FIFO. The class controls
        // semantics such as editing availability without creating parallel queues.
        self.run_serialized(operation, work).await
    }

    async fn run_serialized<T, E, F>(
        &self,
        operation: ApplicationOperation,
        work: F,
    ) -> Result<T, E>
    where
        T: Send + 'static,
        E: ExecutionFailure + Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        // Submission is detached before queue admission so existing mutation
        // semantics survive requester/component cancellation. Explicit
        // whole-library preflight uses begin_operation() directly and remains
        // cancellable until source synchronization has completed.
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        let executor_task = self.clone();
        tokio::spawn(async move {
            let result = match executor_task.begin_operation(operation).await {
                Ok(turn) => turn.execute(work).await,
                Err(error) => Err(E::from_execution_error(error)),
            };
            let _ = result_sender.send(result);
        });

        result_receiver.await.map_err(|_| {
            E::from_execution_error(ApplicationExecutionError::ExecutorStopped {
                label: operation.worker_label(),
            })
        })?
    }

    /// Acquire the serialized executor turn without beginning application work.
    ///
    /// This is intentionally separate from privacy authorization. A caller that
    /// needs a whole-library privacy lease must acquire this turn first, then
    /// synchronize its current source editor, then capture fresh library
    /// authority. The detached waiter preserves queue cleanup even if the
    /// requesting UI future is dropped while the turn is queued.
    pub(crate) async fn begin_operation(
        &self,
        operation: ApplicationOperation,
    ) -> Result<ApplicationExecutionTurn, ApplicationExecutionError> {
        let (ticket, queued) = self.reserve_turn()?;
        self.register_request(operation, ticket, queued);

        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        let executor_task = self.clone();
        tokio::spawn(async move {
            let worker_executor = executor_task.clone();
            let worker = tokio::task::spawn_blocking(move || worker_executor.wait_for_turn(ticket));

            let result = match worker.await {
                Ok(turn) => {
                    executor_task.mark_started(operation, ticket);
                    Ok(ApplicationExecutionTurn {
                        operation,
                        turn: Some(turn),
                        status_lease: Some(ExecutionStatusLease::new(
                            executor_task.clone(),
                            ticket,
                        )),
                    })
                }
                Err(error) => {
                    executor_task.finish_status(ticket);
                    Err(ApplicationExecutionError::WorkerFailed {
                        label: operation.worker_label(),
                        message: error.to_string(),
                    })
                }
            };

            let _ = result_sender.send(result);
        });

        result_receiver
            .await
            .map_err(|_| ApplicationExecutionError::ExecutorStopped {
                label: operation.worker_label(),
            })?
    }

    pub(crate) fn pause_editing(&self) -> EditingPauseGuard {
        self.update_status(|status| {
            status.editing_locks = status.editing_locks.saturating_add(1);
            status.refresh_editing_availability();
        });
        EditingPauseGuard {
            executor: self.clone(),
        }
    }

    fn resume_editing(&self) {
        self.update_status(|status| {
            status.editing_locks = status.editing_locks.saturating_sub(1);
            status.refresh_editing_availability();
        });
    }

    fn update_status(&self, update: impl FnOnce(&mut ApplicationExecutionStatus)) {
        let snapshot = {
            let mut status = lock_unpoisoned(&self.inner.status);
            update(&mut status);
            status.clone()
        };
        let _ = self.inner.updates.send_replace(snapshot);
    }

    fn reserve_turn(&self) -> Result<(u64, bool), ApplicationExecutionError> {
        let mut queue = lock_unpoisoned(&self.inner.queue);
        let depth = queue.next_ticket.saturating_sub(queue.serving);
        if depth >= 2 {
            return Err(ApplicationExecutionError::QueueFull);
        }
        let ticket = queue.next_ticket;
        queue.next_ticket = queue.next_ticket.saturating_add(1);
        Ok((ticket, depth > 0))
    }

    fn wait_for_turn(&self, ticket: u64) -> ExecutionTurn {
        let mut queue = lock_unpoisoned(&self.inner.queue);
        while queue.serving != ticket {
            queue = match self.inner.ready.wait(queue) {
                Ok(queue) => queue,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
        ExecutionTurn {
            executor: self.clone(),
            ticket,
        }
    }

    fn register_request(&self, operation: ApplicationOperation, ticket: u64, queued: bool) {
        self.update_status(|status| {
            if queued {
                status.queued = Some(operation);
                status.queued_ticket = Some(ticket);
                status.waiting = Some(format!(
                    "{} is waiting for the current operation",
                    operation.progress_label().trim_end_matches('…')
                ));
            } else {
                status.current = Some(operation);
                status.current_ticket = Some(ticket);
                status.progress = operation.progress_label().to_string();
                status.waiting = None;
                status.refresh_editing_availability();
            }
        });
    }

    fn mark_started(&self, operation: ApplicationOperation, ticket: u64) {
        self.update_status(|status| {
            if status
                .current_ticket
                .is_some_and(|current| current > ticket)
            {
                return;
            }
            status.current = Some(operation);
            status.current_ticket = Some(ticket);
            if status.queued_ticket == Some(ticket) {
                status.queued = None;
                status.queued_ticket = None;
            }
            status.progress = operation.progress_label().to_string();
            status.waiting = status.queued.map(|queued| {
                format!(
                    "{} is waiting for the current operation",
                    queued.progress_label().trim_end_matches('…')
                )
            });
            status.refresh_editing_availability();
        });
    }

    fn finish_status(&self, ticket: u64) {
        self.update_status(|status| {
            if status.current_ticket == Some(ticket) {
                status.current = None;
                status.current_ticket = None;
                if status.queued.is_some() {
                    status.progress = "Waiting for the next background operation…".to_string();
                } else {
                    status.progress.clear();
                }
                status.refresh_editing_availability();
            }
            if status.queued_ticket == Some(ticket) {
                status.queued = None;
                status.queued_ticket = None;
            }
            status.waiting = status.queued.map(|queued| {
                format!(
                    "{} is waiting for the current operation",
                    queued.progress_label().trim_end_matches('…')
                )
            });
        });
    }
}

impl ApplicationExecutionTurn {
    /// Run an intermediate blocking stage while retaining this exact serialized
    /// turn for a later stage. The detached worker owns the turn until the stage
    /// finishes; if the requesting future disappears, the returned turn is
    /// dropped only after the blocking work has stopped.
    pub(crate) async fn stage<T, E, F>(mut self, work: F) -> Result<(Self, T), E>
    where
        T: Send + 'static,
        E: ExecutionFailure + Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        let operation = self.operation;
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = match tokio::task::spawn_blocking(work).await {
                Ok(Ok(value)) => Ok((self, value)),
                Ok(Err(error)) => {
                    self.finish();
                    Err(error)
                }
                Err(error) => {
                    self.finish();
                    Err(E::from_execution_error(
                        ApplicationExecutionError::WorkerFailed {
                            label: operation.worker_label(),
                            message: error.to_string(),
                        },
                    ))
                }
            };
            let _ = result_sender.send(result);
        });

        result_receiver.await.map_err(|_| {
            E::from_execution_error(ApplicationExecutionError::ExecutorStopped {
                label: operation.worker_label(),
            })
        })?
    }

    /// Run the final blocking body while retaining this exact serialized turn.
    /// Component unmount/cancellation cannot let a later operation overlap work
    /// that is still running.
    pub(crate) async fn execute<T, E, F>(self, work: F) -> Result<T, E>
    where
        T: Send + 'static,
        E: ExecutionFailure + Send + 'static,
        F: FnOnce() -> Result<T, E> + Send + 'static,
    {
        let (mut turn, value) = self.stage(work).await?;
        turn.finish();
        Ok(value)
    }

    fn finish(&mut self) {
        if let Some(status_lease) = self.status_lease.take() {
            status_lease.finish();
        }
        drop(self.turn.take());
    }
}

impl Drop for ApplicationExecutionTurn {
    fn drop(&mut self) {
        self.finish();
    }
}

pub(crate) struct ApplicationReadTicket<T, E> {
    receiver: tokio::sync::oneshot::Receiver<Result<T, E>>,
    label: &'static str,
}

impl<T, E> ApplicationReadTicket<T, E>
where
    E: ExecutionFailure,
{
    pub(crate) async fn wait(self) -> Result<T, E> {
        self.receiver.await.map_err(|_| {
            E::from_execution_error(ApplicationExecutionError::ExecutorStopped {
                label: self.label,
            })
        })?
    }
}

pub(crate) struct EditingPauseGuard {
    executor: ApplicationExecutor,
}

impl Drop for EditingPauseGuard {
    fn drop(&mut self) {
        self.executor.resume_editing();
    }
}

struct ExecutionTurn {
    executor: ApplicationExecutor,
    ticket: u64,
}

impl Drop for ExecutionTurn {
    fn drop(&mut self) {
        let mut queue = lock_unpoisoned(&self.executor.inner.queue);
        if queue.serving == self.ticket {
            queue.serving = queue.serving.saturating_add(1);
            self.executor.inner.ready.notify_all();
        }
    }
}

struct ExecutionStatusLease {
    executor: ApplicationExecutor,
    ticket: u64,
    active: bool,
}

impl ExecutionStatusLease {
    fn new(executor: ApplicationExecutor, ticket: u64) -> Self {
        Self {
            executor,
            ticket,
            active: true,
        }
    }

    fn finish(mut self) {
        self.executor.finish_status(self.ticket);
        self.active = false;
    }
}

impl Drop for ExecutionStatusLease {
    fn drop(&mut self) {
        if self.active {
            self.executor.finish_status(self.ticket);
        }
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_classes_preserve_editing_contract() {
        assert_eq!(
            ApplicationOperation::RestoreNoteRevision.class(),
            ExecutionClass::ExclusiveMutation
        );
        assert_eq!(
            ApplicationOperation::RestoreArchive.class(),
            ExecutionClass::ExclusiveMutation
        );
        assert!(!ApplicationOperation::RestoreArchive.editing_available());
        assert!(ApplicationOperation::AddAttachments.editing_available());
        assert_eq!(
            ApplicationOperation::StartupMaintenance.class(),
            ExecutionClass::Maintenance
        );
    }

    #[test]
    fn execution_errors_are_stable_and_actionable() {
        assert!(
            ApplicationExecutionError::QueueFull
                .to_string()
                .contains("queued")
        );
        let error = ApplicationExecutionError::WorkerFailed {
            label: "test operation",
            message: "panic".to_string(),
        };
        assert_eq!(error.to_string(), "test operation worker failed: panic");
    }

    #[test]
    fn serialized_queue_is_bounded_to_current_plus_one_waiter() {
        let executor = ApplicationExecutor::new();
        assert_eq!(executor.reserve_turn().unwrap(), (0, false));
        assert_eq!(executor.reserve_turn().unwrap(), (1, true));
        assert_eq!(
            executor.reserve_turn(),
            Err(ApplicationExecutionError::QueueFull)
        );
    }

    #[test]
    fn editing_pause_is_raii() {
        let executor = ApplicationExecutor::new();
        assert!(executor.editing_available());
        {
            let _pause = executor.pause_editing();
            assert!(!executor.editing_available());
        }
        assert!(executor.editing_available());
    }
}
