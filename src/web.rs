//! Local web UI and HTTP API. Bound to 127.0.0.1 only.

use crate::hub::{Hub, APP_VERSION};
use crate::locales;
use crate::prompts;
use crate::tools::{self, Ctx, Origin};
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::Arc;

const INDEX: &str = include_str!("../assets/index.html");
pub(crate) const COPILOT_TOOLS: &[&str] = &[
    "connection_status", "list_supported_sensors", "get_live_snapshot", "read_live_data",
    "read_dtcs", "read_freeze_frame", "read_readiness", "read_vin", "list_vehicles",
    "list_sessions", "get_session", "get_session_events", "query_session", "list_fault_annotations",
    "list_recordings", "query_recording", "get_recording_summary", "compare_recordings",
    "find_similar_conditions",
];

#[derive(Clone)]
struct App {
    hub: Arc<Hub>,
    port: u16,
    browser: String,
}

fn host_ok(h: &HeaderMap, port: u16) -> bool {
    let host = h.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn origin_ok(h: &HeaderMap, port: u16) -> bool {
    match h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(o) => o == format!("http://127.0.0.1:{port}") || o == format!("http://localhost:{port}"),
        None => false,
    }
}

async fn guard(State(app): State<App>, req: Request, next: Next) -> Response {
    let h = req.headers();
    if !host_ok(h, app.port) {
        return (StatusCode::FORBIDDEN, "bad host").into_response();
    }
    if req.uri().path() == "/ws" || req.method() != axum::http::Method::GET {
        if !origin_ok(h, app.port) {
            return (StatusCode::FORBIDDEN, "bad origin").into_response();
        }
    }
    let path = req.uri().path();
    let public = matches!(path, "/api/ping" | "/api/languages") || path.starts_with("/api/locales/");
    if path == "/ws" || (path.starts_with("/api/") && !public) {
        let token = if path == "/ws" {
            h.get(header::SEC_WEBSOCKET_PROTOCOL).and_then(|h| h.to_str().ok())
                .and_then(|protocols| protocols.split(',').map(str::trim).find_map(|s| s.strip_prefix("auth.")))
        } else {
            h.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()).and_then(|s| s.strip_prefix("Bearer "))
        };
        if token != Some(app.browser.as_str()) { return StatusCode::UNAUTHORIZED.into_response(); }
    }
    let mut res = next.run(req).await;
    res.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    res.headers_mut().insert(header::CONTENT_SECURITY_POLICY, "frame-ancestors 'none'; base-uri 'none'; object-src 'none'".parse().unwrap());
    res
}

fn exe_path() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "obdtrace".into())
}

fn config_path(client: &str) -> Option<std::path::PathBuf> {
    if client == "copilot-cli" {
        return std::env::var_os("COPILOT_HOME")
            .filter(|path| !path.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| dirs_home().map(|home| home.join(".copilot")))
            .map(|dir| dir.join("mcp-config.json"));
    }
    let home = dirs_home()?;
    match client {
        "claude-desktop" => {
            if cfg!(target_os = "windows") {
                Some(std::path::PathBuf::from(std::env::var("APPDATA").ok()?).join("Claude/claude_desktop_config.json"))
            } else if cfg!(target_os = "macos") {
                Some(home.join("Library/Application Support/Claude/claude_desktop_config.json"))
            } else {
                Some(home.join(".config/Claude/claude_desktop_config.json"))
            }
        }
        "vscode" => {
            if cfg!(target_os = "windows") {
                Some(std::path::PathBuf::from(std::env::var("APPDATA").ok()?).join("Code/User/mcp.json"))
            } else if cfg!(target_os = "macos") {
                Some(home.join("Library/Application Support/Code/User/mcp.json"))
            } else {
                Some(home.join(".config/Code/User/mcp.json"))
            }
        }
        _ => None,
    }
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(Into::into)
}

fn snippet(client: &str, port: u16) -> Value {
    let mut entry = json!({
        "command": exe_path(), "args": ["mcp"], "env": {"OBDTRACE_PORT": port.to_string()}
    });
    if std::env::var_os("OBDTRACE_DATA_DIR").is_some() {
        if let Ok(dir) = crate::store::data_dir().canonicalize() {
            entry["env"]["OBDTRACE_DATA_DIR"] = json!(dir);
        }
    }
    match client {
        "vscode" => {
            entry["type"] = json!("stdio");
            json!({"servers": {"obdtrace": entry}})
        }
        "copilot-cli" => {
            entry["type"] = json!("local");
            entry["tools"] = json!(COPILOT_TOOLS);
            entry["env"]["OBDTRACE_READ_ONLY"] = json!("1");
            json!({"mcpServers": {"obdtrace-auto": entry}})
        }
        _ => json!({"mcpServers": {"obdtrace": entry}}),
    }
}

fn claude_code_command(port: u16) -> String {
    format!("claude mcp add --transport stdio --env OBDTRACE_PORT={port} obdtrace -- \"{}\" mcp", exe_path())
}

async fn index() -> Html<&'static str> {
    Html(INDEX)
}

async fn languages() -> Json<Value> {
    Json(json!(locales::languages().collect::<Vec<_>>()))
}

async fn catalog(Path(language): Path<String>) -> Response {
    match locales::source(&language) {
        Some(source) => ([(header::CONTENT_TYPE, "application/json")], source).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn i18next() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], include_str!("../assets/vendor/i18next.min.js"))
}

async fn ping() -> Json<Value> {
    Json(json!({"app": "obdtrace", "version": APP_VERSION}))
}

async fn status(State(a): State<App>) -> Json<Value> {
    Json(a.hub.status())
}

async fn prompts_api() -> Json<Value> {
    Json(prompts::for_ui())
}

async fn render_prompt(Path(name):Path<String>,Json(args):Json<Value>)->Json<Value>{
    match prompts::mcp_get(&name,args["language"].as_str().unwrap_or("en"),&args){
        Some(v)=>Json(json!({"ok":true,"text":v["messages"][0]["content"]["text"]})),
        None=>Json(json!({"ok":false,"error":"unknown prompt"})),
    }
}

async fn tool(State(a): State<App>, Path(name): Path<String>, body: Option<Json<Value>>) -> Json<Value> {
    let args = body.map(|b| b.0).unwrap_or_else(|| json!({}));
    let ctx = Ctx { origin: Origin::Ui, confirmed: args["confirm"].as_bool() == Some(true), mcp_clear_enabled: false };
    match tools::call(&a.hub, &name, args, ctx).await {
        Ok(v) => Json(json!({"ok": true, "result": v})),
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

async fn onboarding(State(a): State<App>) -> Json<Value> {
    let os = if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" };
    Json(json!({
        "os": os,
        "binary": exe_path(),
        "version": APP_VERSION,
        "snippets": {
            "claude-desktop": snippet("claude-desktop", a.port),
            "vscode": snippet("vscode", a.port),
            "copilot-cli": snippet("copilot-cli", a.port),
        },
        "cli": {
            "claude-code": claude_code_command(a.port),
        },
        "config_paths": {
            "claude-desktop": config_path("claude-desktop").map(|p| p.to_string_lossy().to_string()),
            "vscode": config_path("vscode").map(|p| p.to_string_lossy().to_string()),
            "copilot-cli": config_path("copilot-cli").map(|p| p.to_string_lossy().to_string()),
        }
    }))
}

async fn write_config(State(a): State<App>, headers: HeaderMap, Json(v): Json<Value>) -> Json<Value> {
    let client = v["client"].as_str().unwrap_or("");
    let Some(path) = config_path(client) else {
        return Json(json!({"ok": false, "error": "unknown client"}));
    };
    if client == "copilot-cli" {
        if !headers.contains_key(header::ORIGIN) {
            return Json(json!({"ok": false, "error": "Copilot CLI setup must be requested from the local browser UI"}));
        }
        let status = a.hub.status();
        let active = status["adapter"]["connected"] == true || status["adapter"]["connecting"] == true;
        if active && status["target"].as_str().is_some_and(|target| target.split(':').next() == Some("mock")) {
            return Json(json!({"ok": false, "error": "Disconnect the demo adapter before setting up Copilot CLI for a real car"}));
        }
    }
    match write_client_config(&path, client, a.port) {
        Ok(()) => Json(json!({"ok": true, "path": path.to_string_lossy()})),
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

fn write_client_config(path: &std::path::Path, client: &str, port: u16) -> Result<()> {
    let mut doc = read_config(path)?;
    let key = if client == "vscode" { "servers" } else { "mcpServers" };
    let name = if client == "copilot-cli" { "obdtrace-auto" } else { "obdtrace" };
    let entry = snippet(client, port)[key][name].clone();
    let root = doc.as_object_mut().ok_or_else(|| anyhow::anyhow!("existing config is not a JSON object; not touched"))?;
    let servers = root.entry(key).or_insert_with(|| json!({})).as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("existing {key} value is not a JSON object; not touched"))?;
    if servers.get(name) == Some(&entry) {
        return Ok(());
    }
    servers.insert(name.into(), entry);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::copy(path, path.with_extension("json.bak")) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    std::fs::write(path, serde_json::to_string_pretty(&doc)?)?;
    Ok(())
}

fn read_config(path: &std::path::Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok(json!({})),
        Ok(s) => serde_json::from_str(&s).map_err(|e| anyhow::anyhow!("existing config is not valid JSON ({e}); not touched")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
    }
}

async fn ws(State(a): State<App>, up: WebSocketUpgrade) -> Response {
    up.protocols(["obdtrace"]).on_upgrade(move |sock| ws_loop(a, sock))
}

async fn ws_loop(a: App, mut sock: WebSocket) {
    let mut rx = a.hub.subscribe();
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(s) => { if sock.send(Message::Text(s.into())).await.is_err() { break; } }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            m = sock.recv() => if !matches!(m, Some(Ok(_))) { break; },
        }
    }
}

fn router(hub: Arc<Hub>, port: u16, browser: String) -> Router {
    let app = App { hub, port, browser };
    Router::new()
        .route("/", get(index))
        .route("/assets/i18next.js", get(i18next))
        .route("/api/languages", get(languages))
        .route("/api/locales/{language}", get(catalog))
        .route("/api/ping", get(ping))
        .route("/api/status", get(status))
        .route("/api/prompts", get(prompts_api))
        .route("/api/prompt/{name}",post(render_prompt))
        .route("/api/onboarding", get(onboarding))
        .route("/api/tool/{name}", post(tool))
        .route("/api/write-config", post(write_config))
        .route("/ws", get(ws))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}

pub async fn bind(port: u16) -> Result<tokio::net::TcpListener> {
    Ok(tokio::net::TcpListener::bind(("127.0.0.1", port)).await?)
}

pub async fn serve_listener(listener: tokio::net::TcpListener, hub: Arc<Hub>, port: u16, browser: String) -> Result<()> {
    axum::serve(listener, router(hub, port, browser)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_requires_local_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "127.0.0.1:8765".parse().unwrap());
        assert!(!origin_ok(&headers, 8765));
        headers.insert(header::ORIGIN, "http://evil.example".parse().unwrap());
        assert!(!origin_ok(&headers, 8765));
        headers.insert(header::ORIGIN, "http://localhost:8765".parse().unwrap());
        assert!(origin_ok(&headers, 8765));
    }

    #[test]
    fn config_read_errors_are_not_treated_as_missing() {
        let path = std::env::temp_dir().join(format!("obdtrace-invalid-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(read_config(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), [0xff, 0xfe]);
        assert_eq!(read_config(&path.with_extension("missing")).unwrap(), json!({}));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn client_setup_uses_the_running_apps_port() {
        let port = 18888;
        assert_eq!(snippet("vscode", port)["servers"]["obdtrace"]["env"]["OBDTRACE_PORT"], "18888");
        assert_eq!(snippet("claude-desktop", port)["mcpServers"]["obdtrace"]["env"]["OBDTRACE_PORT"], "18888");
        assert!(claude_code_command(port).contains("--env OBDTRACE_PORT=18888 obdtrace --"));
    }

    #[test]
    fn copilot_config_adds_only_read_access_and_preserves_existing_servers() {
        let path = std::env::temp_dir().join(format!("obdtrace-copilot-{}.json", uuid::Uuid::new_v4()));
        let previous = json!({"mcpServers": {"obd2-demo": {"command": "demo"}, "other": {"command": "other"}}, "extra": true});
        std::fs::write(&path, previous.to_string()).unwrap();

        write_client_config(&path, "copilot-cli", 18888).unwrap();
        let actual = read_config(&path).unwrap();
        let mut expected = previous.clone();
        expected["mcpServers"]["obdtrace-auto"] = snippet("copilot-cli", 18888)["mcpServers"]["obdtrace-auto"].clone();
        assert_eq!(actual, expected);
        let server = &actual["mcpServers"]["obdtrace-auto"];
        assert_eq!(server["env"]["OBDTRACE_PORT"], "18888");
        assert_eq!(server["env"]["OBDTRACE_READ_ONLY"], "1");
        let names = server["tools"].as_array().unwrap();
        for required in ["connection_status", "read_dtcs", "query_session"] {
            assert!(names.contains(&json!(required)));
        }
        for mutating in ["*", "connect", "clear_dtcs", "start_recording", "add_fault_annotation"] {
            assert!(!names.contains(&json!(mutating)));
        }
        let definitions = tools::definitions();
        assert!(names.iter().all(|name| definitions.iter().any(|tool| &tool["name"] == name)));

        let backup = path.with_extension("json.bak");
        write_client_config(&path, "copilot-cli", 18888).unwrap();
        assert_eq!(read_config(&backup).unwrap(), previous);
        std::fs::write(&path, r#"{"mcpServers":[]}"#).unwrap();
        assert!(write_client_config(&path, "copilot-cli", 18888).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), r#"{"mcpServers":[]}"#);

        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(backup).unwrap();
    }
}
