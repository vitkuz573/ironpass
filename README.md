<!-- generated-by: gsd-doc-writer -->

# IronPass

IronPass is an enterprise-grade, open-source **VPN client** for the terminal. It fetches subscription links, parses multiple proxy formats (VLESS, VMess, Trojan, Shadowsocks, Clash, sing-box), filters placeholder nodes, converts between output formats, and — most importantly — **connects to those servers directly** through a local SOCKS5/HTTP proxy with HWID binding support.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Supported Input and Output Formats](#supported-input-and-output-formats)
- [HWID Binding](#hwid-binding)
- [Configuration File](#configuration-file)
- [CLI Reference](#cli-reference)
- [Development and Testing](#development-and-testing)
- [Security and Privacy](#security-and-privacy)
- [License](#license)

## Features

- **Full VPN client**: Start a local SOCKS5/HTTP proxy and route your traffic through any parsed node — not just convert configs.
- **Multi-format parsing**: VLESS, VMess, Trojan, Shadowsocks, Clash YAML, sing-box JSON, Base64-encoded URI lists.
- **Format conversion**: Export subscriptions to Clash, sing-box, V2Ray base64, raw URIs, JSON, or a terminal table.
- **HWID-aware fetching**: Automatic retry with a generated Hardware ID when providers respond with placeholder nodes.
- **Placeholder detection**: Configurable scoring policy distinguishes real nodes from provider sentinels.
- **Subscription metadata**: Extracts `profile-title`, `profile-update-interval`, `profile-web-page-url`, `announce`, and `subscription-userinfo` traffic data.
- **Shell completions**: Generate completions for bash, zsh, fish, PowerShell, and Elvish.

## Installation

### Prerequisites

- Rust **1.85 or newer** (the workspace declares `rust-version = "1.85"`).
- A working C toolchain for native dependencies (rustls, rusqlite).

### Install from source

```bash
git clone https://github.com/example/ironpass.git
cd ironpass
cargo install --path crates/cli
```

The binary is installed as `ironpass`. Make sure `~/.cargo/bin` is on your `PATH`.

### Verify the installation

```bash
ironpass --version
```

## Quick Start

### 1. Fetch a subscription

```bash
ironpass fetch "https://example.com/sub/TOKEN"
```

If the provider requires a HWID, IronPass automatically generates one and retries.

### 2. Save a subscription for later

```bash
ironpass sub add "https://example.com/sub/TOKEN" --name "My Subscription"
```

### 3. Update saved subscriptions

```bash
ironpass sub update
```

This re-fetches every active subscription and refreshes the local node cache.

### 4. Connect through a node

```bash
# Start a local SOCKS5 proxy on port 1080 and HTTP proxy on port 8080
ironpass proxy "https://example.com/sub/TOKEN" --node 0 --socks-port 1080 --http-port 8080
```

Then route your traffic through it:

```bash
curl -x socks5h://127.0.0.1:1080 https://httpbin.org/ip
```

### 5. Export to a client format

```bash
ironpass export "https://example.com/sub/TOKEN" --target singbox --output singbox.json
```

## Supported Input and Output Formats

### Input formats (auto-detected)

| Format              | Description                                                    |
|---------------------|----------------------------------------------------------------|
| Raw URI list        | One `vless://`, `vmess://`, `trojan://`, or `ss://` per line   |
| Base64 URI list     | Base64-encoded raw URI list                                    |
| Clash YAML          | Clash / Clash Meta configuration with a `proxies:` section     |
| sing-box JSON       | sing-box configuration with an `outbounds` array               |

### Output formats

| Format      | CLI flag / target           | Typical client                       |
|-------------|-----------------------------|--------------------------------------|
| Table       | `--format table`            | Terminal preview                     |
| JSON        | `--format json`             | Scripting, inspection                |
| Raw URIs    | `--format raw`              | Generic share format                 |
| V2Ray       | `--format v2ray`            | V2RayN, V2RayNG, NekoRay             |
| Clash       | `--format clash`            | Clash, Clash Meta / mihomo           |
| sing-box    | `--format singbox`          | sing-box, Hiddify                    |
| Surge       | `--target surge`            | Surge for macOS/iOS                  |
| QuantumultX | `--target quantumult`       | Quantumult X                         |
| Loon        | `--target loone`            | Loon                                 |
| Shadowrocket| `--target shadowrocket`     | Shadowrocket                         |

## HWID Binding

Some providers require a Hardware ID to return real nodes. IronPass handles this in three ways:

1. **Automatic HWID**: If no HWID is supplied and the server returns only placeholder nodes, IronPass generates a device-bound HWID and retries.
2. **Per-subscription HWID**: Save a HWID together with a subscription:
   ```bash
   ironpass sub add "https://example.com/sub/TOKEN" --hwid "my-hwid-value"
   ```
3. **Manual override**: Pass a HWID on the command line:
   ```bash
   ironpass fetch "https://example.com/sub/TOKEN" --hwid "my-hwid-value"
   ```

### Managing HWID

```bash
ironpass hwid show          # Show the current HWID
ironpass hwid info          # Show device information
ironpass hwid regenerate    # Regenerate the stored HWID
ironpass hwid set VALUE     # Pin a custom HWID value
```

The HWID is derived from stable device attributes (hostname, username, machine ID, device model) and persisted in the configuration directory.

## Configuration File

IronPass is designed to be a self-contained VPN client. It stores configuration in the XDG config directory (e.g. `~/.config/ironpass/config.toml`) and subscription state in the XDG data directory (e.g. `~/.local/share/ironpass/subscriptions.json`). You do not need a separate GUI client to actually use the nodes — IronPass starts the proxy itself.

Run `ironpass config paths` to see the exact paths on your system.

### Example `config.toml`

```toml
[general]
user_agent = "v2rayN/6.0"
timeout_secs = 30
max_retries = 3

[subscription]
default_url = "https://example.com/sub/TOKEN"
auto_update = true
update_interval_hours = 24
proxy = "http://127.0.0.1:8080"
extra_headers = { "X-Custom-Header" = "value" }

[hwid]
enabled = true
custom_id = ""
device_model_override = ""

[output]
format = "clash"
output_file = ""
pretty = true
sort_by = ""

[logging]
level = "info"
file = true
log_dir = ""
```

### Config keys reference

| Section        | Key                     | Default         | Description                                              |
|----------------|-------------------------|-----------------|----------------------------------------------------------|
| `general`      | `user_agent`            | `v2rayN/6.0`    | User-Agent for HTTP requests                             |
| `general`      | `timeout_secs`          | `30`            | Request timeout in seconds                               |
| `general`      | `max_retries`           | `3`             | Maximum HTTP retry attempts                              |
| `subscription` | `default_url`           | none            | Subscription URL used when none is provided              |
| `subscription` | `auto_update`           | `true`          | Whether to update saved subscriptions automatically      |
| `subscription` | `update_interval_hours` | `24`            | Minimum interval between auto-updates                    |
| `subscription` | `proxy`                 | none            | Upstream HTTP proxy for requests                         |
| `subscription` | `extra_headers`         | `{}`            | Extra headers sent with every request                    |
| `hwid`         | `enabled`               | `true`          | Allow HWID generation and injection                      |
| `hwid`         | `custom_id`             | none            | Override the generated HWID                              |
| `hwid`         | `device_model_override` | none            | Override reported device model                           |
| `output`       | `format`                | `clash`         | Default export format                                    |
| `output`       | `pretty`                | `true`          | Pretty-print JSON/YAML output                            |
| `output`       | `sort_by`               | none            | Default sort field (`name`, `server`, `port`, `protocol`)|
| `logging`      | `level`                 | `info`          | Log level (`error`, `warn`, `info`, `debug`, `trace`)    |
| `logging`      | `file`                  | `true`          | Write logs to a file                                     |

Change a value at runtime:

```bash
ironpass config set general.timeout_secs 60
```

## CLI Reference

### Global options

| Flag              | Description                                   |
|-------------------|-----------------------------------------------|
| `--config PATH`   | Use an alternate configuration directory      |
| `-v, --verbose`   | Enable debug logging                          |
| `--quiet`         | Only log errors                               |
| `--json`          | Output JSON where applicable                  |

### `ironpass fetch [URL]`

Fetch and display subscription nodes.

| Flag                       | Description                                              |
|----------------------------|----------------------------------------------------------|
| `--format FORMAT`          | Output format (`table`, `json`, `raw`, `v2ray`, `clash`, `singbox`) |
| `--output PATH`            | Write output to a file                                   |
| `--hwid VALUE`             | Send a specific HWID on the first request                |
| `--include-placeholders`   | Do not filter placeholder nodes                          |
| `--sort FIELD`             | Sort by `name`, `server`, `port`, or `protocol`          |

Examples:

```bash
ironpass fetch "https://example.com/sub/TOKEN"
ironpass fetch --format json --sort name
ironpass fetch "https://example.com/sub/TOKEN" --format clash --output clash.yaml
```

### `ironpass sub <action>`

Manage saved subscriptions.

| Action                          | Description                                         |
|---------------------------------|-----------------------------------------------------|
| `add URL [--name NAME] [--hwid VALUE]` | Add a subscription                         |
| `remove TARGET`                 | Remove by URL or name                               |
| `list [--detailed]`             | List saved subscriptions                            |
| `update [TARGET] [--hwid VALUE]`| Re-fetch one or all subscriptions                   |

### `ironpass hwid <action>`

| Action           | Description                |
|------------------|----------------------------|
| `show`           | Show current HWID          |
| `info`           | Show detailed device info  |
| `regenerate`     | Regenerate stored HWID     |
| `set VALUE`      | Set a custom HWID          |

### `ironpass convert [INPUT] --to FORMAT`

Convert a local subscription file between formats. Reads from stdin if `INPUT` is omitted.

```bash
ironpass convert input.yaml --from clash --to singbox --output singbox.json
```

### `ironpass analyze [URL]`

Print statistics about a subscription.

| Flag         | Description                          |
|--------------|--------------------------------------|
| `--probe`    | Run connectivity probes              |
| `--detailed` | Show per-node details                |

### `ironpass export [URL] --target TARGET`

Export a subscription for a specific client. Filters out placeholder nodes.

```bash
ironpass export "https://example.com/sub/TOKEN" --target singbox --output sb.json
```

### `ironpass ping URL`

Measure latency and inspect headers from a subscription endpoint.

```bash
ironpass ping "https://example.com/sub/TOKEN" --timeout 15
```

### `ironpass proxy [URL]`

Start a local SOCKS5/HTTP proxy using a selected node. This is the primary way to use IronPass as a VPN client.

| Flag              | Default | Description            |
|-------------------|---------|------------------------|
| `--node INDEX`    | `0`     | Select node by index   |
| `--socks-port`    | `1080`  | Local SOCKS5 port      |
| `--http-port`     | `8080`  | Local HTTP proxy port  |
| `--hwid VALUE`    | none    | Override HWID          |

```bash
ironpass proxy "https://example.com/sub/TOKEN" --node 0 --socks-port 1080 --http-port 8080
```

Then configure your browser, application, or OS to use `socks5h://127.0.0.1:1080` or `http://127.0.0.1:8080`.

### `ironpass completions SHELL`

Generate shell completions.

```bash
ironpass completions bash > /etc/bash_completion.d/ironpass
```

### `ironpass config <action>`

| Action              | Description                          |
|---------------------|--------------------------------------|
| `show`              | Print current configuration          |
| `reset`             | Reset configuration to defaults      |
| `set KEY VALUE`     | Set a config key                     |
| `paths`             | Show config and data file paths      |

## Development and Testing

### Build the workspace

```bash
cargo build --workspace
```

### Run the test suite

```bash
cargo test --workspace
```

The workspace contains:

- Unit tests in each crate (over 100 tests in `ironpass-subscription`).
- Integration tests for the HWID retry and metadata extraction logic in `crates/subscription/tests/`.
- CLI integration tests using `wiremock` in `crates/cli/tests/fetch_integration_tests.rs`.

### Build and view documentation

```bash
cargo doc --workspace --no-deps --open
```

### Code style

The workspace uses the Rust 2024 edition. Format and lint before committing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Security and Privacy

- HWIDs are derived locally from device attributes and are **not** transmitted anywhere except to the subscription provider you configure.
- No subscription content, URLs, or HWIDs are sent to IronPass developers or third-party telemetry services.
- Subscription URLs and HWIDs are stored locally in the XDG config/data directories. Protect these directories with appropriate filesystem permissions.
- The proxy engine is experimental; review node credentials before routing traffic through an untrusted proxy.

## License

IronPass is licensed under the [MIT License](LICENSE-MIT) <!-- VERIFY: actual license file name/path -->. See the license file for details.
