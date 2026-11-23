use super::{AdapterInfo, Transport};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;
use tokio::io::unix::AsyncFd;

const BTPROTO_RFCOMM: libc::c_int = 3;
const SPP: uuid::Uuid = uuid::Uuid::from_u128(0x00001101_0000_1000_8000_00805f9b34fb);

pub async fn paired_adapters() -> Result<Vec<AdapterInfo>> {
    let devices = tokio::time::timeout(Duration::from_secs(5), async {
        let (_, session) = bluez_async::BluetoothSession::new().await?;
        session.get_devices().await
    })
    .await
    .context("paired Bluetooth device listing timed out")??;
    let mut adapters: Vec<_> = devices
        .into_iter()
        .filter(|device| device.paired && !device.blocked && device.services.contains(&SPP))
        .map(|device| {
            let address = device.mac_address.to_string();
            let name = device.alias.or(device.name).filter(|s| !s.trim().is_empty())
                .map_or_else(|| address.clone(), |name| format!("{name} · {address}"));
            AdapterInfo {
                kind: "rfcomm".into(),
                target: format!("rfcomm:{address}"),
                name,
                recommended: false,
                note_key: "pairedSpp",
                note: "Paired classic Bluetooth SPP device (channel 1); select only if this is your adapter".into(),
            }
        })
        .collect();
    adapters.sort_by(|a, b| (&a.target, &a.name).cmp(&(&b.target, &b.name)));
    adapters.dedup_by(|a, b| a.target == b.target);
    Ok(adapters)
}

#[repr(C)]
struct SockAddrRc {
    family: libc::sa_family_t,
    address: [u8; 6],
    channel: u8,
    padding: u8,
}

fn checked(result: libc::ssize_t) -> io::Result<usize> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result as usize)
    }
}

pub struct RfcommTransport {
    stream: AsyncFd<OwnedFd>,
    name: String,
}

impl RfcommTransport {
    pub async fn connect(address: [u8; 6], channel: u8) -> Result<Self> {
        let mac = address.map(|b| format!("{b:02X}")).join(":");
        let name = format!("rfcomm:{mac}@{channel}");
        let stream = tokio::time::timeout(Duration::from_secs(12), connect_socket(address, channel))
            .await
            .context("RFCOMM connection timed out")?
            .with_context(|| format!("could not connect to {name}; pair the adapter first, check its SPP channel and close other apps using it"))?;
        Ok(Self { stream, name })
    }
}

async fn connect_socket(mut address: [u8; 6], channel: u8) -> io::Result<AsyncFd<OwnedFd>> {
    let raw = unsafe {
        libc::socket(
            libc::AF_BLUETOOTH,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            BTPROTO_RFCOMM,
        )
    };
    checked(raw as _)?;
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };

    // Linux's bdaddr_t stores the MAC address least significant byte first.
    address.reverse();
    let addr = SockAddrRc {
        family: libc::AF_BLUETOOTH as _,
        address,
        channel,
        padding: 0,
    };
    let result = unsafe {
        libc::connect(
            raw,
            (&addr as *const SockAddrRc).cast(),
            size_of::<SockAddrRc>() as _,
        )
    };
    let pending = match checked(result as _) {
        Ok(_) => false,
        Err(e)
            if matches!(
                e.raw_os_error(),
                Some(libc::EINPROGRESS) | Some(libc::EAGAIN)
            ) =>
        {
            true
        }
        Err(e) => return Err(e),
    };
    let stream = register(fd)?;
    if pending {
        let _ready = stream.writable().await?;
        let mut error: libc::c_int = 0;
        let mut len = size_of::<libc::c_int>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                raw,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut error as *mut libc::c_int).cast(),
                &mut len,
            )
        };
        checked(result as _)?;
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
    }
    Ok(stream)
}

fn register(fd: OwnedFd) -> io::Result<AsyncFd<OwnedFd>> {
    // The owned descriptor stays open and unchanged until AsyncFd unregisters it.
    unsafe { AsyncFd::register(fd) }.map_err(Into::into)
}

#[async_trait]
impl Transport for RfcommTransport {
    async fn write(&mut self, mut data: &[u8]) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !data.is_empty() {
                let mut ready = self.stream.writable().await?;
                if let Ok(result) = ready.try_io(|fd| {
                    checked(unsafe {
                        libc::send(
                            fd.as_raw_fd(),
                            data.as_ptr().cast(),
                            data.len(),
                            libc::MSG_NOSIGNAL,
                        )
                    })
                }) {
                    let n = result?;
                    if n == 0 {
                        bail!("RFCOMM socket stopped accepting data");
                    }
                    data = &data[n..];
                }
            }
            Ok(())
        })
        .await
        .context("RFCOMM write timed out")?
    }

    async fn read_chunk(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let result = tokio::time::timeout(timeout, async {
            let mut buf = vec![0u8; 512];
            loop {
                let mut ready = self.stream.readable().await?;
                if let Ok(result) = ready.try_io(|fd| {
                    checked(unsafe {
                        libc::recv(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0)
                    })
                }) {
                    let n = result?;
                    if n == 0 {
                        bail!("RFCOMM adapter disconnected");
                    }
                    buf.truncate(n);
                    return Ok(Some(buf));
                }
            }
        })
        .await;
        result.unwrap_or(Ok(None))
    }

    async fn close(&mut self) {
        unsafe { libc::shutdown(self.stream.as_raw_fd(), libc::SHUT_RDWR) };
    }

    fn describe(&self) -> String {
        self.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    #[tokio::test]
    async fn nonblocking_stream_transfers_bytes_and_detects_hangup() {
        let (local, mut peer) = UnixStream::pair().unwrap();
        local.set_nonblocking(true).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut transport = RfcommTransport {
            stream: register(local.into()).unwrap(),
            name: "test".into(),
        };
        let timeout = Duration::from_secs(1);
        assert_eq!(
            transport
                .read_chunk(Duration::from_millis(5))
                .await
                .unwrap(),
            None
        );
        transport.write(b"ATI\r").await.unwrap();
        let mut command = [0; 4];
        peer.read_exact(&mut command).unwrap();
        assert_eq!(&command, b"ATI\r");
        peer.write_all(b"ELM327\r>").unwrap();
        assert_eq!(
            transport.read_chunk(timeout).await.unwrap().unwrap(),
            b"ELM327\r>"
        );
        drop(peer);
        assert!(transport.read_chunk(timeout).await.is_err());
    }
}
