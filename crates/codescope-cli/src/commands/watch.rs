//! `codescope watch` command - continuous indexing

use anyhow::{Context, Result};
use codescope_core::{cleanup_stale_lock, PathFilter, Project, ProjectLock};
use codescope_parser::Language;
use indicatif::{ProgressBar, ProgressStyle};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::commands::util::{
    build_extension_set, build_walker_config, collect_indexable_files, extension_allowed,
};
use crate::services::{
    EventHandle, IndexProgress, IndexService, SchedulerConfig, WorkItem, WorkScheduler, WorkType,
};

/// Run the watch command
pub fn run(
    jobs: Option<usize>,
    debounce_ms: Option<u64>,
    poll_interval_ms: Option<u64>,
    no_semantic: bool,
) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    // Check for stale lock and clean up if needed
    let lock_path = project.lock_file_path();
    cleanup_stale_lock(&lock_path);

    // Acquire exclusive lock
    let _lock = ProjectLock::try_acquire(&lock_path).map_err(|e| {
        anyhow::anyhow!(
            "{}\n\nHint: If no other codescope process is running, delete {} and retry.",
            e,
            lock_path.display()
        )
    })?;

    info!("Starting watch mode for {}", project.root().display());

    // Initial index
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    spinner.set_message("Initializing index service...");

    let mut index_service = IndexService::new(project.clone())?;

    if no_semantic {
        index_service.disable_embeddings();
    }

    // Do initial scan
    spinner.set_message("Scanning files for initial index...");
    let files = collect_indexable_files(&project)?;

    spinner.set_message("Detecting changes...");
    let changes = index_service.detect_changes(&files)?;

    if !changes.is_empty() {
        spinner.finish_with_message(format!(
            "Initial index: {} added, {} modified, {} deleted",
            changes.added.len(),
            changes.modified.len(),
            changes.deleted.len()
        ));

        let progress_cb = make_progress_callback();
        let stats = index_service.process_changes(
            &changes.added,
            &changes.modified,
            &changes.deleted,
            jobs,
            Some(&*progress_cb),
        )?;

        // Resolve call sites
        index_service.resolve_call_sites(Some(&*progress_cb))?;

        spinner.finish_with_message(format!(
            "Initial index complete: {} files, {} chunks",
            stats.indexed_files, stats.indexed_chunks
        ));
    } else {
        spinner.finish_with_message("Index is up to date");
    }

    // Set up scheduler
    let scheduler_config = SchedulerConfig {
        debounce_ms: debounce_ms.unwrap_or(100),
        batch_window_ms: 500,
        poll_interval_ms: poll_interval_ms.unwrap_or(60_000),
        ..Default::default()
    };

    let scheduler = Arc::new(WorkScheduler::new(scheduler_config.clone()));
    let event_handle = scheduler.event_handle();
    let work_rx = scheduler.work_receiver();

    // Set up file watcher
    let watcher_handle = event_handle.clone();
    let project_root = project.root().to_path_buf();
    let codescope_dir = project.codescope_dir();

    let walker_config = build_walker_config(&project);
    let path_filter = Arc::new(PathFilter::new(project_root.clone(), walker_config)?);
    let allowed_extensions =
        build_extension_set(&project.config().indexing.include_extensions).map(Arc::new);

    let mut watcher = create_watcher(
        watcher_handle,
        codescope_dir.clone(),
        Arc::clone(&path_filter),
        allowed_extensions.clone(),
    )?;

    // Watch the project root recursively
    watcher
        .watch(&project_root, RecursiveMode::Recursive)
        .context("Failed to watch project directory")?;

    info!("Watching {} for changes", project_root.display());

    // Statistics
    let indexed_count = Arc::new(AtomicUsize::new(0));
    let deleted_count = Arc::new(AtomicUsize::new(0));
    let running = Arc::new(AtomicBool::new(true));

    // Set up Ctrl+C handler
    let running_clone = Arc::clone(&running);
    let scheduler_clone = Arc::clone(&scheduler);
    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        running_clone.store(false, Ordering::SeqCst);
        scheduler_clone.shutdown();
    })
    .context("Failed to set Ctrl+C handler")?;

    // Start scheduler thread
    let scheduler_thread_handle = {
        let scheduler = Arc::clone(&scheduler);
        std::thread::spawn(move || {
            scheduler.run();
        })
    };

    // Main processing loop
    let status_bar = ProgressBar::new_spinner();
    status_bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    status_bar.enable_steady_tick(Duration::from_millis(100));

    let mut last_poll = Instant::now();
    let poll_interval = Duration::from_millis(scheduler_config.poll_interval_ms);

    while running.load(Ordering::SeqCst) {
        // Update status
        let queue_size = event_handle.queue_size();
        status_bar.set_message(format!(
            "Watching... indexed={}, deleted={}, pending={}",
            indexed_count.load(Ordering::Relaxed),
            deleted_count.load(Ordering::Relaxed),
            queue_size
        ));

        // Process work batches
        match work_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(batch) => {
                process_batch(
                    &mut index_service,
                    batch,
                    jobs,
                    &indexed_count,
                    &deleted_count,
                )?;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // No work, check if we need to do a poll rescan
                if scheduler_config.poll_interval_ms > 0 && last_poll.elapsed() >= poll_interval {
                    debug!("Running periodic rescan");
                    do_poll_rescan(&project, &mut index_service, &event_handle)?;
                    last_poll = Instant::now();
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    status_bar.finish_with_message("Watch mode stopped");

    // Wait for scheduler thread to finish
    let _ = scheduler_thread_handle.join();

    println!(
        "Total: {} files indexed, {} files deleted",
        indexed_count.load(Ordering::Relaxed),
        deleted_count.load(Ordering::Relaxed)
    );

    Ok(())
}

fn create_watcher(
    event_handle: EventHandle,
    codescope_dir: PathBuf,
    path_filter: Arc<PathFilter>,
    allowed_extensions: Option<Arc<HashSet<String>>>,
) -> Result<RecommendedWatcher> {
    let watcher = RecommendedWatcher::new(
        move |result: Result<Event, notify::Error>| match result {
            Ok(event) => {
                handle_fs_event(
                    &event_handle,
                    &event,
                    &codescope_dir,
                    &path_filter,
                    allowed_extensions.as_deref(),
                );
            }
            Err(e) => {
                warn!("Watch error: {e}");
            }
        },
        Config::default().with_poll_interval(Duration::from_secs(2)),
    )
    .context("Failed to create file watcher")?;

    Ok(watcher)
}

fn handle_fs_event(
    handle: &EventHandle,
    event: &Event,
    codescope_dir: &Path,
    path_filter: &PathFilter,
    allowed_extensions: Option<&HashSet<String>>,
) {
    use notify::EventKind;

    // Filter out events from .codescope directory
    let paths: Vec<&PathBuf> = event
        .paths
        .iter()
        .filter(|p| !p.starts_with(codescope_dir))
        .collect();

    if paths.is_empty() {
        return;
    }

    let work_type = match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => WorkType::Index,
        EventKind::Remove(_) => WorkType::Delete,
        EventKind::Any | EventKind::Access(_) | EventKind::Other => return,
    };

    for path in paths {
        // Skip directories and non-supported files
        if path.is_dir() {
            continue;
        }

        // Check if file has a supported extension
        if !has_supported_extension(path) {
            continue;
        }

        if let Some(allowed) = allowed_extensions {
            if !extension_allowed(path, allowed) {
                continue;
            }
        }

        // Check if file should be ignored
        if path_filter.is_ignored(path) {
            continue;
        }

        debug!("File event: {:?} {:?}", work_type, path);
        handle.submit(path.clone(), work_type);
    }
}

fn has_supported_extension(path: &Path) -> bool {
    Language::from_path(path).is_some()
}

/// Create a progress callback that prints to stdout
fn make_progress_callback() -> Box<dyn Fn(&IndexProgress) + Send + Sync> {
    Box::new(|progress: &IndexProgress| match progress {
        IndexProgress::Starting { total_files } => {
            println!("Processing {total_files} files...");
        }
        IndexProgress::FileIndexed {
            path,
            chunks,
            vectors,
        } => {
            if *vectors > 0 {
                println!("  Indexed {path} ({chunks} chunks, {vectors} vectors)");
            } else {
                println!("  Indexed {path} ({chunks} chunks)");
            }
        }
        IndexProgress::FileSkipped { path } => {
            println!("  Skipped {path}");
        }
        IndexProgress::FileFailed { path, error } => {
            eprintln!("  Failed {path}: {error}");
        }
        IndexProgress::FileDeleted { path } => {
            println!("  Deleted {path}");
        }
        IndexProgress::ResolvingCalls { total } => {
            println!("Resolving call sites for {total} files...");
        }
    })
}

fn process_batch(
    index_service: &mut IndexService,
    batch: Vec<WorkItem>,
    jobs: Option<usize>,
    indexed_count: &AtomicUsize,
    deleted_count: &AtomicUsize,
) -> Result<()> {
    // Calculate average queue latency for monitoring
    if !batch.is_empty() {
        let now = Instant::now();
        let total_latency_ms: u128 = batch
            .iter()
            .map(|item| now.duration_since(item.queued_at).as_millis())
            .sum();
        let avg_latency_ms = total_latency_ms / batch.len() as u128;
        debug!(
            "Processing {} items (avg queue latency: {}ms)",
            batch.len(),
            avg_latency_ms
        );
    }

    let (to_delete, to_index): (Vec<_>, Vec<_>) = batch
        .into_iter()
        .partition(|item| item.work_type == WorkType::Delete);

    let progress_cb = make_progress_callback();

    // Process deletions
    if !to_delete.is_empty() {
        let delete_paths: Vec<PathBuf> = to_delete.into_iter().map(|item| item.path).collect();
        let stats = index_service.delete_files(&delete_paths, Some(&*progress_cb))?;
        deleted_count.fetch_add(stats.deleted_files, Ordering::Relaxed);
        debug!("Deleted {} files", stats.deleted_files);
    }

    // Process additions/modifications
    if !to_index.is_empty() {
        // Filter out files that no longer exist
        let index_paths: Vec<PathBuf> = to_index
            .into_iter()
            .filter(|item| item.path.exists())
            .map(|item| item.path)
            .collect();

        if !index_paths.is_empty() {
            let stats = index_service.index_files(&index_paths, jobs, Some(&*progress_cb))?;
            indexed_count.fetch_add(stats.indexed_files, Ordering::Relaxed);
            debug!(
                "Indexed {} files ({} chunks)",
                stats.indexed_files, stats.indexed_chunks
            );

            // Resolve call sites after indexing
            if stats.indexed_files > 0 {
                let _ = index_service.resolve_call_sites(Some(&*progress_cb));
            }
        }
    }

    Ok(())
}

fn do_poll_rescan(
    project: &Project,
    index_service: &mut IndexService,
    event_handle: &EventHandle,
) -> Result<()> {
    let files = collect_indexable_files(project)?;
    let changes = index_service.detect_changes(&files)?;

    if changes.is_empty() {
        return Ok(());
    }

    debug!(
        "Poll rescan found {} added, {} modified, {} deleted",
        changes.added.len(),
        changes.modified.len(),
        changes.deleted.len()
    );

    // Submit changes to the scheduler using batch submission
    let items = changes
        .added
        .iter()
        .chain(changes.modified.iter())
        .map(|p| (p.clone(), WorkType::Index))
        .chain(
            changes
                .deleted
                .iter()
                .map(|p| (p.clone(), WorkType::Delete)),
        );

    event_handle.submit_batch(items);

    Ok(())
}
