//! Entry point de `rust-mcp-server`.
//!
//! 1. Inicializa logging (stderr + fichero, configurable por env).
//! 2. Construye el `ProjectManager`.
//! 3. Construye el `RustMcpServer`.
//! 4. Llama a `start()` — bloquea hasta que el stdio se cierre.

use anyhow::Context;
use rust_mcp_server::{ProjectManager, RustMcpServer};
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let manager = Arc::new(ProjectManager::new());
    let server = RustMcpServer::new(manager);

    server
        .start()
        .await
        .with_context(|| "rust-mcp-server crashed during start()")?;

    Ok(())
}

fn init_tracing() {
    let log_file = std::env::var("RUST_MCP_LOG_FILE")
        .unwrap_or_else(|_| "/tmp/rust_mcp_server.log".to_string());
    let level = std::env::var("RUST_MCP_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .ok();

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&level));

    let registry = tracing_subscriber::registry().with(env_filter);

    if let Some(file) = file {
        let file_layer = fmt::layer()
            .with_writer(Arc::new(file))
            .with_target(true)
            .with_ansi(false);
        registry.with(file_layer).init();
    } else {
        let stderr_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(true)
            .with_ansi(false);
        registry.with(stderr_layer).init();
    }
}
