# AGENTS.md — rust-mcp-server

> Operations manual for agents (human or LLM) working on this repository.
> This file **does not just describe the project**: it defines **how** you should approach it.

---

## 1. Goal

MCP (Model Context Protocol) server written in Rust that analyzes Rust projects
via [`syn`](https://github.com/dtolnay/syn) + [`toml`](https://github.com/toml-rs/toml) and [`rust-analyzer`](https://github.com/rust-lang/rust-analyzer)
(`ra_ap_*` as semantic ASG, iter 3 + 3.5 + 3.6) and `BugPatternAnalyzer`. 
Exposes **37 tools** that return **deterministic Markdown** ready
for consumption by LLMs.

It is the Rust counterpart of `cobol-mcp-server`, `java-mcp-server` and
`python-mcp-server`: same philosophy ("ground truth", no hallucination), different
domain and different parser (syn/toml/ra_ap_* instead of ProLeap/Spoon/LibCST).

> ✅ **Status**: **iteration 9 completed** (Dual transport: `stdio` + Streamable `http` at `/mcp`
> with `--mode/--port/--host/--max-threads/--api-keys` and Bearer token auth; 37 tools;
> bug-hunt with 6 sweeps). **84 tests** (37 unit + 2 http integration + 45 golden snapshots).
> Semantic ASG (`ra_ap_*` 0.0.343) integrated.

---

## 2. Stack & commands

- **Rust 1.85+** (tested under 1.96.0 / 1.97.0; `rustup` optional — via Homebrew)
- Build: `cargo build --release` → `target/release/rust-mcp-server` (~13 MB macOS, 16 MB musl)
- Compile: `cargo check`
- Test: `cargo test` (84 tests: 37 unit + 2 http integration + 45 golden snapshots)
- Smoke stdio: `python3 scripts/smoke_test.py`
- Smoke HTTP: `./scripts/smoke-http.sh --port 18090`
- Version: managed by `version.txt` (read by `build.rs` → `OUT_DIR/version.rs`)
- Key dependencies:
  - `rmcp = "2.2"` (oficial, 4t145) — features `["server", "macros", "schemars", "transport-io", "transport-streamable-http-server", "transport-streamable-http-server-session"]`
  - `axum = "0.8"` (HTTP router for Streamable HTTP mode)
  - `tokio = "1"` (async runtime, features `["macros", "rt-multi-thread", "sync", "time", "io-util", "fs", "signal"]`)
  - `serde` + `serde_json` (serialización)
  - `schemars` (JSON Schema for inputSchema)
  - `thiserror` + `anyhow` (errors)
  - `chrono` (expiry of ProjectEntry)
  - `tracing` + `tracing-subscriber` (logging)
  - `async-trait` (async dispatch in `McpTool`)
  - `insta` (golden snapshots, dev-dep)
  - `tempfile` (test fixtures, dev-dep)
  - `reqwest` (integration tests, dev-dep)
- Transport:
  - Default: `stdio` (`rmcp::transport::io::stdio()`)
  - HTTP mode: Streamable HTTP (`rmcp::transport::streamable_http_server::StreamableHttpService` via Axum on `/mcp`)
- Semantic análisis ASG: `ra_ap_*` 0.0.343 since early Iter 3 (see §13).
- **IMPORTANT**: `ra_ap_*` crates are version-pinned at `=0.0.343`. DO NOT update without verifying compatibility with all tools.

---

## 3. Arquitecture

```
rust-mcp-server/
├── Cargo.toml                          — deps + profile (lto, strip, panic=abort)
├── version.txt                         — version definition
├── build.rs                            — Version injection in OUT_DIR/version.rs
├── Dockerfile                          — multi-stage build musl
├── .github/workflows/ci.yml            — 3 jobs: test-native, cross-compile-musl, lint
├── src/
│   ├── main.rs                         — entrypoint: init_tracing + start()
│   ├── lib.rs                          — public API for tests
│   ├── server.rs                       — RustMcpServer + impl ServerHandler
│   ├── project_manager.rs              — RwLock<HashMap> + expiry (10 min)
│   ├── project_loader.rs               — path / Git CLI / archive resolver
│   ├── model/
│   │   ├── project_entry.rs            — record: name, alias, kind, path, expiry
│   │   ├── project_load_exception.rs   — error codes
│   │   └── finding.rs                  — Finding + FindingCategory + FindingSeverity
│   ├── analysis/                       — dual engine (syn + ra_ap_*)
│   │   ├── analysis_host.rs            — ProjectAnalysisHost (ra_ap_ide wrapper)
│   │   ├── symbol_resolver.rs          — name → SymbolLocation (with ImplicitReferences)
│   │   ├── bug_pattern_analyzer.rs     — 6 sweeps (4 static + 1 semantic + 1 heuristic)
│   │   ├── ra_walker.rs                — walk manual over ra_ap_syntax::SyntaxNode
│   │   ├── syntax.rs                   — wrapper syn (legacy)
│   │   ├── cargo_manifest.rs           — wrapper toml
│   │   ├── rust_analyzer_utils.rs      — ProLeapAnalyzerUtils like facade
│   │   └── text_utils.rs               — count_whole_word_matches
│   └── tools/                          — 37 tools (one struct each)
│       ├── mcp_tool.rs                 — trait McpTool
│       ├── base_rust_tool.rs           — shared helpers
│       ├── bug_hunt_helpers.rs         — format_findings, severity_rank
│       ├── ra_helpers.rs               — build_resolver_for_entry, format_references
│       ├── render.rs                   — helpers Markdown (table, code_block, etc.)
│       └── <tool_name>_tool.rs         — each tool
└── tests/
    ├── fixtures/
    │   ├── sample_crate/               — main fixture with 3 deps
    │   └── sample_crate_with_deps/     — fixture for find_unused_dependencies
    ├── snapshots/                      — 45 golden snapshots
    └── snapshots_test.rs               — snapshot tests

**Architectural rules**:

- `McpTool` is the **stable trait**. Tools implement it, they do not instantiate
  `rmcp` directly. A change to `McpTool` touches all tools.
- `BaseRustTool` is the **utilities struct**. Each tool contains it and
  uses it for `arg()`, `ok_markdown()`, `find_entry()`.
- `ProjectManager` is thread-safe (`tokio::sync::RwLock<HashMap>`) and applies
  per-entry expiry (10 min, same as java/cobol).
- `RustMcpServer::build_tools()` is the **single registration point** for tools.
  Adding a new one = one line there. (Mirror of `start()` in Java.)
- `model/` is pure types, no logic, no I/O.

---

## 4. MCP servers available to the agent

This project runs alongside three other MCP servers that you should actively use.

### 4.1 — `java-mcp-server` (analysis of the Rust code itself)

Load it **at the start of every working session on the repo**:

```
load_java_project(source="<path to repo>", alias="rust-mcp-server")
```

Uses:
- `inspect_class` / `inspect_method` — understand a tool or a manager.
- `list_methods` / `find_references` — locate extension points.
- `find_implementations` (of `McpTool` or `BaseRustTool`) — see existing tools
  before adding a new one.
- `validate_code_reference` — before assuming a symbol exists after editing.
- `search_source` — search patterns across all code.

### 4.2 — `cobol-mcp-server` (cross-validation of pattern)

Useful when you need to contrast the Rust implementation against the Java pattern:

```
load_cobol_file(filePath="path/to/fixture.cbl")
```

`cobol-mcp-server` is the "grandfather" of the pattern: 68 tools, 5 bug-hunt sweeps,
`ProLeapAnalyzerUtils` facade. The rust-mcp-server code should end up
structurally parallel.

### 4.3 — `python-mcp-server` (cross-validation of pattern)

Similar to cobol: the Python version has 37 tools with a more
functional pattern (one module per tool). Useful for comparison.

### 4.4 — Available skills

- `audit-warnings` (global) — audit IDE/clippy warnings.
- `diagnose-stale-mcp` (global) — diagnose MCPs that don't respond, outdated JARs.
- The specific skills of the other MCPs (`cobol-bug-hunt`, etc.) **do not
  apply** directly here — the domain is different.

### 4.5 — The rust-mcp-server's own Rust code

It is the **source of truth** about what information the server extracts and how it
formats it. If in doubt about what a tool returns, read its `call()` and the `BaseRustTool`
helpers it invokes. **Do not infer from the tool name.**

---

## 5. Deterministic procedures (recipes)

### 5.1 — Adding a new MCP tool

1. `java-mcp-server` → `inspect_class` on `McpTool` and `BaseRustTool`
   (confirm the contract).
2. `find_implementations McpTool` (see existing tools, maintain style).
3. Create `src/tools/<tool_name>.rs` with:
   - struct `pub struct XxxTool { base: BaseRustTool }`
   - `pub fn new(manager: Arc<ProjectManager>) -> Self`
   - `impl McpTool for XxxTool` with `name()`, `description()`, `input_schema()`, `call()`
4. Add `pub mod <tool_name>;` to `src/tools/mod.rs`.
5. Add `Box::new(XxxTool::new(self.manager.clone()))` to
   `RustMcpServer::build_tools()` (server.rs).
6. `cargo check` — must pass.
7. Load a fixture and verify that the tool returns Markdown in the expected
   shape.
8. Add a test in `tests/snapshots_test.rs` following the pattern of
   `list_loaded_projects_with_one_project`.

### 5.2 — Modifying an existing tool

1. `java-mcp-server` → load the project.
2. `inspect_method` of the `call()` to modify.
3. `find_references` to list all consumers (tests, server.rs).
4. **Minimal and local** change — do not refactor along the way.
5. `cargo test` must stay green (78/78).
6. Re-run the snapshot test: the output must match the snapshot byte-for-byte.
   If intentionally changed, regenerate with `INSTA_UPDATE=always`.

### 5.3 — Investigating a bug in a tool

1. Load the minimal fixture that triggers the bug.
2. Run the tool from the binary (`target/release/rust-mcp-server`) with
   a test MCP client (see §11).
3. `inspect_method` of the `call()`.
4. `find_references` to trace dependencies.
5. Apply the minimal fix.
6. Repeat step 2: output must be as expected.
7. `cargo test` must stay green.

### 5.4 — Auditing clippy warnings

1. `cargo clippy --all-targets -- -D warnings` first (must be clean).
2. Resolve in this order: **latent bugs** → **dead code** → **deprecations**
   → **stylistic cleanup**.
3. After each change, `cargo test` (78 tests: 33 unit + 45 snapshots). The tests cover the general
   pattern, not the full ground truth — they don't replace manual validation
   when the change is semantically subtle.

---

## 6. Conocimiento Rust que el agente debe aplicar

### 6.1 — Type hierarchy in `rmcp 2.2` (key to avoiding broken constructors)

```
rmcp::ServerHandler (trait)            — implements the full MCP handler
├── get_info() -> ServerInfo           — sync; describes the server
├── list_tools()                       — async; lists tools
└── call_tool()                        — async; dispatches by name

rmcp::model::Tool (struct, non_exhaustive)
└── Tool::new(name, description, input_schema)  — the ONLY public constructor

rmcp::model::Implementation (struct, non_exhaustive)
└── Implementation::new(name, version).with_title(...)

rmcp::model::ServerInfo = InitializeResult (type alias)
└── ServerInfo::new(capabilities)
       .with_server_info(impl)
       .with_instructions(text)
```

**Implication**: `Tool`, `Implementation` and `ServerInfo` are **non-exhaustive**.
They cannot be built with a struct expression. ALWAYS use the provided
constructors or the compiler will yell `E0639`.

### 6.2 — `rmcp` specifics

- `Content` does not exist as a public type — it is `ContentBlock` and lives in
  `rmcp::model::content::ContentBlock` (re-exported as `rmcp::model::ContentBlock`).
  It is constructed with `ContentBlock::text(string)`.
- `ProtocolVersion::default()` → `LATEST` (V_2025_11_25 in this version).
- The `serve(transport)` method comes from `ServiceExt`, not from `ServerHandler`.
  You must `use rmcp::ServiceExt;` to invoke it.
- `server.waiting().await` returns `Result<QuitReason, tokio::task::JoinError>`.
- The `input_schema` of a `Tool` is `Arc<JsonObject>`, NOT `Map` directly.
  `Tool::new` accepts it via `Into<Arc<JsonObject>>`.
- `request.arguments` in `CallToolRequestParams` is `Option<JsonObject>`,
  not `serde_json::Value`. Must convert with `.map(|m| Value::Object(m.into_iter().collect()))`.

### 6.3 — Canonical code patterns

Tool construction (always this shape):
```rust
pub struct XxxTool { base: BaseRustTool }
impl XxxTool { pub fn new(manager: Arc<ProjectManager>) -> Self { Self { base: BaseRustTool::new(manager) } } }
#[async_trait]
impl McpTool for XxxTool {
    fn name(&self) -> &'static str { "xxx_tool" }
    fn description(&self) -> &'static str { "..." }
    fn input_schema(&self) -> Value { json!({ "type": "object", "properties": {} }) }
    async fn call(&self, args: Value) -> Result<CallToolResult, ErrorData> { ... }
}
```

`ListToolsResult::with_all_items(items)` is the deterministic constructor.
Avoid `ListToolsResult { tools, ... }` directly.

Text ContentBlock detection:
```rust
let text = match &result.content[0] {
    rmcp::model::ContentBlock::Text(t) => t.text.clone(),
    other => panic!("expected TextContent, got {:?}", other),
};
```

### 6.4 — `ra_ap_*` specifics (integrated since iter 3)

rust-analyzer is not a normal dependency: it is a **versioned monolith** that
publishes snapshots of its internal crates on `crates.io` with the
`ra_ap_*` prefix. Each rust-analyzer release breaks API — hence the **exact pin**
(`=0.0.343` in Cargo.toml).

**Current strategy**:

1. Exact pin: `ra_ap_ide = "=0.0.343"`, etc. (8 crates).
2. `ProjectAnalysisHost` (`analysis/analysis_host.rs`) wraps `ra_ap_ide::AnalysisHost`
   and exposes `analysis()` and `files: HashMap<String, FileId>`.
3. `SymbolResolver` indexes name → `SymbolLocation` with `file_id` + `TextRange`.
4. The semantic tools (`resolve_type`, `find_implementations`, `function_call_graph`,
   `struct_field_xref`, `find_dead_code`) use `Analysis::find_all_refs`,
   `goto_definition`, `goto_implementation`.
5. The bug-hunt sweeps (unsafe, unwrap, unchecked, panic) use
   `ra_ap_syntax::SourceFile` + `ra_walker` instead of `syn::visit::Visit`.
6. **`syn` legacy**: `rust_analyzer_utils.rs` (listings and inspections) and
   `syntax.rs` still use `syn` as parser. Full migration to `ra_ap_*` is
   future work (deeper scope).

Ver §13 para el roadmap completo.

---

## 7. Code conventions

- **Rust**: idiomatic, no `unsafe` unless necessary. `tracing` for logs.
- Imports: from `rust_mcp_server::model`, `rust_mcp_server::project_manager`,
  `rust_mcp_server::tools::{mcp_tool, base_rust_tool}`. Never import
  `rmcp` directly from a tool that is not the `call()` itself.
- Tools: one struct per tool, central registration in `RustMcpServer::build_tools()`.
- Naming: snake_case for functions/modules, PascalCase for structs.
- Logging: `tracing::{info, debug, warn, error}`, one line per event, no PII.
- Output: stable Markdown (via `BaseRustTool::ok_markdown`), no timestamps
  or volatile content.
- Error messages: readable for the final LLM (in Spanish when suggesting
  heuristic fixes, in English for structural errors).
- Tests: `cargo test` with `insta::assert_snapshot!` for golden snapshots,
  Rust fixtures in `tests/fixtures/`, do not use external crates in tests.

---

## 8. Anti-patterns (DO NOT do)

- ❌ Do not use struct expression with `Tool`, `Implementation` or `ServerInfo` —
  they are `non_exhaustive`. ALWAYS use the constructors (`Tool::new`,
  `Implementation::new`, `ServerInfo::new`).
- ❌ Do not use `tokio::fs` without the `fs` feature on `tokio`. For simple I/O
  in non-async code, `std::fs` is simpler.
- ❌ Do not include timestamps (expiry, loaded_at) in tool Markdown —
  it breaks the determinism of golden snapshots. If you need to expose them,
  do it in a separate tool (`project_metadata_tool` with `include_expiry: bool`).
- ❌ Do not change the `McpTool` or `BaseRustTool` signature without updating all
  tools — it is the stable API of the ground truth.
- ❌ Do not add a tool that does not extend `McpTool` (breaks homogeneity and the
  `build_tools()` aggregator).
- ❌ Do not hardcode the version number in `Cargo.toml` AND in `version.txt` —
  always read from `version.txt` (via `include!(concat!(env!("OUT_DIR"), "/version.rs"))`).
- ❌ Do not log absolute user paths or PII in the Markdown output.
- ❌ Do not use a tool's result as truth without having cross-checked it against
  the Rust code that generates it.
- ❌ Do not break determinism: two calls on the same project must
  produce **exactly** the same Markdown (byte-for-byte).

---

## 9. Self-check before answering

If the user's question touches the Rust code of `rust-mcp-server` or the
output of a tool:

1. Have I loaded the project with `java-mcp-server`? If no → load it.
2. Have I validated at least one proposal with `validate_code_reference` or
   `inspect_class`? If no → do it.
3. If the proposal touches a tool, have I run it against the fixture
   (`tests/fixtures/sample_crate`)? If no → do it.
4. Does my answer explicitly distinguish between **"what the code does
   today"** (verified by reading/loading) and **"what it should do"** (proposal)?
5. If I propose a change, does it include a procedure to verify it (concrete
   steps with `cargo test` and/or invoking the binary)?

If any answer is "no", the next step before proceeding is to cover it.

---

## 10. Example session transcript (pattern to follow)

**Task**: "the `list_loaded_projects` tool does not show the workspace member
count in the main table, only in a separate section".

```
[1] load_java_project(rust-mcp-server)                                    # java-mcp-server
[2] inspect_class ListLoadedProjectsTool                                  # see call()
[3] list_methods ListLoadedProjectsTool                                   # see the shape
[4] find_references ListLoadedProjectsTool                                # see consumers (server, tests)
[5] inspect_method ListLoadedProjectsTool call                            # understand the loop
[6] (proposal) add "Members" column to the table, with the count
[7] cargo test                                                            # 7/7 green (or regenerate snapshot)
[8] cargo build --release && smoke test with python client                # verify end-to-end
[9] commit with message explaining the Markdown schema change
```

---

## 11. Recursos

- `rmcp` 2.2: https://github.com/modelcontextprotocol/rust-sdk
- rust-analyzer (futuro): https://github.com/rust-lang/rust-analyzer
- MCP spec: https://modelcontextprotocol.io/specification/2025-11-25
- README del repo: `./README.md` (orientado a usuarios finales)
- Sibling projects:
  - `../cobol-mcp-server/AGENTS.md` (same pattern, ProLeap, 68 tools)
  - `../java-mcp-server/AGENTS.md` (same pattern, Spoon, 27 tools)
  - `../python-mcp-server/AGENTS.md` (same pattern, LibCST, 37 tools)

---

## 12. Golden snapshot tests (determinism enforced)

The repo includes `tests/snapshots_test.rs` which runs each tool against the
fixtures and compares the Markdown **byte-for-byte** against snapshots stored in
`tests/snapshots/`.

**Current coverage**: 45 snapshots (all main tools and variants).
The snapshots cover: source management, listings, inspections, semantic
cross-references, bug-hunt sweeps, and edge case validation (symbol not found,
path traversal blocked, tool without arguments).

**Test runner behavior**:
- **First run** (`INSTA_UPDATE=auto` or `always`): creates the snapshot.
- **Subsequent runs**: if the output differs, the test fails with a clear
  diff and the `.snap.new` file with the new version.

**Rules**:
- Any change that **intentionally modifies** the output must be
  accompanied by snapshot regeneration (`INSTA_UPDATE=always cargo test`).
- Snapshots are part of the contract — a change that affects them requires
  the same level of review as an API change.

---

## 13. Plan de iteraciones

| Iter | What is added | Status |
|------|---------------|--------|
| **1** | Skeleton: `rmcp` + tool pattern + `ProjectManager` + 1 tool (`list_loaded_projects`) + golden snapshots + AGENTS.md/README + smoke test end-to-end | ✅ Done |
| **2** | Local loading + archive (zip/tar.gz/tar.zst/tar.xz) + analysis with `syn` + `toml`. **25 tools** registered. `RustAnalyzerUtils` facade (mirror of `ProLeapAnalyzerUtils`). | ✅ Done |
| **3 (minimum)** | `ra_ap_*` 0.0.343 integrated (`ra_ap_syntax`, `ra_ap_hir`, `ra_ap_ide`, `ra_ap_project_model`, `ra_ap_base_db`, `ra_ap_vfs`, `ra_ap_cfg`, `ra_ap_ide_db`, `ra_ap_test_fixture`). `ProjectAnalysisHost` loads the project via `ChangeFixture` and exposes a `ra_ap_ide::Analysis`. **1 semantic tool** (`analysis_summary`). Binary: 12 MB. | ✅ Done |
| **3.5** | `SymbolResolver` (`src/analysis/symbol_resolver.rs`) — indexes name → `SymbolLocation { file_id, name_range, kind, full_range }`. Helpers in `src/tools/ra_helpers.rs`. **1 new semantic tool** (`resolve_type`) that uses `Analysis::goto_definition` to resolve a symbol to its definition. 27 tools total. | ✅ Done |
| **3.6** | The remaining 4 semantic tools: `find_implementations` (via `goto_implementation`), `get_type_hierarchy` (via `goto_implementation` on trait), `function_call_graph` (via `find_all_refs` — call graph ENTRY POINTS, i.e. callers; callees require iter 4 with HIR), `struct_field_xref` (via `find_all_refs` on field name). 31 tools total. | ✅ Done |
| **4** | `BugPatternAnalyzer` (`src/analysis/bug_pattern_analyzer.rs`) — 5 bug-hunt sweeps. 4 static with `syn` (`visit::Visit`): `find_unsafe_blocks`, `find_unwrap_patterns`, `find_unchecked_patterns`, `find_panic_sites`. 1 heuristic with `syn` (textual whole-word): `find_dead_code` (semantic version with `ra_ap_*` → iter 4.5). 36 tools total. | ✅ Done |
| **4.5** | `find_dead_code` rewritten with `SymbolResolver` + `Analysis::find_all_refs` (semantic). 36 tools. | ✅ Done |
| **5** | Cross-compile to musl with `cargo-zigbuild` + `zig`. Static ELF 64-bit binaries (x86_64: 16 MB, aarch64: 14 MB). | ✅ Done |
| **6+** | `find_unused_dependencies`; `SymbolLocation.implicit_references` (skip `#[test]`); fix path traversal; real Git clone; rustup shims on macOS. 37 tools. | ✅ Done |
| **7+** | `cargo zigbuild --tests` validated; `text_utils::count_whole_word_matches` consolidated; Git clone flow validated. 78 tests. | ✅ Done |
| **8+** | CI workflow (3 jobs); multi-stage Dockerfile (42 MB); enriched sample_crate (3 real deps); refactor sweeps to `ra_ap_syntax` + `ra_walker`. 37 tools, 78 tests green (33 unit + 45 snapshots), 0 warnings. | ✅ Done |
| **9 (current)** | Dual transport parity (MCP_CONTRACT): `stdio` (default) + Streamable HTTP (`/mcp`, `--mode http --port/--host/--max-threads/--api-keys`). Bearer token auth (`ApiKeyStore`), axum 0.8 integration, `HttpTransportLauncher`, `tests/http_transport_test.rs` (2 tests), `scripts/smoke-http.sh` (E2E check). **84 tests green (37 unit + 2 http + 45 snapshots), 0 warnings.** | ✅ Done |

**Iteration rule**: each iter ends with `cargo test` green + smoke
test end-to-end OK + snapshot diff if applicable. Do not skip to the next
iter without validating the previous one.

## 14. Lessons learned

### iter 7+ (musl test, DRY, Git clone, refactor deferred)

- **`cargo zigbuild --tests --target x86_64-unknown-linux-musl`** compiles the test binary successfully (3m 0s, ~16 MB). BUT the tests fail when running in an Alpine container because they use absolute paths from the macOS host (`/Users/alejandro.ferreira/...`). Standard pattern: **build for cross-target, test on host**. The smoke test is the cross-platform validation. For real cross-platform tests, the tests would need to use `tempfile` or paths relative to the manifest dir.
- **Git clone + archive in podman/Alpine**: the flow DOES work (URL classified correctly, download attempted, `git` CLI call correct). It only fails in the container because minimal `alpine` does not have `ca-certificates` (TLS) nor `git` preinstalled. For end-to-end test: `podman run --rm -v $(pwd):/app:ro alpine sh -c "apk add --no-cache git ca-certificates && /app/..."` and then run the flow. NOT integrated as automated test (requires network and extra container packages).
- **`text_utils::count_whole_word_matches`** is the consolidated version of `count_dep_occurrences` (iter 6). Reusable for future sweeps. 5 unit tests verify: empty needle, basic match (use/extern/path), boundary (rejects substring), multiple matches, underscores in needle. Total: 33 unit tests + 45 snapshots = **78 tests green**.
- **`ra_ap_syntax` refactor (iter 8+)**: the 4 static sweeps of `BugPatternAnalyzer` now use `ra_ap_syntax::SourceFile` (parsed via `parse_all_sources`) + manual walk with `ra_walker::walk_typed` instead of `syn::parse_file` + `syn::visit::Visit`. New helpers: `path_to_string(Path)` and `expr_call_path(CallExpr)`. Removed the 4 visitor structs (`UnsafeVisitor`, `UnwrapVisitor`, `UncheckedVisitor`, `PanicVisitor`). The 5th sweep (`find_unused_dependencies`) remains textual. `syntax.rs` and `rust_analyzer_utils` still use `syn::ItemKind`/`ParsedFile` — scope of another refactor (deferred). **Benefit**: smaller binary (at compile time, syn is no longer needed for the sweeps; release link-time does not change much because syn remains as transitive dep via `quote`/`proc-macro2`).
- **CI workflow (`.github/workflows/ci.yml`)**: 3 jobs — `test-native` (cargo test on ubuntu), `cross-compile-musl` (cargo zigbuild + docker run on alpine), `lint` (fmt + clippy). Clippy without `-D warnings` because of minor doc list warnings (not critical); TODO cleanup.
- **Multi-stage Dockerfile**: stage 1 downloads zig prebuilt 0.13.0 (not in Debian repos), installs `cargo-zigbuild 0.23.0`, compiles with `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 LTO=false` (without this, OOM in `ra_ap_hir_ty`). Stage 2 is `alpine:3.20` with `git` + `ca-certificates` + `tini`, non-root user, ENTRYPOINT the server. Final image: **41.9 MB** (binary 16 MB + Alpine 26 MB). Verified with `podman build -t rust-mcp-server:test .` + `podman run --rm -i rust-mcp-server:test` (initializes correctly).
- **Enriched sample_crate**: 3 real deps (`cfg-if`, `once_cell`, `pretty_assertions`) + real usage in `is_release()` (cfg-if), `cached_value()` (once_cell), and `test_*` (pretty_assertions). `list_dependencies` reports them correctly with scope (normal/dev). `find_unused_dependencies` still gives 0 findings (all used). Total: 4 fixture tests + 78 repo tests.
- **`ra_ap_syntax` does not have a `Visit`/`Visitor` trait** (unlike `syn` which auto-generates one). Refactoring the 4 static sweeps (unsafe/unwrap/unchecked/panic) to `ra_ap_syntax` would require manual AST walk (~200-300 LoC) with no functional benefit (behavior is identical). **Deferred** to iter 8+ if a real use case arises.

### iter 6+ (find_unused_dependencies + shims)

- **Serde does not automatically convert `dev_dependencies` (snake_case) to `dev-dependencies` (kebab-case)**. You must add `#[serde(rename = "dev-dependencies")]` on the field. Same for `build-dependencies`. Without this rename, dev/build deps are not deserialized and the tool reports 0 findings even if the Cargo.toml has declared deps.
- **The test fixture had dep names in COMMENTS** (`//! The `Cargo.toml` declares `serde`, ...`). The whole-word search detected them as "used" → false positive. Solution: fixture with clean code, no comments mentioning dep names.
- **rustup shims setup on macOS**: the actual rustup binary at `homebrew/Cellar/rustup/.../libexec/bin/rustup` is the real polymorphic binary (11 MB). The one at `homebrew/bin/rustup` is a bash wrapper (203 bytes) that ignores `argv[0]`. **You must copy the `libexec/bin/rustup`** (not the wrapper) to `~/.cargo/bin/{cargo,rustc,rustup,...}`. `cp` not `ln -s` (symlinks preserve `argv[0]` pointing to `rustup`).
- **Path traversal with `..`**: `Path::starts_with` does NOT normalize `..` (returns `true` for `foo/../bar` when compared to `foo`). Must normalize the path manually with a `Component::ParentDir` walker.

### iter 5 (cross-compile musl + fat binary)

- **Toolchain conflict on macOS**: there are two `cargo` — Homebrew (`/usr/local/bin/cargo`) and rustup (`~/.cargo/bin/cargo`). `rustup target add` installs std for rustup's, but if you use Homebrew's it does not see them. **Solution**: `PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"` or use rustup's cargo directly.
- **`cargo zigbuild` does not support `+toolchain`**. The workaround is the PATH override.
- **Build time**: x86_64-unknown-linux-musl took 3m 56s, aarch64 took 4m 02s (cold builds; subsequent builds will be seconds).
- **Binary size**: 16 MB (x86_64) and 14 MB (aarch64). Stripped + statically linked (no glibc dependencies). Portable to any modern Linux.
- **`podman run` does not work without `podman machine start`**. First time you need to initialize (`podman machine init`) and start (`podman machine start`). The "Cannot connect to Podman" error indicates VM not started.
- **Binary test in podman**: `alpine` minimal image does not have `python3`. For smoke tests you can pipe JSON-RPC directly via heredoc. The server responds correctly with `initialize`, `tools/list`, `tools/call` — `ra_ap_*` works on musl without issues.
- **Warning during build**: `linker stderr: ignoring deprecated linker optimization setting '1'` comes from `cargo-zigbuild` + `zig 0.16` + the `-C link-arg=-z` flag that is no longer supported. Does not affect the binary.

### Post-iter 4.5 improvements (dead code `#[test]`, Git clone, snapshots)

- **`ra_ap_syntax::ast::Attr`** (not `Attribute`) is the type. And `Attr::path()` returns `Option<Path>`, not `Path` directly.
- **`Path::segment()`** (singular) returns `Option<PathSegment>`, and `PathSegment::name_ref()` returns `Option<NameRef>`. No `segments()` (iterator) — only `qualifier()` and `segment()`.
- **`Node::attrs()`** returns `AstChildren<Attr>`, not `&[Attr]`. Must `.collect::<Vec<_>>()`.
- **For `find_dead_code`**: enriching `SymbolLocation` with `implicit_references: ImplicitReferences { test, derive, no_mangle, cfg, allow }` correctly skips `#[test] fn test_foo()`. The fixture goes from 3 dead items to 1 (only `mod tests` remains — `#[cfg(test)]` on the `mod` requires checking attrs of the `mod`, iter 5+).
- **Real Git clone with `git` CLI is trivial** vs `gix`. `std::process::Command::new("git").args(["clone", "--depth", "1", url, dest])` in a `spawn_blocking` resolves the iter 2 deferral. The unit test `resolve_git_invalid_url` validates the error path (invalid URL → `GitCloneFailed`). The happy path with a real URL works but is not tested automatically (requires network).
- **Cargo warnings**: the remaining 3 `dead_code` items (`item_name`, `is_pub`, `has_skip_attr`, syn helpers from iter 4) no longer apply after the refactoring to `SymbolLocation`-based. Code now has 0 warnings on `cargo check`.

### iter 4.5 (semantic dead code with `ra_ap_*`)

- **`ra_ap_syntax::TextRange` → `ra_ap_ide::TextSize` mapping**: both are `pub struct TextSize(pub u32)` but **from different crates**. They are the same numeric value but different types. Fortunately, the conversion is automatic `Into` because they have the same structural shape.
- **`HasVisibility::visibility()` returns `Option<Visibility>`, not `Visibility` directly**. And `Visibility` is a struct (not enum) — how to tell if it is public is by checking `pub_token().is_some()`.
- **`find_all_refs` from the identifier position** includes the declaration in `references`. The rule for "dead" is: `total_refs == 0` (not even self-references, recursion, or macro usage).
- **Items in `mod tests { ... }`** are dead code (zero references from Rust) — correct, because `#[test]` is not visible to the semantic engine as a reference. If we wanted to exclude `#[test]`, the resolver would need to enrich `SymbolLocation` with attributes (iter 5+).
- **Fixture item detection**: `test_add`, `test_multiply`, and `mod tests` itself correctly appear as dead code. `add` and `multiply` (pubs) are excluded.

### iter 4 (BugPatternAnalyzer, 5 sweeps)

- **`syn 2.0` renamed `Item::TypeAlias` to `Item::Type`**. Search the code with grep before migrating.
- **`syn::Item` and `ra_ap_syntax::ast::Item` are different types** though their APIs are similar. For a sweep that uses both (syn for parsing + ra_ap for query), you must pick one as canonical. Decision: we use `syn::Item` for the 4 static sweeps (parsing) and `ra_ap_syntax::ast::Item` (via `analysis.parse(file_id)`) only where strictly necessary.
- **`proc_macro2::Span` → `ra_ap_ide::TextSize` mapping is not trivial** — `proc_macro2::Span` does not have a stable public `start()` method. `LineIndex` is the correct way but requires setup. For iter 4, `find_dead_code` avoids this by using textual heuristics (whole-word search) instead of `find_all_refs`. Semantic version → iter 4.5.
- **`syn::visit::Visit` requires `Visit<'ast>`** with lifetime `'ast`, not `'_`. If the visitor has its own `Vec<Finding>` (not `&mut Vec`), there are no lifetime issues.
- **`tokio::task::spawn_blocking` with sync analysis** is the correct pattern: the async tool launches the blocking analysis on a thread pool, then formats the result. Do not use `spawn_blocking` inside the analyzer (it is sync).
- **Whole-word matching** in Rust: to count occurrences of a name without false positives (e.g. `add` inside `address`), you must check boundaries (alphanumeric/underscore characters).

### iter 3.6 (4 remaining semantic tools)

- **`goto_implementation` takes `&GotoImplementationConfig`**, not `GotoImplementationConfig` by value. Clear compilation error, easy to fix.
- **`SearchScope` does not have `::default()`** — use `None` (which means the whole workspace) or explicit constructors like `SearchScope::whole_project()`.
- **`find_all_refs` returns `Option<Vec<ReferenceSearchResult>>`** (a vec of results, one per declaration site). Must iterate `result.references` which is `IntMap<FileId, Vec<(TextRange, ReferenceCategory)>>` and NOT `HashMap`. The `references` field itself is private to the `ide_db` crate.
- **`ReferenceSearchResult.declaration`** points to the declaration (we filter it out to not include it in callers/refs).
- **`get_type_hierarchy` on a function** correctly returns "No type hierarchy info" — `goto_implementation` only applies to traits/types. It is an honest response, not an error.
- **Callers in `assert_eq!(foo(...))`** are not always found by the semantic engine, because the call is inside the macro expansion of `assert_eq!`. The `function_callers` tool (text-based, iter 2) finds them; the semantic one does not always. Documented.

### iter 3.5 (symbol resolver + `resolve_type`)

- **`SymbolResolver` + `goto_definition` works end-to-end** with ~280 LoC. The key piece was `AnalysisHost` which now exposes `files: HashMap<String, FileId>` (virtual_path → file_id mapping), allowing conversion of symbolic names to `FilePosition` with offset.
- **`ImplItem::Fn` and `AssocItem::Fn` are different** — for `impl` methods, you must iterate `i.assoc_item_list().and_then(|l| l.assoc_items())` and filter by `AssocItem::Fn`. `i.assoc_items()` direct does not exist.
- **For nested `mod foo { ... }` you must recurse** — items inside a `mod` are in `m.item_list()` and are processed the same as top-level. Without this, `test_add` is not indexed (lives in `mod tests`).
- **`Analysis::parse(file_id)` returns `Cancellable<SourceFile>`** which is `Result<SourceFile, Cancelled>`. In practice it is never cancelled, but you must match.
- **`FileId::index()`** exists; `u32::from(FileId)` does not.

### iter 3 (`ra_ap_*` minimal integration)

- **The `ra_ap_*` 0.0.343 from 4t145 compile cleanly** with Rust 1.96 — the 5-15 min expectation was pessimistic: they compile in ~30s (already compiled) or ~2.5 min (cold). The binary adds 8.5 MB stripped (from 3.5 to 12 MB), far less than the 80-150 MB I estimated.
- **The `load-cargo` crate is not published by 4t145** — you cannot load a real Cargo workspace. The workaround (`ChangeFixture` with `src/lib.rs` mapped to `/lib.rs`) works for single-crate without externs, which covers the fixture case and most small crates.
- **`Analysis::db` is not public** — to populate the `RootDatabase` you must create it with `RootDatabase::default()`, apply the change with `db.apply_change(...)`, and then `AnalysisHost::with_database(db)`. Clean pattern.
- **The `ChangeFixture` fixture format requires exact `/lib.rs`** — sub-module files follow their real paths (`/src/foo.rs`), but the crate root must be `/lib.rs` or `/main.rs`. It is an easy detail to miss.

### iter 2 (syn + toml + Git/archive loading)

- **gix API is complex** — the `PrepareFetch → fetch_then_checkout → main_worktree` flow requires implementing `NestedProgress` or using a wrapper. Deferred to iter 3 (where it is orchestrated alongside `ra_ap_*`).
- **`Url::parse` does not understand `git@host:user/repo`** — the scheme with `:` before the path confuses the parser. Shortcut: `if source.starts_with("git@")` before the parser.
- **Snapshot determinism**: any timestamp (`loaded_at`, `expiry`) in the Markdown breaks byte-for-byte. Tools MUST NOT include them — the LLM does not need them and they break the golden test. If needed, put them in a separate `project_metadata` tool with `include_timestamps: bool`.
- **`quote` and `prettyplease`** are necessary deps to convert `syn::Type`/`syn::Signature` to string. They were transitive; I made them explicit.
- **HashMap does not guarantee order** — `cargo_manifest.features()` sorts by name so the output is deterministic.
- **`tokio::task::spawn_blocking`** with sync closure does not allow `.await` inside — the callback is purely synchronous. Pattern: pass values by move, do I/O inside, return Result.
