# rust-mcp-server

**MCP (Model Context Protocol) server for exhaustive Rust code analysis.**

MCP server written in Rust that analyzes Rust projects via [`syn`](https://github.com/dtolnay/syn) + `toml` and [`rust-analyzer`](https://github.com/rust-lang/rust-analyzer) (`ra_ap_*` as semantic ASG). Exposes **37 tools** that return **deterministic Markdown** ready for consumption by LLMs.

It is the Rust counterpart of `cobol-mcp-server`, `java-mcp-server` and `python-mcp-server`: same philosophy ("ground truth", no hallucination), different domain and different parser (syn/toml/ra_ap_* instead of ProLeap/Spoon/LibCST).

---

## Quick start

### Prerequisites

- **Rust 1.85+** (tested with 1.96.0 via brew and rustup)
- **macOS / Linux** (Windows not tested)
- Optional, for cross-compile to musl: `zig` + `cargo-zigbuild` + `rustup` with targets `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`

### Native build

```bash
cargo build --release
```

Binary: `target/release/rust-mcp-server` (~13 MB, arm64/x86_64).

### Run

MCP Server use **stdio transport** — is spawned as a subprocess of OpenCode, Claude Desktop, Visual Studio Code, etc.:

```bash
./target/release/rust-mcp-server
```

### Logging

Default is `/tmp/rust_mcp_server.log`. Configurable via env:

| Variable | Default | Description |
|---|---|---|
| `RUST_MCP_LOG_FILE` | `/tmp/rust_mcp_server.log` | Path del fichero de log |
| `RUST_MCP_LOG_LEVEL` | `info` | `debug`, `info`, `warn`, `error` |

### Test

```bash
cargo test                            # 78 tests: 33 unit + 45 golden snapshots
cargo test --release                  # same but optimized
INSTA_UPDATE=always cargo test        # generate snapshots
```

### Smoke test end-to-end

```bash
cargo build --release
python3 scripts/smoke_test.py
```

The script performs the full JSON-RPC handshake: `initialize` → `tools/list` → loads the fixture → invokes 11 tools → verifies outputs → `exit`. Verifies the server works end-to-end with a real MCP client.

---

## CI and container

The project has a GitHub Actions workflow at `.github/workflows/ci.yml` with 3 jobs:
- `test-native`: `cargo test` on Ubuntu.
- `cross-compile-musl`: `cargo zigbuild` + verification with `docker run alpine`.
- `lint`: `cargo fmt --check` + `cargo clippy` (without `-D warnings` due to minor doc list warnings).

And a multi-stage `Dockerfile` that produces the `rust-mcp-server:test` image (~42 MB):

```bash
podman build -t rust-mcp-server:test -f Dockerfile .
podman run --rm -i rust-mcp-server:test
```

## Test fixtures

The project includes two example Rust crates in `tests/fixtures/`, designed so the tools have real material to work with (instead of empty fixtures):

**`sample_crate/`** — the main fixture. Has 3 **real** and **used** deps:
- `cfg-if = "1"` (normal) — used in `is_release()` with the `cfg_if!` macro (verifies `cfg-if` is processed correctly).
- `once_cell = "1"` (normal) — used in `cached_value()` (verifies sync::OnceCell).
- `pretty_assertions = "1"` (dev-dep) — used in tests (verifies dev-deps).

The fixture has 4 passing tests: `test_add`, `test_multiply`, `test_is_release`, `test_cached_value`.

**`sample_crate_with_deps/`** — fixture for `find_unused_dependencies`. Has 3 UNUSED deps (serde, unused_dep, unused_dev_dep) that the tool must report.

---

`rust-mcp-server` can be compiled as a **static Linux binary** using [`zig`](https://ziglang.org/) as linker (via `cargo-zigbuild`). The resulting binary does not depend on glibc and runs on any modern Linux distro (Alpine, Ubuntu, RHEL, etc.) without external dependencies.

### Prerequisites

```bash
# zig — linker used by cargo-zigbuild
brew install zig

# rustup — the Rust toolchain manager
brew install rustup-init
rustup-init -y --default-toolchain stable
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl

# cargo-zigbuild — the wrapper that invokes zig as linker
cargo install cargo-zigbuild
```

> **Note on dual toolchain**: if you have Rust installed via Homebrew (`brew install rust`), `rustup target add` only affects the rustup toolchain. The default `cargo` resolves to the Homebrew binary, which does NOT see the musl targets. **Solution** (recommended): `brew uninstall rust` to have a single toolchain. If you do not want to uninstall, you need to create rustup shims manually (next section).

#### Shims setup if you have toolchain conflict

If after `brew install rustup-init` the default `cargo` is not from rustup, you need to create shims:

```bash
RUSTUP_REAL=$(find /Users/alejandro.ferreira/homebrew/Cellar/rustup -name rustup -type f 2>/dev/null | head -1)

for cmd in cargo rustc rustup rustdoc rustfmt cargo-clippy clippy-driver; do
  cp "$RUSTUP_REAL" "$HOME/.cargo/bin/$cmd"
done
chmod +x "$HOME/.cargo/bin/"{cargo,rustc,rustup,rustdoc,rustfmt,cargo-clippy,clippy-driver}

cargo --version
```

> **Why `cp` and not `ln -s`**: the rustup binary detects its name via `argv[0]`. Symlinks preserve `argv[0]` pointing to `rustup`, not `cargo`, so dispatch fails. `cp` breaks the link, giving each shim its own `argv[0]`.

### Build

```bash
cargo zigbuild --release --target x86_64-unknown-linux-musl

cargo zigbuild --release --target aarch64-unknown-linux-musl
```

Output:
- `target/x86_64-unknown-linux-musl/release/rust-mcp-server` (~16 MB, statically linked)
- `target/aarch64-unknown-linux-musl/release/rust-mcp-server` (~14 MB, statically linked)

> **Cold build time**: ~4 min per target. Incremental builds are seconds (musl targets are cached in `target/`).

### Test in podman (Docker replacement)

```bash
podman machine start

podman run --rm -v $(pwd):/app:ro -w /app alpine sh -c "
  {
    echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"x\",\"version\":\"1\"}}}'
    echo '{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}'
    echo '{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}'
    sleep 1
  } | /app/target/x86_64-unknown-linux-musl/release/rust-mcp-server 2>&1 | head -c 500
"
```

### End-to-end verification with 6 tools

```bash
podman run --rm -v $(pwd):/app:ro -w /app alpine sh -c "
{
  echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"musl-test\",\"version\":\"1.0\"}}}'
  echo '{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}'
  echo '{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"load_rust_project\",\"arguments\":{\"source\":\"/app/tests/fixtures/sample_crate\",\"alias\":\"sample\"}}}'
  sleep 1
  echo '{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"program_summary\",\"arguments\":{\"name\":\"sample\"}}}'
  sleep 1
  echo '{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"resolve_type\",\"arguments\":{\"name\":\"sample\",\"symbol\":\"add\"}}}'
  sleep 1
  echo '{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"find_dead_code\",\"arguments\":{\"name\":\"sample\"}}}'
  sleep 1
  echo '{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"analysis_summary\",\"arguments\":{\"name\":\"sample\"}}}'
  sleep 2
} | /app/target/x86_64-unknown-linux-musl/release/rust-mcp-server 2>&1
"
```

Expected output (summary):
- `ID 2: # Project Loaded`
- `ID 3: # Program Summary: 'sample_crate'`
- `ID 4: # Definition of 'add' (1)`
- `ID 5: # Dead code (semantic) (0)`
- `ID 6: # ASG Summary: 'sample_crate'`

### musl limitations

- `ureq` (HTTP client) may not work on musl for some URLs (TLS). We don't use it for real I/O, so it does not matter.
- `gix` is included but `resolve_git` uses `git` CLI (not `gix::clone`), so it requires `git` installed on the target system (not a problem for our use case).
- `cargo-zigbuild` emits a warning during build: `linker stderr: ignoring deprecated linker optimization setting '1'`. Comes from the interaction with `zig 0.16`. **Does not affect the binary** — it builds and works correctly.

---

## MCP tool reference (37 tools)

### Source management (5)
- `load_rust_project` — loads a local path, Git URL, or archive URL
- `unload_rust_project` — removes from cache
- `project_metadata` — name, alias, kind, path, package info
- `inspect_build_config` — shows the `Cargo.toml`
- `list_crates` — workspace members

### Cargo/TOML (3)
- `list_dependencies` — `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`
- `list_cargo_features` — `[features]`
- `program_summary` — high-level counts (files, functions, structs, etc.)

### File ops (2)
- `get_file_content` — reads a file (with path traversal protection)
- `search_source` — textual search

### Source listings with `syn` (7)
- `list_functions`, `list_structs`, `list_enums`, `list_traits`, `list_impls`, `list_modules`, `list_macros`

### Inspections with `syn` (4)
- `inspect_function` (with body), `inspect_struct` (with fields), `inspect_enum` (with variants), `inspect_trait` (with methods)

### Semantic cross-references with `ra_ap_*` (6)
- `analysis_summary` — verifies the ASG loads
- `resolve_type` — `goto_definition` from rust-analyzer
- `find_implementations` — `goto_implementation` (traits → implementations)
- `get_type_hierarchy` — type hierarchy
- `function_call_graph` — call graph ENTRY POINTS (callers)
- `struct_field_xref` — references to a field

### Text-based references (2)
- `find_references` — grep whole-word
- `function_callers` — text-based call graph

### Validation (1)
- `validate_code_reference` — EXISTS / NOT_FOUND for top-level symbols

### Bug-hunt (6)
- `find_unsafe_blocks` — `unsafe { ... }` blocks
- `find_unwrap_patterns` — `.unwrap()`, `.expect()` calls
- `find_unchecked_patterns` — `mem::transmute`, raw pointer ops
- `find_panic_sites` — `panic!`, `unreachable!`, `todo!`
- `find_dead_code` — unreferenced items (skip `#[test]`, `#[cfg]`, `#[derive]`, `#[no_mangle]`)
- `find_unused_dependencies` — dependencies declared in `Cargo.toml` not used in `.rs`

### Loading (1)
- `list_loaded_projects` — lists loaded projects

---

## Configuration for AI agents

### OpenCode / Claude Desktop / VS Code / Cline

Add to `opencode.json` / `claude_desktop_config.json` / equivalent:

```json
{
  "mcpServers": {
    "rust-mcp-server": {
      "command": "/path/to/target/release/rust-mcp-server"
    }
  }
}
```

### Linux (binario fat-binary musl)

```json
{
  "mcpServers": {
    "rust-mcp-server": {
      "command": "/path/to/target/x86_64-unknown-linux-musl/release/rust-mcp-server"
    }
  }
}
```

---

## Architecture

1:1 mirror of the other three AMS MCPs (`cobol-mcp-server`, `java-mcp-server`, `python-mcp-server`):

```
rust-mcp-server/
├── src/
│   ├── main.rs / lib.rs / server.rs    — entrypoint + tool registry
│   ├── project_manager.rs              — RwLock<HashMap> + expiry
│   ├── project_loader.rs               — path / Git CLI / archive resolver
│   ├── model/                          — ProjectEntry, ProjectLoadError, Finding
│   ├── analysis/                       — BugPatternAnalyzer (5 sweeps)
│   │   ├── analysis_host.rs            — ra_ap_ide AnalysisHost wrapper
│   │   ├── symbol_resolver.rs          — nombre → SymbolLocation
│   │   ├── syntax.rs                   — syn wrapper (para sweeps estáticos)
│   │   ├── cargo_manifest.rs           — toml wrapper
│   │   └── rust_analyzer_utils.rs      — fachada estilo ProLeapAnalyzerUtils
│   └── tools/                          — 36 tools (uno por struct)
│       ├── mcp_tool.rs                 — trait McpTool
│       ├── base_rust_tool.rs           — helpers compartidos
│       └── <tool_name>_tool.rs         — cada tool
└── tests/
    ├── fixtures/sample_crate/          — crate Rust mínimo para tests
    └── snapshots_test.rs               — 43 golden snapshots
```

**Design decisions** (see `AGENTS.md` §7 for details):

- **Verbose tool pattern**: one struct per tool, explicit registration in `RustMcpServer::build_tools()`. Inspired by `BaseCobolTool` / `BaseJavaTool`.
- **Stable `McpTool` trait**: tool contract. Do NOT change without reviewing all tools.
- **Thread-safe ProjectManager**: `tokio::sync::RwLock<HashMap>`, 10 min expiry, same shape as `java-mcp-server`.
- **Deterministic Markdown**: byte-for-byte output, no timestamps. Golden snapshots in `tests/snapshots/`.
- **Version from `version.txt`**: `build.rs` injects it into `OUT_DIR/version.rs`, `lib.rs` includes it with `include!`. Equivalent to Maven filtering of `version.properties` in Java.
- **Dual-engine bug-hunt**: 4 sweeps use `syn` (static, parse-only), 1 sweep (`find_dead_code`) uses `ra_ap_*` (semantic with `find_all_refs`). `SymbolLocation` includes `implicit_references: { test, derive, no_mangle, cfg, allow }` to avoid false positives.

---

## Roadmap

| Iter | What | Status |
|------|------|--------|
| 1 | Skeleton: `rmcp` + tool pattern + `ProjectManager` + 1 tool + snapshots | ✅ |
| 2 | Local loading + archive (zip/tar.gz/tar.zst/tar.xz) + analysis with `syn` + `toml` | ✅ |
| 3 | `ra_ap_*` 0.0.343 integrated, `analysis_summary` | ✅ |
| 3.5 | `SymbolResolver`, semantic `resolve_type` | ✅ |
| 3.6 | 4 semantic tools: `find_implementations`, `get_type_hierarchy`, `function_call_graph`, `struct_field_xref` | ✅ |
| 4 | `BugPatternAnalyzer` with 5 sweeps (4 syn + 1 heuristic) | ✅ |
| 4.5 | semantic `find_dead_code` (via `ra_ap_*`) | ✅ |
| **5** | **Cross-compile to musl with `cargo-zigbuild` + `zig`, verified in podman/Alpine** | ✅ |
| 6+ | `find_unused_dependencies` (sweep 6); `SymbolLocation.implicit_references` (skip `#[test]`/`#[cfg]`); path traversal fix; real Git clone via `git` CLI; consolidated `text_utils::count_whole_word_matches`; rustup shims for macOS | ✅ |
| 7+ | `cargo zigbuild --tests` verified; Git clone flow validated; refactor sweeps to `ra_ap_syntax` deferred (no `Visit` trait) | ✅ |

---