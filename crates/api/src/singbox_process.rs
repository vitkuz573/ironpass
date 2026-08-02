//! Managed sing-box subprocess with health monitoring and restart backoff.

use crate::singbox::SingBoxConfig;
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

#[derive(Debug)]
pub struct SingBoxProcessManager {
    sing_box_path: Option<PathBuf>,
    child: Arc<Mutex<Option<Child>>>,
    start_time: Arc<Mutex<Option<Instant>>>,
    last_error: Arc<Mutex<Option<String>>>,
    restart_count: Arc<Mutex<usize>>,
}

impl SingBoxProcessManager {
    pub fn new() -> Self {
        Self {
            sing_box_path: None,
            child: Arc::new(Mutex::new(None)),
            start_time: Arc::new(Mutex::new(None)),
            last_error: Arc::new(Mutex::new(None)),
            restart_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self {
            sing_box_path: Some(path),
            child: Arc::new(Mutex::new(None)),
            start_time: Arc::new(Mutex::new(None)),
            last_error: Arc::new(Mutex::new(None)),
            restart_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.sing_box_path = Some(path);
    }

    /// Locate the sing-box binary. Uses configured path, then PATH.
    pub async fn locate_binary(&self) -> anyhow::Result<PathBuf> {
        if let Some(ref path) = self.sing_box_path {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        if let Ok(path) = which::global("sing-box") {
            return Ok(path);
        }

        // Fallback to common names/locations.
        for name in ["sing-box", "sing-box.exe", "sb"] {
            if let Ok(path) = which::global(name) {
                return Ok(path);
            }
        }

        anyhow::bail!("sing-box binary not found. Install sing-box or set sing_box_path in config.")
    }

    pub async fn start(&self, config: &SingBoxConfig) -> anyhow::Result<()> {
        let bin = self.locate_binary().await?;

        let temp_dir = std::env::temp_dir().join("ironpass");
        fs::create_dir_all(&temp_dir).await?;
        let config_path = temp_dir.join(format!("sing-box-{}.json", uuid::Uuid::new_v4()));
        fs::write(&config_path, &config.json).await?;

        info!("Starting sing-box with config: {}", config_path.display());

        let mut child = Command::new(&bin)
            .arg("run")
            .arg("-c")
            .arg(&config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        // Briefly wait to catch immediate startup failures.
        sleep(Duration::from_millis(300)).await;
        match child.try_wait()? {
            Some(status) => {
                let code = status.code().unwrap_or(-1);
                anyhow::bail!("sing-box exited immediately with code {}", code);
            }
            None => {}
        }

        let pid = child.id().unwrap_or(0);
        info!("sing-box started (pid {})", pid);

        *self.child.lock().await = Some(child);
        *self.start_time.lock().await = Some(Instant::now());
        *self.last_error.lock().await = None;
        *self.restart_count.lock().await = 0;

        // Spawn monitor task.
        let child_arc = Arc::clone(&self.child);
        let start_time_arc = Arc::clone(&self.start_time);
        let last_error_arc = Arc::clone(&self.last_error);
        let restart_count_arc = Arc::clone(&self.restart_count);
        let config_json = config.json.clone();
        let bin_clone = bin.clone();

        tokio::spawn(async move {
            monitor(
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
            info!("Stopping sing-box (pid {})", c.id().unwrap_or(0));
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
}

impl Default for SingBoxProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn monitor(
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
                        error!("Failed to wait for sing-box: {}", e);
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
        warn!("sing-box exited with code {:?}", code);
        *last_error_arc.lock().await = Some(format!("sing-box exited with code {:?}", code));

        let mut restart_count = restart_count_arc.lock().await;
        if *restart_count >= MAX_RESTART_ATTEMPTS {
            error!("sing-box exceeded maximum restart attempts; giving up");
            *start_time_arc.lock().await = None;
            break;
        }
        *restart_count += 1;
        let attempt = *restart_count;
        drop(restart_count);

        let backoff = BASE_BACKOFF.mul_f32(2.0f32.powi(attempt as i32 - 1));
        warn!(
            "Restarting sing-box in {:?} (attempt {}/{})",
            backoff, attempt, MAX_RESTART_ATTEMPTS
        );
        sleep(backoff).await;

        // Rewrite config (path is reused).
        if fs::write(&config_path, &config_json).await.is_err() {
            error!("Failed to rewrite sing-box config");
            break;
        }

        match Command::new(&bin)
            .arg("run")
            .arg("-c")
            .arg(&config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(mut child) => {
                sleep(Duration::from_millis(300)).await;
                if child.try_wait().ok().flatten().is_some() {
                    error!("Restarted sing-box exited immediately");
                    *last_error_arc.lock().await =
                        Some("Restarted sing-box exited immediately".into());
                    let mut guard = child_arc.lock().await;
                    *guard = None;
                    break;
                }
                info!("sing-box restarted (pid {})", child.id().unwrap_or(0));
                *start_time_arc.lock().await = Some(Instant::now());
                let mut guard = child_arc.lock().await;
                *guard = Some(child);
            }
            Err(e) => {
                error!("Failed to restart sing-box: {}", e);
                *last_error_arc.lock().await = Some(format!("Failed to restart sing-box: {}", e));
                break;
            }
        }
    }

    // Cleanup config file.
    let _ = fs::remove_file(&config_path).await;
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
