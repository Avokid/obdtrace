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
        },
        "cli": {
            "claude-code": claude_code_command(a.port),
        },
        "config_paths": {
            "claude-desktop": config_path("claude-desktop").map(|p| p.to_string_lossy().to_string()),
            "vscode": config_path("vscode").map(|p| p.to_string_lossy().to_string()),
        }
    }))
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
    fn client_setup_uses_the_running_apps_port() {
        let port = 18888;
        assert_eq!(snippet("vscode", port)["servers"]["obdtrace"]["env"]["OBDTRACE_PORT"], "18888");
        assert_eq!(snippet("claude-desktop", port)["mcpServers"]["obdtrace"]["env"]["OBDTRACE_PORT"], "18888");
        assert!(claude_code_command(port).contains("--env OBDTRACE_PORT=18888 obdtrace --"));
    }
}
