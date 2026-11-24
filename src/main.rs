mod bridge;
mod elm;
mod history;
mod hub;
mod identity;
mod locales;
mod mcp;
mod obd;
mod prompts;
mod redact;
mod store;
mod tools;
mod transport;
mod web;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::IsTerminal;

#[derive(Parser)]
#[command(
    name = "obdtrace",
    version,
    about = "OBD-II MCP server for ELM327 adapters (BLE, serial, WiFi) with a local setup app"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// MCP server on stdio (what AI clients launch)
    Mcp,
    /// Open the local app (setup wizard, live data, recordings)
    App {
        #[arg(long)]
        no_browser: bool,
    },
    /// List adapters visible to this computer
    Scan,
    /// Check that the MCP server answers correctly
    Doctor,
    /// Run the app against a simulated car
    Demo {
        #[arg(long)]
        no_browser: bool,
    },
}

fn port() -> u16 {
    std::env::var("OBDTRACE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8765)
}

fn open_browser(url: &std::path::Path) {
    let r = if cfg!(target_os = "windows") {
        std::process::Command::new("explorer.exe").arg(url).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if r.is_err() {
        eprintln!("Open this private file in your browser: {}", url.display());
    }
}

async fn run_app(no_browser: bool, demo: bool) -> Result<()> {
    let p = port();
    bridge::prepare_dir()?;
    let url = bridge::launch_file(p);
    if bridge::Peer::discover(p).await.is_ok() {
        eprintln!("OBDTrace is already running; open {}", url.display());
        if !no_browser {
            open_browser(&url);
        }
        return Ok(());
    }
    let listener = web::bind(p).await?;
    let hub = hub::Hub::open_default()?;
    if demo {
        hub.connect("mock:audi_demo").await?;
    }
    let owner = bridge::Owner::start(hub.clone(), p).await?;
    eprintln!(
        "OBDTrace: open {} in your browser (Ctrl+C to quit)",
        url.display()
    );
    if !no_browser {
        open_browser(&url);
    }
    let result = tokio::select! {
        result=web::serve_listener(listener,hub.clone(),p,owner.browser.clone())=>result,
        signal=tokio::signal::ctrl_c()=>signal.map_err(Into::into),
    };
    hub.disconnect().await?;
    result
}

async fn run_mcp() -> Result<()> {
    let p = port();
    bridge::prepare_dir()?;
    let listener = match web::bind(p).await {
        Ok(listener) => listener,
        Err(bind_error) => {
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if let Ok(peer) = bridge::Peer::discover(p).await {
                    return mcp::run_stdio(mcp::Backend::Remote(peer)).await;
                }
            }
            return Err(bind_error);
        }
    };
    let hub = hub::Hub::open_default()?;
    let owner = bridge::Owner::start(hub.clone(), p).await?;
    let browser = owner.browser.clone();
    let h = hub.clone();
    let server = tokio::spawn(async move {
        if let Err(e) = web::serve_listener(listener, h, p, browser).await {
            tracing::warn!("web UI not started: {e}");
        }
    });
    let auto_connect = std::env::var("OBDTRACE_CONNECTION").ok().map(|target| {
        let h = hub.clone();
        tokio::spawn(async move {
            if let Err(e) = h.connect(&target).await {
                tracing::warn!("auto-connect failed: {e}");
            }
        })
    });
    let result = tokio::select! {
        result=mcp::run_stdio(mcp::Backend::Local(hub.clone()))=>result,
        signal=tokio::signal::ctrl_c()=>signal.map_err(Into::into),
    };
    server.abort();
    if let Some(task) = auto_connect {
        task.abort();
        if let Err(e) = task.await {
            if !e.is_cancelled() {
                tracing::error!("auto-connect task failed: {e}");
            }
        }
    }
    hub.disconnect().await?;
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Some(Cmd::Mcp) => run_mcp().await,
        Some(Cmd::App { no_browser }) => run_app(no_browser, false).await,
        Some(Cmd::Demo { no_browser }) => run_app(no_browser, true).await,
        Some(Cmd::Scan) => {
            let (list, warnings) = transport::list_adapters(true).await;
            for a in list {
                println!("{:<8} {:<28} {}  {}", a.kind, a.name, a.target, a.note);
            }
            for w in warnings {
                eprintln!("! {w}");
            }
            Ok(())
        }
        Some(Cmd::Doctor) => {
            let hub = hub::Hub::open_default()?;
            let results = mcp::selftest(hub).await?;
            let mut all = true;
            for (name, ok) in results {
                println!("{} {name}", if ok { "OK  " } else { "FAIL" });
                all &= ok;
            }
            println!("binary: {}", std::env::current_exe()?.display());
            if all {
                Ok(())
            } else {
                std::process::exit(1)
            }
        }
        None if std::io::stdin().is_terminal() => run_app(false, false).await,
        None => run_mcp().await,
    }
}
