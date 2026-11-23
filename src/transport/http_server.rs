use std::future::Future;
use std::sync::Arc;

use anyhow::Context;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::IntoResponse, Router};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::project_manager::ProjectManager;
use crate::server::{RustMcpServer, VERSION};
use crate::transport::api_key_store::ApiKeyStore;

pub struct HttpTransportLauncher;

impl HttpTransportLauncher {
    pub async fn start(
        manager: Arc<ProjectManager>,
        host: &str,
        port: u16,
        api_key_store: Option<ApiKeyStore>,
    ) -> anyhow::Result<()> {
        let listener = tokio::net::TcpListener::bind(format!("{host}:{port}"))
            .await
            .with_context(|| format!("failed to bind HTTP listener on {host}:{port}"))?;

        let shutdown = async {
            let _ = tokio::signal::ctrl_c().await;
            info!("Received Ctrl+C, shutting down HTTP transport...");
        };

        Self::start_with_listener(listener, manager, api_key_store, shutdown).await
    }

    pub async fn start_with_listener<F>(
        listener: tokio::net::TcpListener,
        manager: Arc<ProjectManager>,
        api_key_store: Option<ApiKeyStore>,
        shutdown: F,
    ) -> anyhow::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let local_addr = listener.local_addr()?;
        let ct = CancellationToken::new();

        let config = StreamableHttpServerConfig::default()
            .with_stateful_mode(true)
            .with_cancellation_token(ct.clone())
            .disable_allowed_hosts()
            .disable_allowed_origins();

        let srv_manager = manager.clone();
        let service: StreamableHttpService<RustMcpServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok(RustMcpServer::new(srv_manager.clone())),
                Default::default(),
                config,
            );

        let mut app = Router::new().nest_service("/mcp", service);

        if let Some(store) = api_key_store.clone() {
            let store = Arc::new(store);
            app = app.layer(axum::middleware::from_fn(
                move |req: Request, next: Next| {
                    let store = store.clone();
                    async move {
                        if req.uri().path().ends_with("/mcp") {
                            let auth_header = req
                                .headers()
                                .get(axum::http::header::AUTHORIZATION)
                                .and_then(|v| v.to_str().ok());

                            let authorized = match auth_header {
                                Some(h) if h.starts_with("Bearer ") => {
                                    let token = h["Bearer ".len()..].trim();
                                    store.validate(token)
                                }
                                _ => false,
                            };

                            if !authorized {
                                return (
                                    StatusCode::UNAUTHORIZED,
                                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                                    r#"{"error":"unauthorized"}"#,
                                )
                                    .into_response();
                            }
                        }

                        next.run(req).await
                    }
                },
            ));
        }

        info!(
            "MCP server listening on http://{}/mcp (Streamable HTTP)",
            local_addr
        );
        info!(
            "rust-mcp-server v{} (HTTP mode, auth: {})",
            VERSION,
            if let Some(ref s) = api_key_store {
                format!("{} api-key(s)", s.count())
            } else {
                "disabled".to_string()
            }
        );

        let ct_shutdown = ct.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown.await;
                ct_shutdown.cancel();
            })
            .await
            .with_context(|| "error running axum HTTP server")?;

        info!("rust-mcp-server (HTTP) shut down complete");
        Ok(())
    }
}
