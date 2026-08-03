//! Managed proxy-core subprocess (sing-box or Xray-core) with health monitoring and restart backoff.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

const MAX_RESTART_ATTEMPTS: usize = 5;
const BASE_BACKOFF: Duration = Duration::from_secs(1);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Core type backing the proxy process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoreType {
    #[default]
    SingBox,
    Xray,
}

impl CoreType {
    fn binary_names(&self) -> &'static [&'static str] {
        match self {
            CoreType::SingBox => &["sing-box", "sing-box.exe", "sb"],
            CoreType::Xray => &["xray", "xray.exe"],
        }
    }

    fn run_args(&self, config_path: &std::path::Path) -> Vec<std::ffi::OsString> {
        match self {
            CoreType::SingBox => {
                vec!["run".into(), "-c".into(), config_path.as_os_str().into()]
            }
            CoreType::Xray => {
                vec![
                    "run".into(),
                    "-config".into(),
                    config_path.as_os_str().into(),
                ]
            }
        }
    }

    fn config_file_prefix(&self) -> &'static str {
        match self {
            CoreType::SingBox => "sing-box",
            CoreType::Xray => "xray",
        }
    }
}

#[derive(Debug)]
pub struct CoreProcessManager {
    core_type: CoreType,
    core_path: Option<PathBuf>,
    child: Arc<Mutex<Option<Child>>>,
    start_time: Arc<Mutex<Option<Instant>>>,
    last_error: Arc<Mutex<Option<String>>>,
    restart_count: Arc<Mutex<usize>>,
    // Tracks whether the manager believes the process is running. Separated
    // from the child mutex so `is_running()`/`status()` never block behind a
    // `wait().await` in the monitor task.
    running: Arc<Mutex<bool>>,
    // Cached PID of the running child. Updated independently of the child
    // mutex so `status()` can report it without blocking on the monitor wait.
    pid: Arc<Mutex<Option<u32>>>,
    // Cancellation token for the currently running monitor task. `stop()` uses
    // this to abort the monitor loop and wait for it to finish.
    monitor_token: Arc<Mutex<CancellationToken>>,
}

impl CoreProcessManager {
    pub fn new() -> Self {
        Self {
            core_type: CoreType::default(),
            core_path: None,
            child: Arc::new(Mutex::new(None)),
            start_time: Arc::new(Mutex::new(None)),
            last_error: Arc::new(Mutex::new(None)),
            restart_count: Arc::new(Mutex::new(0)),
            running: Arc::new(Mutex::new(false)),
            pid: Arc::new(Mutex::new(None)),
            monitor_token: Arc::new(Mutex::new(CancellationToken::new())),
        }
    }

    pub fn with_path(path: PathBuf, core_type: CoreType) -> Self {
        Self {
            core_type,
            core_path: Some(path),
            child: Arc::new(Mutex::new(None)),
            start_time: Arc::new(Mutex::new(None)),
            last_error: Arc::new(Mutex::new(None)),
            restart_count: Arc::new(Mutex::new(0)),
            running: Arc::new(Mutex::new(false)),
            pid: Arc::new(Mutex::new(None)),
            monitor_token: Arc::new(Mutex::new(CancellationToken::new())),
        }
    }

    pub fn set_core_type(&mut self, core_type: CoreType) {
        self.core_type = core_type;
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.core_path = Some(path);
    }

    pub fn core_type(&self) -> CoreType {
        self.core_type
    }

    /// Locate the core binary. Uses configured path, then PATH.
    pub async fn locate_binary(&self) -> anyhow::Result<PathBuf> {
        if let Some(ref path) = self.core_path
            && path.exists()
        {
            return Ok(path.clone());
        }

        for name in self.core_type.binary_names() {
            if let Ok(path) = which::global(name) {
                return Ok(path);
            }
        }

        anyhow::bail!(
            "{} binary not found. Install it or set the path via CLI/config.",
            match self.core_type {
                CoreType::SingBox => "sing-box",
                CoreType::Xray => "xray",
            }
        )
    }

    /// Check whether an Xray binary exists in PATH.
    ///
    /// Used by callers that need to validate Xray availability before building
    /// a configuration that requires it.
    pub fn which_xray_in_path() -> anyhow::Result<PathBuf> {
        for name in ["xray", "xray.exe"] {
            if let Ok(path) = which::global(name) {
                return Ok(path);
            }
        }
        anyhow::bail!("xray not found in PATH")
    }

    pub async fn start(&self, config_json: &str) -> anyhow::Result<()> {
        // Stop any existing process and monitor before spawning a new one.
        self.stop().await.ok();

        let bin = self.locate_binary().await?;

        let temp_dir = std::env::temp_dir().join("ironpass");
        fs::create_dir_all(&temp_dir).await?;
        let config_path = temp_dir.join(format!(
            "{}-{}.json",
            self.core_type.config_file_prefix(),
            uuid::Uuid::new_v4()
        ));
        fs::write(&config_path, config_json).await?;

        info!(
            "Starting {} with config: {}",
            match self.core_type {
                CoreType::SingBox => "sing-box",
                CoreType::Xray => "xray-core",
            },
            config_path.display()
        );

        let args = self.core_type.run_args(&config_path);
        let mut command = Command::new(&bin);
        command.args(args);
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        // Briefly wait to catch immediate startup failures.
        sleep(Duration::from_millis(300)).await;
        if let Some(status) = child.try_wait()? {
            let code = status.code().unwrap_or(-1);
            let _ = fs::remove_file(&config_path).await;
            anyhow::bail!("{} exited immediately with code {}", self.core_name(), code);
        }

        let pid = child.id();
        info!(
            "{} started (pid {})",
            self.core_name(),
            pid.unwrap_or(0)
        );

        *self.child.lock().await = Some(child);
        *self.start_time.lock().await = Some(Instant::now());
        *self.last_error.lock().await = None;
        *self.restart_count.lock().await = 0;
        *self.running.lock().await = true;
        *self.pid.lock().await = pid;

        // Spawn monitor task with a fresh cancellation token.
        let token = {
            let mut guard = self.monitor_token.lock().await;
            let new_token = CancellationToken::new();
            *guard = new_token.clone();
            new_token
        };
        let child_arc = Arc::clone(&self.child);
        let start_time_arc = Arc::clone(&self.start_time);
        let last_error_arc = Arc::clone(&self.last_error);
        let restart_count_arc = Arc::clone(&self.restart_count);
        let running_arc = Arc::clone(&self.running);
        let pid_arc = Arc::clone(&self.pid);
        let config_json = config_json.to_string();
        let bin_clone = bin.clone();
        let core_type = self.core_type;

        tokio::spawn(async move {
            monitor(
                core_type,
                token,
                child_arc,
                start_time_arc,
                last_error_arc,
                restart_count_arc,
                running_arc,
                pid_arc,
                config_json,
                config_path,
                bin_clone,
            )
            .await;
        });

        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        // Cancel the monitor task first so it cannot restart the child after we
        // kill it. We keep the child ownership inside the monitor; cancelling
        // makes the monitor drop its copy and exit instead of respawning.
        let old_token = {
            let mut guard = self.monitor_token.lock().await;
            let old = guard.clone();
            *guard = CancellationToken::new();
            old
        };

        // Take the child out of the mutex so we can kill and wait on it without
        // holding the lock across an await point.
        let child = {
            let mut child = self.child.lock().await;
            child.take()
        };

        // A running child implies a monitor task was spawned. Wait for it to
        // observe the cancellation and exit, but only up to STOP_TIMEOUT. If no
        // child is running then there is nothing monitoring the process, so
        // waiting for the token would hang forever.
        let had_child = child.is_some();

        if let Some(mut c) = child {
            info!(
                "Stopping {} (pid {})",
                self.core_name(),
                c.id().unwrap_or(0)
            );
            // `start_kill()` only sends SIGKILL; the process may be a shell
            // wrapper on some platforms. Use `kill()` and then wait so the
            // OS reaps the child and the port is released.
            if let Err(e) = c.start_kill() {
                warn!("Failed to send kill signal to {}: {}", self.core_name(), e);
            }
            tokio::time::timeout(STOP_TIMEOUT, c.wait())
                .await
                .ok()
                .transpose()
                .ok();
        }

        if had_child {
            // Wait for the old monitor task to observe the cancellation and exit.
            tokio::time::timeout(STOP_TIMEOUT, old_token.cancelled())
                .await
                .ok();
        }

        *self.running.lock().await = false;
        *self.pid.lock().await = None;
        *self.start_time.lock().await = None;
        *self.restart_count.lock().await = 0;
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }

    pub async fn status(&self) -> (Option<u32>, Option<u64>, Option<String>) {
        // Quick snapshot: grab cached PID and metadata under separate locks;
        // never block on the child mutex here because the monitor may hold the
        // child across a wait.
        let pid = *self.pid.lock().await;
        let start_time = *self.start_time.lock().await;
        let uptime = start_time.map(|t| t.elapsed().as_secs());
        let last_error = self.last_error.lock().await.clone();
        (pid, uptime, last_error)
    }

    fn core_name(&self) -> &'static str {
        match self.core_type {
            CoreType::SingBox => "sing-box",
            CoreType::Xray => "xray-core",
        }
    }
}

impl Default for CoreProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(200);

#[allow(clippy::too_many_arguments)]
async fn monitor(
    core_type: CoreType,
    token: CancellationToken,
    child_arc: Arc<Mutex<Option<Child>>>,
    start_time_arc: Arc<Mutex<Option<Instant>>>,
    last_error_arc: Arc<Mutex<Option<String>>>,
    restart_count_arc: Arc<Mutex<usize>>,
    running_arc: Arc<Mutex<bool>>,
    pid_arc: Arc<Mutex<Option<u32>>>,
    config_json: String,
    config_path: PathBuf,
    bin: PathBuf,
) {
    loop {
        // Exit immediately if stop was requested before this loop iteration.
        if token.is_cancelled() {
            *running_arc.lock().await = false;
            *pid_arc.lock().await = None;
            break;
        }

        // Poll the child while it remains in the shared mutex. This lets
        // `stop()` take ownership and kill it at any time; the monitor never
        // holds the child across a long await, preventing leaked processes.
        let exit_status = loop {
            if token.is_cancelled() {
                break None;
            }

            let mut guard = child_arc.lock().await;
            let maybe_status = match guard.as_mut() {
                Some(c) => c.try_wait(),
                None => break None,
            };

            match maybe_status {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    // Child still running. Release the lock and wait a bit.
                    drop(guard);
                    tokio::select! {
                        biased;
                        _ = token.cancelled() => break None,
                        _ = sleep(WAIT_POLL_INTERVAL) => {}
                    }
                }
                Err(e) => {
                    error!("Failed to poll {} status: {}", core_name(core_type), e);
                    break Some(std::process::ExitStatus::default());
                }
            }
        };

        if token.is_cancelled() {
            *running_arc.lock().await = false;
            *pid_arc.lock().await = None;
            break;
        }

        if exit_status.is_none() {
            // Manager was stopped or no child to monitor.
            *running_arc.lock().await = false;
            *pid_arc.lock().await = None;
            break;
        }

        let code = exit_status.as_ref().and_then(|s| s.code());
        warn!("{} exited with code {:?}", core_name(core_type), code);
        *last_error_arc.lock().await = Some(format!(
            "{} exited with code {:?}",
            core_name(core_type),
            code
        ));
        *pid_arc.lock().await = None;

        let mut restart_count = restart_count_arc.lock().await;
        if *restart_count >= MAX_RESTART_ATTEMPTS {
            error!(
                "{} exceeded maximum restart attempts; giving up",
                core_name(core_type)
            );
            *start_time_arc.lock().await = None;
            *running_arc.lock().await = false;
            break;
        }
        *restart_count += 1;
        let attempt = *restart_count;
        drop(restart_count);

        let backoff = BASE_BACKOFF.mul_f32(2.0f32.powi(attempt as i32 - 1));
        warn!(
            "Restarting {} in {:?} (attempt {}/{})",
            core_name(core_type),
            backoff,
            attempt,
            MAX_RESTART_ATTEMPTS
        );

        tokio::select! {
            biased;
            _ = token.cancelled() => {
                *running_arc.lock().await = false;
                *pid_arc.lock().await = None;
                break;
            }
            _ = sleep(backoff) => {}
        }

        // Rewrite config (path is reused).
        if fs::write(&config_path, &config_json).await.is_err() {
            error!("Failed to rewrite {} config", core_name(core_type));
            *running_arc.lock().await = false;
            *pid_arc.lock().await = None;
            break;
        }

        let args = core_type.run_args(&config_path);
        let mut command = Command::new(&bin);
        command.args(args);
        match command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(mut new_child) => {
                sleep(Duration::from_millis(300)).await;
                if new_child.try_wait().ok().flatten().is_some() {
                    error!("Restarted {} exited immediately", core_name(core_type));
                    *last_error_arc.lock().await = Some(format!(
                        "Restarted {} exited immediately",
                        core_name(core_type)
                    ));
                    let mut guard = child_arc.lock().await;
                    *guard = None;
                    *running_arc.lock().await = false;
                    *pid_arc.lock().await = None;
                    break;
                }
                let new_pid = new_child.id();
                info!(
                    "{} restarted (pid {})",
                    core_name(core_type),
                    new_pid.unwrap_or(0)
                );
                *start_time_arc.lock().await = Some(Instant::now());
                *pid_arc.lock().await = new_pid;
                let mut guard = child_arc.lock().await;
                *guard = Some(new_child);
            }
            Err(e) => {
                error!("Failed to restart {}: {}", core_name(core_type), e);
                *last_error_arc.lock().await =
                    Some(format!("Failed to restart {}: {}", core_name(core_type), e));
                *running_arc.lock().await = false;
                *pid_arc.lock().await = None;
                break;
            }
        }
    }

    // Cleanup config file.
    let _ = fs::remove_file(&config_path).await;
}

fn core_name(core_type: CoreType) -> &'static str {
    match core_type {
        CoreType::SingBox => "sing-box",
        CoreType::Xray => "xray-core",
    }
}

// Minimal `which` implementation to avoid extra dependency.
mod which {
    use std::path::PathBuf;

    pub fn global(name: &str) -> anyhow::Result<PathBuf> {
        let path_env = std::env::var_os("PATH").ok_or_else(|| anyhow::anyhow!("PATH not set"))?;
        for dir in std::env::split_paths(&path_env) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        anyhow::bail!("{} not found in PATH", name)
    }
}
