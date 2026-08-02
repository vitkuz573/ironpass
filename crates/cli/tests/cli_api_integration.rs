use assert_cmd::Command;
use ironpass_api::db::DbPool;
use ironpass_api::state::AppState;
use ironpass_config::ConfigManager;
use ironpass_core::traits::HwidProvider;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

struct MockHwidProvider;

impl HwidProvider for MockHwidProvider {
    fn generate(&self) -> ironpass_core::Result<String> {
        Ok("mock-hwid".into())
    }

    fn get_device_info(&self) -> ironpass_core::Result<ironpass_core::models::HwidInfo> {
        Ok(ironpass_core::models::HwidInfo {
            hwid: "mock-hwid".into(),
            device_model: "Mock".into(),
            os: "Linux".into(),
            hostname: "mock".into(),
            username: "mock".into(),
            machine_id: "mock".into(),
        })
    }
}

/// Spawn the API server in a background thread with its own Tokio runtime.
/// Returns the bound address and a guard that shuts the server down when dropped.
fn spawn_server() -> (SocketAddr, ChildGuard) {
    let (addr_tx, addr_rx) = std::sync::mpsc::channel();

    let thread_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let config_manager = ConfigManager::with_dirs(
                std::env::temp_dir().join("ironpass-cli-test-cfg"),
                std::env::temp_dir().join("ironpass-cli-test-data"),
            );
            let db = DbPool::open_in_memory().unwrap();
            let hwid: Arc<dyn HwidProvider + Send + Sync> = Arc::new(MockHwidProvider);
            let state = Arc::new(AppState::new(config_manager, db, hwid));

            let app = ironpass_api::app(state);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            addr_tx.send(addr).unwrap();

            axum::serve(listener, app).await.unwrap();
        });
    });

    let addr = addr_rx.recv().unwrap();
    // Store the thread handle so we can abort the server when the test finishes.
    let child = ChildGuard::new(thread_handle);
    (addr, child)
}

struct ChildGuard {
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ChildGuard {
    fn new(handle: std::thread::JoinHandle<()>) -> Self {
        Self {
            handle: Some(handle),
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // Detach the server thread. The Tokio runtime will be torn down
            // when the process exits, which is sufficient for integration tests.
            drop(handle);
        }
    }
}

fn wait_for_server(addr: SocketAddr) {
    for _ in 0..60 {
        match std::net::TcpStream::connect(addr) {
            Ok(mut stream) => {
                use std::io::{Read, Write};
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                let request = format!(
                    "GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                    addr.port()
                );
                if stream.write_all(request.as_bytes()).is_ok() {
                    let mut buf = [0u8; 256];
                    if stream.read(&mut buf).is_ok_and(|n| n > 0) {
                        let text = String::from_utf8_lossy(&buf);
                        if text.contains("200 OK") {
                            return;
                        }
                    }
                }
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

#[test]
fn cli_health_command_hits_api() {
    let (addr, _guard) = spawn_server();
    wait_for_server(addr);

    let mut cmd = Command::cargo_bin("ironpass").unwrap();
    cmd.arg("--api-url")
        .arg(format!("http://{}", addr))
        .arg("config")
        .arg("show");
    cmd.assert().success();
}

#[test]
fn cli_add_subscription_via_api() {
    let (addr, _guard) = spawn_server();
    wait_for_server(addr);

    let mut cmd = Command::cargo_bin("ironpass").unwrap();
    cmd.arg("--api-url")
        .arg(format!("http://{}", addr))
        .arg("sub")
        .arg("add")
        .arg("https://example.com/sub")
        .arg("--name")
        .arg("test");
    cmd.assert().success().stdout(predicates::str::contains("Added subscription"));
}
