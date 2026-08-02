use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

const TOKEN: &str = "test-token-42";
const REAL_URI: &str =
    "vless://550e8400-e29b-41d4-a716-446655440000@real.example.com:443?encryption=none#RealNode";
const PLACEHOLDER_URI: &str =
    "vless://00000000-0000-0000-0000-000000000000@0.0.0.0:1?encryption=none#Placeholder";

fn fetch_cmd(config_dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("ironpass").expect("ironpass binary exists");
    cmd.arg("--config").arg(config_dir.path());
    cmd
}

fn mock_url(server: &MockServer) -> String {
    format!("http://127.0.0.1:{}/sub/{}", server.address().port(), TOKEN)
}

fn plain_text(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_string(body)
        .insert_header("content-type", "text/plain; charset=utf-8")
}

#[tokio::test]
async fn fetch_retries_with_generated_hwid_when_server_returns_placeholders() {
    let server = MockServer::start().await;
    let url = mock_url(&server);

    Mock::given(method("GET"))
        .and(path(format!("/sub/{}", TOKEN)))
        .and(header_not_present("x-hwid"))
        .respond_with(plain_text(PLACEHOLDER_URI).insert_header("x-hwid-not-supported", "true"))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/sub/{}", TOKEN)))
        .and(header_non_empty("x-hwid"))
        .respond_with(plain_text(REAL_URI))
        .expect(1)
        .mount(&server)
        .await;

    let config_dir = TempDir::new().unwrap();

    fetch_cmd(&config_dir)
        .arg("fetch")
        .arg(&url)
        .arg("--format")
        .arg("raw")
        .assert()
        .success()
        .stdout(contains("real.example.com"))
        .stdout(contains("550e8400-e29b-41d4-a716-446655440000"))
        .stdout(contains("RealNode"))
        .stdout(contains("0.0.0.0").not());
}

#[tokio::test]
async fn fetch_explicit_hwid_sent_on_first_request() {
    let server = MockServer::start().await;
    let url = mock_url(&server);

    Mock::given(method("GET"))
        .and(path(format!("/sub/{}", TOKEN)))
        .and(header("x-hwid", "explicit-hwid-123"))
        .respond_with(plain_text(REAL_URI))
        .expect(1)
        .mount(&server)
        .await;

    let config_dir = TempDir::new().unwrap();

    fetch_cmd(&config_dir)
        .arg("fetch")
        .arg(&url)
        .arg("--format")
        .arg("raw")
        .arg("--hwid")
        .arg("explicit-hwid-123")
        .assert()
        .success()
        .stdout(contains("real.example.com"));
}

#[tokio::test]
async fn fetch_include_placeholders_preserves_placeholder_nodes() {
    let server = MockServer::start().await;
    let url = mock_url(&server);

    let body = format!("{}\n{}", PLACEHOLDER_URI, REAL_URI);

    Mock::given(method("GET"))
        .and(path(format!("/sub/{}", TOKEN)))
        .respond_with(plain_text(&body))
        .expect(1)
        .mount(&server)
        .await;

    let config_dir = TempDir::new().unwrap();

    fetch_cmd(&config_dir)
        .arg("fetch")
        .arg(&url)
        .arg("--format")
        .arg("raw")
        .arg("--include-placeholders")
        .assert()
        .success()
        .stdout(contains("Placeholder"))
        .stdout(contains("RealNode"));
}

#[tokio::test]
async fn fetch_hwid_limit_returns_nonzero_and_clear_error() {
    let server = MockServer::start().await;
    let url = mock_url(&server);

    Mock::given(method("GET"))
        .and(path(format!("/sub/{}", TOKEN)))
        .respond_with(plain_text(PLACEHOLDER_URI).insert_header("x-hwid-limit", "true"))
        .expect(1)
        .mount(&server)
        .await;

    let config_dir = TempDir::new().unwrap();

    fetch_cmd(&config_dir)
        .arg("fetch")
        .arg(&url)
        .arg("--format")
        .arg("raw")
        .assert()
        .failure()
        .stderr(contains("Device limit exceeded").or(contains("device limit")));
}

#[tokio::test]
async fn fetch_format_json_outputs_valid_json_with_real_nodes() {
    let server = MockServer::start().await;
    let url = mock_url(&server);

    Mock::given(method("GET"))
        .and(path(format!("/sub/{}", TOKEN)))
        .respond_with(plain_text(REAL_URI))
        .expect(1)
        .mount(&server)
        .await;

    let config_dir = TempDir::new().unwrap();

    let assert = fetch_cmd(&config_dir)
        .arg("fetch")
        .arg(&url)
        .arg("--format")
        .arg("json")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("fetch --format json should emit valid JSON");
    assert!(parsed.is_array());
    assert!(
        parsed
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.get("server") == Some(&serde_json::json!("real.example.com")))
    );
}

// wiremock does not expose a `header_not_present` matcher in the prelude,
// so provide a small helper that matches requests *without* a given header.
fn header_not_present(header_name: &str) -> HeaderNotPresent {
    HeaderNotPresent {
        name: header_name.to_string(),
    }
}

struct HeaderNotPresent {
    name: String,
}

impl wiremock::Match for HeaderNotPresent {
    fn matches(&self, request: &wiremock::Request) -> bool {
        !request.headers.contains_key(&self.name)
    }
}

fn header_non_empty(header_name: &str) -> HeaderNonEmpty {
    HeaderNonEmpty {
        name: header_name.to_string(),
    }
}

struct HeaderNonEmpty {
    name: String,
}

impl wiremock::Match for HeaderNonEmpty {
    fn matches(&self, request: &wiremock::Request) -> bool {
        request
            .headers
            .get(&self.name)
            .map(|v| !v.to_str().unwrap_or("").is_empty())
            .unwrap_or(false)
    }
}
