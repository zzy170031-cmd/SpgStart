use aes_gcm_siv::aead::{Aead, KeyInit};
use aes_gcm_siv::{Aes256GcmSiv, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub trait SecretStore: Send + Sync {
    fn set_secret(&self, key: &str, value: &str) -> Result<()>;
    fn get_secret(&self, key: &str) -> Result<Option<String>>;
    fn delete_secret(&self, key: &str) -> Result<()>;
}

#[derive(Clone)]
pub struct HybridSecretStore {
    service_name: String,
    fallback: FileSecretVault,
}

impl HybridSecretStore {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            service_name: "spg-web".to_string(),
            fallback: FileSecretVault::new(
                workspace_root
                    .join("data")
                    .join(".spg_web_provider_secrets.enc"),
            ),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service_name, key).context("failed to open keyring entry")
    }
}

impl SecretStore for HybridSecretStore {
    fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        let keyring_result = self.entry(key).and_then(|entry| {
            entry
                .set_password(value)
                .context("failed to write keyring secret")
        });

        if keyring_result.is_ok()
            && !persist_fallback_copy()
            && self.keyring_round_trips(key).unwrap_or(false)
        {
            let _ = self.fallback.delete_secret(key);
            return Ok(());
        }

        self.fallback.set_secret(key, value)?;

        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key).and_then(|entry| {
            entry
                .get_password()
                .map(Some)
                .context("failed to read keyring secret")
        }) {
            Ok(Some(secret)) if !secret.trim().is_empty() => Ok(Some(secret)),
            _ => self.fallback.get_secret(key),
        }
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        let _ = self.entry(key).and_then(|entry| {
            entry
                .delete_credential()
                .context("failed to delete keyring secret")
        });

        self.fallback.delete_secret(key)
    }
}

fn persist_fallback_copy() -> bool {
    matches!(
        std::env::var("SPG_WEB_FORCE_FILE_SECRET_FALLBACK"),
        Ok(value) if matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES")
    )
}

impl HybridSecretStore {
    fn keyring_round_trips(&self, key: &str) -> Result<bool> {
        let secret = self.entry(key).and_then(|entry| {
            entry
                .get_password()
                .context("failed to read keyring secret")
        })?;
        Ok(!secret.trim().is_empty())
    }
}

#[derive(Clone)]
struct FileSecretVault {
    path: PathBuf,
}

impl FileSecretVault {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load_map(&self) -> Result<HashMap<String, String>> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }

        let payload = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let raw = STANDARD
            .decode(payload.trim())
            .context("failed to decode fallback secret payload")?;
        if raw.len() < 12 {
            return Ok(HashMap::new());
        }

        let (nonce_bytes, cipher_bytes) = raw.split_at(12);
        let cipher = cipher();
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), cipher_bytes)
            .map_err(|_| anyhow!("failed to decrypt fallback secret vault"))?;
        let map = serde_json::from_slice(&plaintext).context("failed to parse secret vault")?;
        Ok(map)
    }

    fn save_map(&self, map: &HashMap<String, String>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let mut nonce = [0_u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let plaintext = serde_json::to_vec(map).context("failed to serialize secret vault")?;
        let cipher = cipher();
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
            .map_err(|_| anyhow!("failed to encrypt secret vault"))?;

        let mut combined = nonce.to_vec();
        combined.extend(ciphertext);
        fs::write(&self.path, STANDARD.encode(combined))
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }
}

impl SecretStore for FileSecretVault {
    fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        let mut map = self.load_map()?;
        map.insert(key.to_string(), value.to_string());
        self.save_map(&map)
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        Ok(self.load_map()?.get(key).cloned())
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        let mut map = self.load_map()?;
        map.remove(key);
        self.save_map(&map)
    }
}

fn cipher() -> Aes256GcmSiv {
    let username = std::env::var("USERNAME").unwrap_or_else(|_| "unknown-user".to_string());
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown-host".to_string());
    let digest = Sha256::digest(format!("spg-web::{username}::{hostname}").as_bytes());
    Aes256GcmSiv::new_from_slice(&digest).expect("cipher key")
}

#[derive(Clone, Default)]
pub struct MemorySecretStore {
    secrets: Arc<Mutex<HashMap<String, String>>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemorySecretStore {
    fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        self.secrets
            .lock()
            .expect("memory secrets lock")
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .secrets
            .lock()
            .expect("memory secrets lock")
            .get(key)
            .cloned())
    }

    fn delete_secret(&self, key: &str) -> Result<()> {
        self.secrets
            .lock()
            .expect("memory secrets lock")
            .remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{HybridSecretStore, SecretStore};
    use tempfile::tempdir;

    #[test]
    fn hybrid_store_round_trips_with_encrypted_fallback() {
        let temp = tempdir().expect("temp dir");
        let store = HybridSecretStore::new(temp.path());
        store
            .set_secret("provider::doubao", "secret-demo-key")
            .expect("set secret");

        let secret = store
            .get_secret("provider::doubao")
            .expect("get secret")
            .expect("stored secret");
        assert_eq!(secret, "secret-demo-key");
    }
}
