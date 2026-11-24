use anyhow::{bail, Context, Result};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::io::Write;
use std::path::Path;

pub struct Identity([u8; 32]);

impl Identity {
    pub fn open(dir: &Path, existing_vehicles: bool) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("vehicle.key");
        match std::fs::read(&path) {
            Ok(bytes) => {
                let key: [u8; 32] = bytes.try_into().map_err(|_| {
                    anyhow::anyhow!(
                        "vehicle.key is corrupt; restore the original key from your private backup"
                    )
                })?;
                Ok(Self(key))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !existing_vehicles => {
                let mut key = [0; 32];
                getrandom::fill(&mut key)
                    .map_err(|e| anyhow::anyhow!("cannot generate vehicle key: {e}"))?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options
                    .open(path)
                    .context("cannot create private vehicle key")?;
                file.write_all(&key)?;
                file.sync_all()?;
                Ok(Self(key))
            }
            Err(e) => Err(e).context(
                "cannot read vehicle.key; restore the original key if vehicle history exists",
            ),
        }
    }

    pub fn fingerprint(&self, vin: &str) -> Result<String> {
        let vin = vin.trim().to_ascii_uppercase();
        if !crate::redact::is_plausible_vin(&vin) {
            bail!("vehicle did not provide a valid VIN; select a local vehicle profile");
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.0)?;
        mac.update(vin.as_bytes());
        Ok(mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_stable_and_not_based_on_masked_vin() {
        let key = Identity([7; 32]);
        let a = key.fingerprint("WAUZZZ8K9BA012345").unwrap();
        assert_eq!(a, key.fingerprint(" wauzzz8k9ba012345 ").unwrap());
        assert_ne!(a, key.fingerprint("WAUZZZ8K9BA012346").unwrap());
        assert_ne!(
            a,
            Identity([8; 32]).fingerprint("WAUZZZ8K9BA012345").unwrap()
        );
        assert!(key.fingerprint("unknown").is_err());
    }

    #[test]
    fn key_is_persistent_and_never_silently_replaced() {
        let dir = std::env::temp_dir().join(format!("obd-key-{}", uuid::Uuid::new_v4()));
        let a = Identity::open(&dir, false)
            .unwrap()
            .fingerprint("WAUZZZ8K9BA012345")
            .unwrap();
        assert_eq!(
            a,
            Identity::open(&dir, true)
                .unwrap()
                .fingerprint("WAUZZZ8K9BA012345")
                .unwrap()
        );
        std::fs::remove_file(dir.join("vehicle.key")).unwrap();
        assert!(Identity::open(&dir, true).is_err());
        std::fs::remove_dir(dir).unwrap();
    }
}
