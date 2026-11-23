//! Entry point de `rust-mcp-server`.
//!
//! Soporta dual transport:
//! - `stdio` (por defecto): modo subproceso para clientes locales (OpenCode, Claude Desktop).
//! - `http` (`--mode http`): modo Streamable HTTP en `/mcp` con soporte opcional de Bearer auth (`--api-keys`).

use anyhow::Context;
use rust_mcp_server::{ApiKeyStore, HttpTransportLauncher, ProjectManager, RustMcpServer};
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Debug, Clone)]
struct CliConfig {
    mode: String,
    port: u16,
    host: String,
    _max_threads: usize,
    api_keys: Option<String>,
}

impl CliConfig {
    fn parse() -> Self {
        let mut mode = "stdio".to_string();
        let mut port = 8080u16;
        let mut host = "0.0.0.0".to_string();
        let mut max_threads = 16usize;
        let mut api_keys = None;

        let mut iter = std::env::args().skip(1);
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--mode" => {
                    if let Some(v) = iter.next() {
                        mode = v.to_ascii_lowercase();
                    }
                }
                "--port" => {
                    if let Some(v) = iter.next() {
                        if let Ok(p) = v.parse::<u16>() {
                            port = p;
                        }
                    }
                }
                "--host" => {
                    if let Some(v) = iter.next() {
                        host = v;
                    }
                }
                "--max-threads" => {
                    if let Some(v) = iter.next() {
                        if let Ok(t) = v.parse::<usize>() {
                            max_threads = t;
                        }
                    }
                }
                "--api-keys" => {
                    if let Some(v) = iter.next() {
                        api_keys = Some(v);
                    }
                }
                _ => {}
            }
        }

        Self {
            mode,
            port,
            host,
            _max_threads: max_threads,
            api_keys,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = CliConfig::parse();
    let manager = Arc::new(ProjectManager::new());

    if cli.mode == "http" {
        let api_key_store = if let Some(ref spec) = cli.api_keys {
            Some(
                ApiKeyStore::load(spec)
                    .with_context(|| format!("failed to load API keys from '{spec}'"))?,
            )
        } else {
            None
        };

        HttpTransportLauncher::start(manager, &cli.host, cli.port, api_key_store)
            .await
            .with_context(|| "rust-mcp-server crashed during HTTP transport")?;
    } else {
        let server = RustMcpServer::new(manager);
        server
            .start()
            .await
            .with_context(|| "rust-mcp-server crashed during start()")?;
    }

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
