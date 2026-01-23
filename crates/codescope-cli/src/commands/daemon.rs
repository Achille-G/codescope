//! `codescope daemon` command - background indexing daemon

use anyhow::{Context, Result};
use clap::Subcommand;
use codescope_core::{Project, ProjectLock};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum DaemonCommand {
    /// Start the daemon in background
    Start {
        /// Number of parallel jobs for indexing
        #[arg(short, long)]
        jobs: Option<usize>,

        /// Debounce time in milliseconds (default: 100)
        #[arg(long)]
        debounce_ms: Option<u64>,

        /// Poll interval in milliseconds for safety rescan (default: 60000)
        #[arg(long)]
        poll_interval_ms: Option<u64>,
    },

    /// Stop the running daemon
    Stop,

    /// Show daemon status
    Status,
}

/// Get the PID file path for the daemon
fn pid_file_path(project: &Project) -> PathBuf {
    project.codescope_dir().join("daemon.pid")
}

/// Get the log file path for the daemon
fn log_file_path(project: &Project) -> PathBuf {
    project.codescope_dir().join("daemon.log")
}

/// Read the PID from the PID file
fn read_pid(pid_file: &Path) -> Option<u32> {
    fs::read_to_string(pid_file).ok()?.trim().parse().ok()
}

/// Write PID to the PID file
fn write_pid(pid_file: &Path, pid: u32) -> Result<()> {
    let mut file = fs::File::create(pid_file)?;
    writeln!(file, "{pid}")?;
    Ok(())
}

/// Check if a process with the given PID is running
#[cfg(windows)]
fn is_process_running(pid: u32) -> bool {
    use std::process::Command;
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| {
            let output = String::from_utf8_lossy(&o.stdout);
            output.contains(&pid.to_string())
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_process_running(pid: u32) -> bool {
    use std::process::Command;
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Kill a process by PID
#[cfg(windows)]
fn kill_process(pid: u32) -> bool {
    Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn kill_process(pid: u32) -> bool {
    Command::new("kill")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn run(command: DaemonCommand) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let project = Project::find(&current_dir)
        .context("Not in a codescope project. Run 'codescope init' first.")?;

    match command {
        DaemonCommand::Start {
            jobs,
            debounce_ms,
            poll_interval_ms,
        } => run_start(&project, jobs, debounce_ms, poll_interval_ms),
        DaemonCommand::Stop => run_stop(&project),
        DaemonCommand::Status => run_status(&project),
    }
}

fn run_start(
    project: &Project,
    jobs: Option<usize>,
    debounce_ms: Option<u64>,
    poll_interval_ms: Option<u64>,
) -> Result<()> {
    let pid_file = pid_file_path(project);
    let log_file = log_file_path(project);
    let lock_path = project.lock_file_path();

    // Check if already running
    if let Some(pid) = read_pid(&pid_file) {
        if is_process_running(pid) {
            println!("Daemon is already running (PID {pid})");
            return Ok(());
        }
        // Clean up stale PID file
        let _ = fs::remove_file(&pid_file);
    }

    // Check if lock is held
    if ProjectLock::is_locked(&lock_path) {
        let pid = ProjectLock::read_holder_pid(&lock_path)
            .map(|p| format!(" (PID {p})"))
            .unwrap_or_default();
        println!("Another codescope process is already running{pid}. Cannot start daemon.");
        return Ok(());
    }

    // Get current executable path
    let exe = env::current_exe().context("Failed to get current executable path")?;

    // Build arguments for watch command
    let mut args = vec!["watch".to_string()];
    if let Some(j) = jobs {
        args.push("--jobs".to_string());
        args.push(j.to_string());
    }
    if let Some(d) = debounce_ms {
        args.push("--debounce-ms".to_string());
        args.push(d.to_string());
    }
    if let Some(p) = poll_interval_ms {
        args.push("--poll-interval-ms".to_string());
        args.push(p.to_string());
    }

    // Open log file for output
    let log = fs::File::create(&log_file).context("Failed to create log file")?;

    // Spawn the process
    #[cfg(windows)]
    let child = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;

        Command::new(&exe)
            .args(&args)
            .current_dir(project.root())
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .context("Failed to spawn daemon process")?
    };

    #[cfg(unix)]
    let child = {
        use std::os::unix::process::CommandExt;

        // Fork and detach from parent
        unsafe {
            Command::new(&exe)
                .args(&args)
                .current_dir(project.root())
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .pre_exec(|| {
                    // Create new session to detach from parent
                    libc::setsid();
                    Ok(())
                })
                .spawn()
                .context("Failed to spawn daemon process")?
        }
    };

    // Write PID file
    write_pid(&pid_file, child.id())?;

    let child_id = child.id();
    info!("Started daemon with PID {child_id}");
    println!("Daemon started (PID {child_id})");
    println!("Log file: {}", log_file.display());

    Ok(())
}

fn run_stop(project: &Project) -> Result<()> {
    let pid_file = pid_file_path(project);
    let lock_path = project.lock_file_path();

    // Try to read PID from file
    let pid = read_pid(&pid_file);

    // If no PID file, check lock
    let pid = match pid {
        Some(p) => Some(p),
        None => ProjectLock::read_holder_pid(&lock_path),
    };

    match pid {
        Some(pid) => {
            if is_process_running(pid) {
                println!("Stopping daemon (PID {pid})...");
                if kill_process(pid) {
                    // Wait a bit for process to exit
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    // Clean up files
                    let _ = fs::remove_file(&pid_file);

                    println!("Daemon stopped");
                } else {
                    println!("Failed to stop daemon. You may need to kill it manually.");
                }
            } else {
                println!("Daemon is not running (stale PID file)");
                let _ = fs::remove_file(&pid_file);
            }
        }
        None => {
            if ProjectLock::is_locked(&lock_path) {
                println!("A codescope process is running but no PID file found.");
                println!("You may need to stop it manually or delete the lock file:");
                println!("  {}", lock_path.display());
            } else {
                println!("Daemon is not running");
            }
        }
    }

    Ok(())
}

fn run_status(project: &Project) -> Result<()> {
    let pid_file = pid_file_path(project);
    let log_file = log_file_path(project);
    let lock_path = project.lock_file_path();

    println!("Daemon status");
    println!("=============");
    println!();

    // Check PID file
    let pid = read_pid(&pid_file);
    let running = match pid {
        Some(pid) => {
            if is_process_running(pid) {
                println!("Status:   running");
                println!("PID:      {pid}");
                true
            } else {
                println!("Status:   not running (stale PID file)");
                false
            }
        }
        None => {
            // Check lock as fallback
            if ProjectLock::is_locked(&lock_path) {
                let pid = ProjectLock::read_holder_pid(&lock_path);
                println!("Status:   running (started via 'codescope watch')");
                if let Some(p) = pid {
                    println!("PID:      {p}");
                }
                true
            } else {
                println!("Status:   not running");
                false
            }
        }
    };

    println!("PID file: {}", pid_file.display());
    println!("Log file: {}", log_file.display());
    println!("Lock:     {}", lock_path.display());

    // Show recent log entries if running
    if running && log_file.exists() {
        println!();
        println!("Recent log entries:");
        if let Ok(content) = fs::read_to_string(&log_file) {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(5);
            for line in &lines[start..] {
                println!("  {line}");
            }
        }
    }

    Ok(())
}
