//! Managed proxy-core subprocess (sing-box or Xray-core) with health monitoring and restart backoff.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{error, info, warn};

const MAX_RESTART_ATTEMPTS: usize = 5;
const BASE_BACKOFF: Duration = Duration::from_secs(1);

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

    pub async fn start(&self, config_json: &str) -> anyhow::Result<()> {
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
            anyhow::bail!("{} exited immediately with code {}", self.core_name(), code);
        }

        let pid = child.id().unwrap_or(0);
        info!("{} started (pid {})", self.core_name(), pid);

        *self.child.lock().await = Some(child);
        *self.start_time.lock().await = Some(Instant::now());
        *self.last_error.lock().await = None;
        *self.restart_count.lock().await = 0;

        // Spawn monitor task.
        let child_arc = Arc::clone(&self.child);
        let start_time_arc = Arc::clone(&self.start_time);
        let last_error_arc = Arc::clone(&self.last_error);
        let restart_count_arc = Arc::clone(&self.restart_count);
        let config_json = config_json.to_string();
        let bin_clone = bin.clone();
        let core_type = self.core_type;

        tokio::spawn(async move {
            monitor(
                core_type,
                child_arc,
                start_time_arc,
                last_error_arc,
                restart_count_arc,
                config_json,
                config_path,
                bin_clone,
            )
            .await;
        });

        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        let mut child = self.child.lock().await;
        if let Some(ref mut c) = *child {
            info!(
                "Stopping {} (pid {})",
                self.core_name(),
                c.id().unwrap_or(0)
            );
            let _ = c.start_kill();
            let _ = c.wait().await;
        }
        *child = None;
        *self.start_time.lock().await = None;
        *self.restart_count.lock().await = 0;
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        let mut child = self.child.lock().await;
        if let Some(ref mut c) = *child {
            match c.try_wait() {
                Ok(None) => return true,
                Ok(Some(_)) => {
                    *child = None;
                    return false;
                }
                Err(_) => {
                    *child = None;
                    return false;
                }
            }
        }
        false
    }

    pub async fn status(&self) -> (Option<u32>, Option<u64>, Option<String>) {
        let child = self.child.lock().await;
        let pid = child.as_ref().and_then(|c| c.id());
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

#[allow(clippy::too_many_arguments)]
async fn monitor(
    core_type: CoreType,
    child_arc: Arc<Mutex<Option<Child>>>,
    start_time_arc: Arc<Mutex<Option<Instant>>>,
    last_error_arc: Arc<Mutex<Option<String>>>,
    restart_count_arc: Arc<Mutex<usize>>,
    config_json: String,
    config_path: PathBuf,
    bin: PathBuf,
) {
    loop {
        let exit_status = {
            let mut child_guard = child_arc.lock().await;
            if let Some(ref mut child) = *child_guard {
                match child.wait().await {
                    Ok(status) => Some(status),
                    Err(e) => {
                        error!("Failed to wait for {}: {}", core_name(core_type), e);
                        Some(std::process::ExitStatus::default())
                    }
                }
            } else {
                None
            }
        };

        if exit_status.is_none() {
            // Manager was stopped.
            break;
        }

        let code = exit_status.as_ref().and_then(|s| s.code());
        warn!("{} exited with code {:?}", core_name(core_type), code);
        *last_error_arc.lock().await = Some(format!(
            "{} exited with code {:?}",
            core_name(core_type),
            code
        ));

        let mut restart_count = restart_count_arc.lock().await;
        if *restart_count >= MAX_RESTART_ATTEMPTS {
            error!(
                "{} exceeded maximum restart attempts; giving up",
                core_name(core_type)
            );
            *start_time_arc.lock().await = None;
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
        sleep(backoff).await;

        // Rewrite config (path is reused).
        if fs::write(&config_path, &config_json).await.is_err() {
            error!("Failed to rewrite {} config", core_name(core_type));
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
            Ok(mut child) => {
                sleep(Duration::from_millis(300)).await;
                if child.try_wait().ok().flatten().is_some() {
                    error!("Restarted {} exited immediately", core_name(core_type));
                    *last_error_arc.lock().await = Some(format!(
                        "Restarted {} exited immediately",
                        core_name(core_type)
                    ));
                    let mut guard = child_arc.lock().await;
                    *guard = None;
                    break;
                }
                info!(
                    "{} restarted (pid {})",
                    core_name(core_type),
                    child.id().unwrap_or(0)
                );
                *start_time_arc.lock().await = Some(Instant::now());
                let mut guard = child_arc.lock().await;
                *guard = Some(child);
            }
            Err(e) => {
                error!("Failed to restart {}: {}", core_name(core_type), e);
                *last_error_arc.lock().await =
                    Some(format!("Failed to restart {}: {}", core_name(core_type), e));
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
