//! Debounced work scheduler for file system events
//!
//! Handles event storms (save-on-every-keystroke, git checkout, etc.)
//! by debouncing events and batching work items.

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, trace, warn};

/// Type of work to perform on a file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkType {
    /// File was created or modified - needs indexing
    Index,
    /// File was deleted - needs removal from index
    Delete,
}

/// A work item representing a file that needs processing
#[derive(Debug, Clone)]
pub struct WorkItem {
    pub path: PathBuf,
    pub work_type: WorkType,
    /// When this item was first queued (for latency tracking)
    pub queued_at: Instant,
}

/// Configuration for the work scheduler
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Debounce window - events for the same file within this window are merged
    pub debounce_ms: u64,
    /// Batch processing window - collect events for this long before processing
    pub batch_window_ms: u64,
    /// Maximum queue size before applying backpressure
    pub max_queue_size: usize,
    /// Poll interval for safety rescan (0 = disabled)
    pub poll_interval_ms: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 100,
            batch_window_ms: 500,
            max_queue_size: 10_000,
            poll_interval_ms: 60_000, // 1 minute
        }
    }
}

/// Internal state for pending events
struct PendingEvents {
    /// Map from path to (work_type, first_seen_time)
    events: HashMap<PathBuf, (WorkType, Instant)>,
    /// Last time we flushed the batch
    last_flush: Instant,
}

impl PendingEvents {
    fn new() -> Self {
        Self {
            events: HashMap::new(),
            last_flush: Instant::now(),
        }
    }
}

/// Work scheduler that debounces file events and produces batches
pub struct WorkScheduler {
    config: SchedulerConfig,
    pending: Arc<Mutex<PendingEvents>>,
    work_tx: Sender<Vec<WorkItem>>,
    work_rx: Receiver<Vec<WorkItem>>,
    shutdown_tx: Sender<()>,
    shutdown_rx: Receiver<()>,
}

impl WorkScheduler {
    /// Create a new work scheduler with the given configuration
    pub fn new(config: SchedulerConfig) -> Self {
        let (work_tx, work_rx) = bounded(16);
        let (shutdown_tx, shutdown_rx) = bounded(1);

        Self {
            config,
            pending: Arc::new(Mutex::new(PendingEvents::new())),
            work_tx,
            work_rx,
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Get a handle for submitting events
    pub fn event_handle(&self) -> EventHandle {
        EventHandle {
            pending: Arc::clone(&self.pending),
            config: self.config.clone(),
        }
    }

    /// Get the receiver for work batches
    pub fn work_receiver(&self) -> Receiver<Vec<WorkItem>> {
        self.work_rx.clone()
    }

    /// Signal the scheduler to shut down
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    /// Run the scheduler loop in the current thread
    ///
    /// This processes pending events and sends batches to the work receiver.
    /// Returns when shutdown is signaled.
    pub fn run(&self) {
        let tick_interval = Duration::from_millis(50);

        loop {
            // Check for shutdown
            if self.shutdown_rx.try_recv().is_ok() {
                debug!("Scheduler received shutdown signal");
                // Flush remaining work
                self.flush_if_ready(true);
                break;
            }

            // Check if we should flush
            self.flush_if_ready(false);

            std::thread::sleep(tick_interval);
        }
    }

    /// Flush pending events if the batch window has elapsed
    fn flush_if_ready(&self, force: bool) {
        let mut pending = self.pending.lock();

        if pending.events.is_empty() {
            return;
        }

        let elapsed = pending.last_flush.elapsed();
        let batch_window = Duration::from_millis(self.config.batch_window_ms);

        if !force && elapsed < batch_window {
            return;
        }

        // Check debounce - only include events that have settled
        let debounce = Duration::from_millis(self.config.debounce_ms);
        let now = Instant::now();

        let mut ready_items = Vec::new();
        let mut still_pending = HashMap::new();

        for (path, (work_type, first_seen)) in pending.events.drain() {
            if force || now.duration_since(first_seen) >= debounce {
                ready_items.push(WorkItem {
                    path,
                    work_type,
                    queued_at: first_seen,
                });
            } else {
                still_pending.insert(path, (work_type, first_seen));
            }
        }

        pending.events = still_pending;

        if !ready_items.is_empty() {
            pending.last_flush = Instant::now();
            drop(pending);

            debug!("Flushing {} work items", ready_items.len());

            // Separate deletes and indexes - deletes should be processed first
            let (deletes, indexes): (Vec<_>, Vec<_>) = ready_items
                .into_iter()
                .partition(|item| item.work_type == WorkType::Delete);

            // Send deletes first, then indexes
            if !deletes.is_empty() {
                if let Err(e) = self.work_tx.send(deletes) {
                    warn!("Failed to send delete work items: {e}");
                }
            }
            if !indexes.is_empty() {
                if let Err(e) = self.work_tx.send(indexes) {
                    warn!("Failed to send index work items: {e}");
                }
            }
        }
    }
}

/// Handle for submitting events to the scheduler
#[derive(Clone)]
pub struct EventHandle {
    pending: Arc<Mutex<PendingEvents>>,
    config: SchedulerConfig,
}

impl EventHandle {
    /// Submit a file event
    ///
    /// Events for the same path are merged according to these rules:
    /// - Delete always wins over Index (if a file is deleted, no point indexing it)
    /// - Later Index events replace earlier ones (file modified multiple times)
    pub fn submit(&self, path: PathBuf, work_type: WorkType) {
        let mut pending = self.pending.lock();

        // Backpressure: if queue is too large, drop oldest events
        if pending.events.len() >= self.config.max_queue_size {
            warn!(
                "Work queue full ({} items), dropping oldest events",
                pending.events.len()
            );
            // Remove 10% of oldest events
            let to_remove = self.config.max_queue_size / 10;
            let mut oldest: Vec<_> = pending
                .events
                .iter()
                .map(|(p, (_, t))| (p.clone(), *t))
                .collect();
            oldest.sort_by_key(|(_, time)| *time);
            for (path, _) in oldest.into_iter().take(to_remove) {
                pending.events.remove(&path);
            }
        }

        let now = Instant::now();

        let entry = pending.events.get(&path).cloned();
        match entry {
            Some((existing_type, first_seen)) => {
                // Merge rules:
                // - Delete always wins
                // - Otherwise, keep the newer work type but original timestamp
                let merged_type =
                    if existing_type == WorkType::Delete || work_type == WorkType::Delete {
                        WorkType::Delete
                    } else {
                        work_type
                    };
                pending.events.insert(path, (merged_type, first_seen));
            }
            None => {
                trace!("New work item: {:?} for {}", work_type, path.display());
                pending.events.insert(path, (work_type, now));
            }
        }
    }

    /// Submit multiple events at once
    pub fn submit_batch(&self, items: impl IntoIterator<Item = (PathBuf, WorkType)>) {
        for (path, work_type) in items {
            self.submit(path, work_type);
        }
    }

    /// Get the current queue size
    pub fn queue_size(&self) -> usize {
        self.pending.lock().events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_event_merging() {
        let scheduler = WorkScheduler::new(SchedulerConfig {
            debounce_ms: 10,
            batch_window_ms: 50,
            ..Default::default()
        });

        let handle = scheduler.event_handle();

        // Submit multiple events for the same file
        handle.submit(PathBuf::from("test.rs"), WorkType::Index);
        handle.submit(PathBuf::from("test.rs"), WorkType::Index);
        handle.submit(PathBuf::from("test.rs"), WorkType::Index);

        // Should only have one pending event
        assert_eq!(handle.queue_size(), 1);
    }

    #[test]
    fn test_delete_wins() {
        let scheduler = WorkScheduler::new(SchedulerConfig::default());
        let handle = scheduler.event_handle();

        handle.submit(PathBuf::from("test.rs"), WorkType::Index);
        handle.submit(PathBuf::from("test.rs"), WorkType::Delete);

        let pending = scheduler.pending.lock();
        let (work_type, _) = pending.events.get(&PathBuf::from("test.rs")).unwrap();
        assert_eq!(*work_type, WorkType::Delete);
    }

    #[test]
    fn test_batch_flush() {
        let scheduler = WorkScheduler::new(SchedulerConfig {
            debounce_ms: 1,
            batch_window_ms: 10,
            ..Default::default()
        });

        let handle = scheduler.event_handle();
        let rx = scheduler.work_receiver();

        // Submit events
        handle.submit(PathBuf::from("a.rs"), WorkType::Index);
        handle.submit(PathBuf::from("b.rs"), WorkType::Index);

        // Run scheduler in background
        let scheduler_clone = Arc::new(scheduler);
        let scheduler_ref = Arc::clone(&scheduler_clone);
        let scheduler_thread = thread::spawn(move || {
            scheduler_ref.run();
        });

        // Wait for batch to be flushed
        thread::sleep(Duration::from_millis(100));

        // Shutdown
        scheduler_clone.shutdown();
        scheduler_thread.join().unwrap();

        // Should have received work
        let batch = rx.try_recv().unwrap();
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_backpressure() {
        let scheduler = WorkScheduler::new(SchedulerConfig {
            max_queue_size: 10,
            ..Default::default()
        });

        let handle = scheduler.event_handle();

        // Submit more than max_queue_size events
        for i in 0..20 {
            handle.submit(PathBuf::from(format!("file{i}.rs")), WorkType::Index);
        }

        // Queue should be at or below max size
        assert!(handle.queue_size() <= 10);
    }
}
