//! Per-owner rendezvous. The certificate is trusted only for this process instance.
use crate::{
    hub::Hub,
    store::data_dir,
    tools::{self, Ctx, Origin},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_rustls::{
    rustls::{
        pki_types::{CertificateDer, PrivatePkcs8KeyDer},
        ClientConfig, RootCertStore, ServerConfig,
    },
    TlsAcceptor, TlsConnector,
};

#[derive(Serialize, Deserialize)]
pub struct Peer {
    port: u16,
    cert: Vec<u8>,
    token: String,
    data: PathBuf,
}

fn runtime_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(base)
            .join("OBDTrace")
            .join(".obdtrace-runtime");
    }
    data_dir().join(".obdtrace-runtime")
}
pub fn launch_file(port: u16) -> PathBuf {
    runtime_dir().join(format!("open-{port}.html"))
}
fn descriptor(port: u16) -> PathBuf {
    runtime_dir().join(format!("peer-{port}.json"))
}

pub fn prepare_dir() -> Result<()> {
    #[cfg(windows)]
    ensure!(
        std::env::var_os("LOCALAPPDATA").is_some(),
        "private Windows user profile required"
    );
    let data = data_dir();
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    for path in [&data, &runtime_dir()] {
        builder.create(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let m = path.symlink_metadata()?;
            ensure!(
                m.is_dir() && m.uid() == unsafe { libc::geteuid() } && m.mode() & 0o077 == 0,
                "use a private directory owned by your account (0700): {}",
                path.display()
            );
            for base in [std::path::absolute(path)?, path.canonicalize()?] {
                for parent in base.ancestors().skip(1) {
                    let link = parent.symlink_metadata()?;
                    let target = parent.metadata()?;
                    ensure!(
                        [link.uid(), target.uid()]
                            .iter()
                            .all(|&id| id == 0 || id == m.uid())
                            && (target.mode() & 0o022 == 0 || target.mode() & 0o1000 != 0),
                        "another user can replace a parent of the private data directory: {}",
                        parent.display()
                    );
                }
            }
        }
    }
    Ok(())
}

fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(runtime_dir())?;
    file.write_all(bytes)?;
    file.persist(path)?;
    Ok(())
}

async fn recv(stream: &mut (impl AsyncRead + Unpin), limit: usize) -> Result<Value> {
    let size = stream.read_u32().await? as usize;
    ensure!(size <= limit, "bridge message too large");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn send(stream: &mut (impl AsyncWrite + Unpin), value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    stream.write_u32(bytes.len().try_into()?).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

impl Peer {
    pub async fn discover(port: u16) -> Result<Self> {
        let peer: Self = serde_json::from_slice(&std::fs::read(descriptor(port))?)?;
        ensure!(
            peer.data == data_dir().canonicalize()?,
            "different OBDTrace data directory"
        );
        ensure!(
            timeout(Duration::from_secs(2), peer.request(json!({"op":"ping"}))).await?? == true,
            "cannot authenticate running OBDTrace instance"
        );
        Ok(peer)
    }

    pub async fn request(&self, mut req: Value) -> Result<Value> {
        let mut roots = RootCertStore::empty();
        roots.add(CertificateDer::from(self.cert.clone()))?;
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        timeout(Duration::from_secs(120), async {
            let tcp = TcpStream::connect(("127.0.0.1", self.port)).await?;
            let mut stream = connector
                .connect("localhost".try_into()?, tcp)
                .await
                .context("bridge certificate mismatch or TLS failure")?;
            req["token"] = json!(self.token);
            send(&mut stream, &req).await?;
            let response = recv(&mut stream, 32 << 20).await?;
            ensure!(
                response["ok"] == true,
                "{}",
                response["error"].as_str().unwrap_or("invalid bridge reply")
            );
            Ok(response["result"].clone())
        })
        .await?
    }
}

pub struct Owner {
    pub browser: String,
    task: tokio::task::JoinHandle<()>,
    port: u16,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(descriptor(self.port));
        let _ = std::fs::remove_file(launch_file(self.port));
    }
}

impl Owner {
    pub async fn start(hub: Arc<Hub>, port: u16) -> Result<Self> {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
        let der = cert.cert.der().clone();
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![der.clone()],
                PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
            )?;
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let token = uuid::Uuid::new_v4().simple().to_string();
        let browser = uuid::Uuid::new_v4().simple().to_string();
        let peer = Peer {
            port: listener.local_addr()?.port(),
            cert: der.to_vec(),
            token: token.clone(),
            data: data_dir().canonicalize()?,
        };
        publish(&launch_file(port), format!("<meta name=\"referrer\" content=\"no-referrer\"><meta http-equiv=\"refresh\" content=\"0;url=http://127.0.0.1:{port}/#{browser}\">").as_bytes())?;
        publish(&descriptor(port), &serde_json::to_vec(&peer)?)?;
        let slots = Arc::new(tokio::sync::Semaphore::new(32));
        let task = tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    continue;
                };
                let (acceptor, hub, token) = (acceptor.clone(), hub.clone(), token.clone());
                tokio::spawn(async move {
                    let _slot = slot;
                    let preflight = async {
                        let mut stream = acceptor.accept(tcp).await?;
                        let req = recv(&mut stream, 1 << 20).await?;
                        ensure!(req["token"].as_str() == Some(&token), "unauthorized");
                        Ok::<_, anyhow::Error>((stream, req))
                    };
                    let Ok(Ok((mut stream, v))) = timeout(Duration::from_secs(3), preflight).await
                    else {
                        return;
                    };
                    let result = match v["op"].as_str() {
                        Some("ping") => Ok(json!(true)),
                        Some("hello") => {
                            hub.note_client(
                                v["name"].as_str().unwrap_or(""),
                                v["version"].as_str().unwrap_or(""),
                            );
                            Ok(Value::Null)
                        }
                        Some("call") => {
                            tools::call(
                                &hub,
                                v["name"].as_str().unwrap_or(""),
                                v["args"].clone(),
                                Ctx {
                                    origin: Origin::Mcp,
                                    confirmed: v["confirmed"] == true,
                                    mcp_clear_enabled: tools::clear_enabled_for_mcp(),
                                },
                            )
                            .await
                        }
                        _ => Err(anyhow::anyhow!("unknown bridge operation")),
                    };
                    let reply = match result {
                        Ok(value) => json!({"ok":true,"result":value}),
                        Err(e) => json!({"ok":false,"error":e.to_string()}),
                    };
                    let _ = timeout(Duration::from_secs(5), send(&mut stream, &reply)).await;
                });
            }
        });
        Ok(Self {
            browser,
            task,
            port,
        })
    }
}
