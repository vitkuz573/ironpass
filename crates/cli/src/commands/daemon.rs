use crate::args::DaemonAction;
use color_eyre::eyre;
use std::process::{Command, Stdio};

pub async fn handle(action: DaemonAction) -> eyre::Result<()> {
    match action {
        DaemonAction::Start => start_daemon().await,
        DaemonAction::Stop => stop_daemon().await,
        DaemonAction::Status => daemon_status().await,
    }
}

async fn start_daemon() -> eyre::Result<()> {
    if daemon_running().await {
        println!("ironpassd is already running");
        return Ok(());
    }

    let mut cmd = Command::new("ironpassd");
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn()?;
    println!("Started ironpassd (pid {})", child.id());

    // Wait briefly to confirm startup.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    if daemon_running().await {
        println!("ironpassd is healthy");
        Ok(())
    } else {
        let _ = child.kill();
        Err(eyre::eyre!("ironpassd failed to start"))
    }
}

async fn stop_daemon() -> eyre::Result<()> {
    if !daemon_running().await {
        println!("ironpassd is not running");
        return Ok(());
    }

    // Try graceful shutdown via API first.
    let client = reqwest::Client::new();
    let _ = client
        .post("http://127.0.0.1:8080/api/v1/proxy/stop")
        .send()
        .await;

    // Then kill the process.
    #[cfg(unix)]
    {
        let _ = Command::new("pkill").arg("-f").arg("ironpassd").status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill").args(["/F", "/IM", "ironpassd.exe"]).status();
    }

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    if daemon_running().await {
        Err(eyre::eyre!("Failed to stop ironpassd"))
    } else {
        println!("ironpassd stopped");
        Ok(())
    }
}

async fn daemon_status() -> eyre::Result<()> {
    if daemon_running().await {
        println!("ironpassd is running");
    } else {
        println!("ironpassd is not running");
    }
    Ok(())
}

async fn daemon_running() -> bool {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_default();
    client
        .get("http://127.0.0.1:8080/api/v1/health")
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
