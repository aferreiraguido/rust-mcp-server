use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use rust_mcp_server::{ApiKeyStore, HttpTransportLauncher, ProjectManager};

fn parse_mcp_response(text: &str) -> serde_json::Value {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        return v;
    }
    for line in text.lines().rev() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("data:") {
            let rest = rest.trim();
            if rest.starts_with('{') {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                    return v;
                }
            }
        }
    }
    serde_json::json!({})
}

#[tokio::test]
async fn test_http_transport_lifecycle() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}/mcp");

    let manager = Arc::new(ProjectManager::new());
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let server_handle = tokio::spawn(async move {
        let _ = HttpTransportLauncher::start_with_listener(listener, manager, None, async move {
            let _ = shutdown_rx.await;
        })
        .await;
    });

    let client = reqwest::Client::new();

    // 1. Initialize
    let init_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {
                "name": "test-http-client",
                "version": "1.0"
            }
        }
    });

    let init_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&init_payload)
        .send()
        .await?;

    assert_eq!(init_resp.status(), 200);

    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    assert!(session_id.is_some(), "Expected Mcp-Session-Id header");
    let session_id = session_id.unwrap();

    let init_text = init_resp.text().await?;
    let init_body = parse_mcp_response(&init_text);
    let server_name = init_body["result"]["serverInfo"]["name"]
        .as_str()
        .unwrap_or("");
    assert_eq!(server_name, "rust-mcp-server");

    // 2. notifications/initialized
    let notify_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&notify_payload)
        .send()
        .await?;

    // 3. tools/list
    let list_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let list_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&list_payload)
        .send()
        .await?;

    assert_eq!(list_resp.status(), 200);
    let list_text = list_resp.text().await?;
    let list_body = parse_mcp_response(&list_text);
    let tools = list_body["result"]["tools"].as_array().unwrap();
    assert!(tools
        .iter()
        .any(|t| t["name"] == "load_rust_project" || t["name"] == "list_loaded_projects"));

    // 4. tools/call (list_loaded_projects)
    let call_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "list_loaded_projects",
            "arguments": {}
        }
    });

    let call_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .json(&call_payload)
        .send()
        .await?;

    assert_eq!(call_resp.status(), 200);
    let call_text = call_resp.text().await?;
    let call_body = parse_mcp_response(&call_text);
    let content = &call_body["result"]["content"];
    assert!(content.is_array());

    // Shutdown
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;

    Ok(())
}

#[tokio::test]
async fn test_http_transport_auth() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}/mcp");

    let manager = Arc::new(ProjectManager::new());
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let api_keys = ApiKeyStore::load("sk-secret-key-1, sk-secret-key-2").unwrap();

    let server_handle = tokio::spawn(async move {
        let _ = HttpTransportLauncher::start_with_listener(
            listener,
            manager,
            Some(api_keys),
            async move {
                let _ = shutdown_rx.await;
            },
        )
        .await;
    });

    let client = reqwest::Client::new();
    let init_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1" }
        }
    });

    // 1. Missing Authorization header -> 401
    let resp_anon = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&init_payload)
        .send()
        .await?;
    assert_eq!(resp_anon.status(), 401);
    let body_anon = resp_anon.text().await?;
    assert!(body_anon.contains("unauthorized"));

    // 2. Wrong Authorization token -> 401
    let resp_bad = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Authorization", "Bearer invalid-token")
        .json(&init_payload)
        .send()
        .await?;
    assert_eq!(resp_bad.status(), 401);

    // 3. Valid Authorization token -> 200
    let resp_ok = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Authorization", "Bearer sk-secret-key-1")
        .json(&init_payload)
        .send()
        .await?;
    assert_eq!(resp_ok.status(), 200);

    // Shutdown
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;

    Ok(())
}
