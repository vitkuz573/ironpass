<!-- generated-by: gsd-doc-writer -->

# Contributing to IronPass

Thank you for considering a contribution to IronPass. This document describes how to set up a development environment, follow our coding conventions, add new parser formats, run tests, and report issues.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Development Setup](#development-setup)
- [Commit Message Conventions](#commit-message-conventions)
- [Test-Driven Development Workflow](#test-driven-development-workflow)
- [How to Add a Parser Format](#how-to-add-a-parser-format)
- [How to Run Tests](#how-to-run-tests)
- [Reporting Issues](#reporting-issues)

## Code of Conduct

We expect all contributors to interact respectfully and constructively. In particular:

- Be respectful in code reviews and issue discussions.
- Accept constructive criticism gracefully.
- Focus on what is best for the project and its users.
- Avoid discriminatory, harassing, or otherwise unwelcome behaviour.

Contributors who violate these standards may be banned from further participation.

## Development Setup

1. **Install Rust 1.85 or newer** using [rustup](https://rustup.rs/):

   ```bash
   rustup update stable
   rustc --version
   ```

2. **Clone the repository**:

   ```bash
   git clone https://github.com/example/ironpass.git
   cd ironpass
   ```

3. **Install system dependencies**:

   A C compiler is required for `rustls` and `rusqlite`.

   - Debian/Ubuntu: `sudo apt-get install build-essential pkg-config libssl-dev`
   - macOS: `xcode-select --install`
   - Windows: install the MSVC build tools or MinGW.

4. **Build the workspace**:

   ```bash
   cargo build --workspace
   ```

5. **Run the test suite**:

   ```bash
   cargo test --workspace
   ```

6. **Install the CLI locally** (optional):

   ```bash
   cargo install --path crates/cli
   ```

For day-to-day development, see [ARCHITECTURE.md](ARCHITECTURE.md) for a description of each crate and the data flow.

## Commit Message Conventions

We use [Conventional Commits](https://www.conventionalcommits.org/) to keep the changelog machine-readable.

Format:

```
<type>(<scope>): <short summary>

<body>

<footer>
```

Types:

| Type       | Use when                                                             |
|------------|----------------------------------------------------------------------|
| `feat`     | Adding a new feature or significant capability                       |
| `fix`      | Fixing a bug                                                         |
| `docs`     | Documentation-only changes                                           |
| `style`    | Code style changes (formatting, semicolons, etc.)                    |
| `refactor` | Code changes that neither fix a bug nor add a feature                |
| `perf`     | Performance improvements                                             |
| `test`     | Adding or correcting tests                                           |
| `chore`    | Maintenance tasks (build, dependencies, CI)                          |

Common scopes:

- `cli`
- `subscription`
- `parser`
- `hwid`
- `config`
- `engine`
- `core`
- `transport`

Examples:

```
feat(parser): add hysteria2 URI parser

fix(subscription): do not retry when explicit HWID is rejected
test(cli): add wiremock integration for device limit error
```

## Test-Driven Development Workflow

We require new features and bug fixes to be accompanied by tests.

1. **Write a failing test first.** The test should express the desired behaviour in the smallest possible terms.
2. **Implement the minimal change** needed to make the test pass.
3. **Refactor** while keeping the test green.
4. **Add integration tests** when the change crosses crate boundaries or touches the CLI.
5. **Run the full workspace test suite** before opening a pull request:

   ```bash
   cargo test --workspace
   ```

Unit tests live in `#[cfg(test)]` modules next to the code they exercise. Integration tests live in `crates/<crate>/tests/` directories. CLI integration tests use `wiremock` to avoid relying on external network services.

## How to Add a Parser Format

1. Open `crates/subscription/src/parser.rs`.
2. Extend `SubscriptionParser::detect_format` to recognise the new format.
3. Add a private parsing method (for example `parse_my_format`) that returns `Result<Vec<ProxyNode>>`.
4. Call the new method from `SubscriptionParser::parse`.
5. Map the source-specific fields to the canonical `ProxyNode` fields:

   | `ProxyNode` field | Typical source field |
   |-------------------|----------------------|
   | `protocol`        | `type` or URI scheme |
   | `server`          | `server` / `add`     |
   | `port`            | `port` / `server_port` |
   | `uuid`            | `uuid` / `id`        |
   | `password`        | `password`           |
   | `transport`       | `network` / `type`   |
   | `security`        | `tls` / `security`   |
   | `sni`             | `sni` / `server_name`|
   | `path`            | `path` / `ws-path`   |
   | `host`            | `host` / `headers.Host` |

6. Add unit tests covering a minimal example and edge cases (missing fields, unsupported variants).

### Updating the CLI

If the new format should be selectable from the command line:

1. Add a corresponding variant to `FetchFormatArg` in `crates/cli/src/args.rs` if the format only affects display.
2. Add an integration test in `crates/cli/tests/` if the format is exposed by `fetch`.

## How to Run Tests

```bash
# Run all tests in the workspace
cargo test --workspace

# Run tests for a specific crate
cargo test -p ironpass-subscription
cargo test -p ironpass-cli

# Run integration tests only
cargo test --workspace --test '*'

# Run with verbose output
cargo test --workspace -- --nocapture

# Build and open documentation
cargo doc --workspace --no-deps --open
```

Before submitting a pull request, also run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Reporting Issues

When reporting a bug, please include the following information:

1. **IronPass version** (`ironpass --version`).
2. **Rust version** (`rustc --version`).
3. **Operating system and architecture**.
4. **Steps to reproduce** the issue, with the exact command you ran.
5. **Expected behaviour** and **actual behaviour**.
6. **Minimal input** that triggers the problem (use fake URLs and UUIDs, never real subscription credentials).
7. **Relevant logs** if available; run with `-v` to enable debug logging.

For feature requests, describe the use case, the desired CLI interface, and any relevant formats or protocols.
