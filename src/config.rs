use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    /// Shell origin, no trailing slash, e.g. `https://handout.example`.
    pub public_url: String,
    /// Content origin, no trailing slash.
    pub content_url: String,
    pub secret: [u8; 32],
    pub max_upload_bytes: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let data_dir = PathBuf::from(env_or("HANDOUT_DATA_DIR", "./data"));
        let listen: SocketAddr = env_or("HANDOUT_LISTEN", "0.0.0.0:8080")
            .parse()
            .context("HANDOUT_LISTEN must be host:port")?;
        let public_url = trim_slash(&env_or("HANDOUT_PUBLIC_URL", "http://localhost:8080"));
        let content_url = trim_slash(&env_or("HANDOUT_CONTENT_URL", "http://127.0.0.1:8080"));
        let max_upload_mb: usize = env_or("HANDOUT_MAX_UPLOAD_MB", "100")
            .parse()
            .context("HANDOUT_MAX_UPLOAD_MB must be an integer")?;
        std::fs::create_dir_all(&data_dir).context("create data dir")?;
        let secret = match std::env::var("HANDOUT_SECRET") {
            Ok(s) if !s.is_empty() => Sha256::digest(s.as_bytes()).into(),
            _ => load_or_create_secret(&data_dir)?,
        };
        Ok(Self {
            data_dir,
            listen,
            public_url,
            content_url,
            secret,
            max_upload_bytes: max_upload_mb * 1024 * 1024,
        })
    }

    pub fn for_tests(data_dir: PathBuf, public_url: &str, content_url: &str) -> Self {
        let mut secret = [0u8; 32];
        rand::fill(&mut secret);
        Self {
            data_dir,
            listen: "127.0.0.1:0".parse().unwrap(),
            public_url: trim_slash(public_url),
            content_url: trim_slash(content_url),
            secret,
            max_upload_bytes: 100 * 1024 * 1024,
        }
    }

    /// `host[:port]` part of the public URL.
    pub fn public_host(&self) -> String {
        host_of(&self.public_url)
    }
    pub fn content_host(&self) -> String {
        host_of(&self.content_url)
    }
    pub fn secure_cookies(&self) -> bool {
        self.public_url.starts_with("https://")
    }
    /// `__Host-` prefix in production, plain name over http (dev).
    pub fn cookie_name(&self, base: &str) -> String {
        if self.secure_cookies() {
            format!("__Host-{base}")
        } else {
            base.to_string()
        }
    }
    pub fn file_link(&self, owner_slug: &str, public_id: &str) -> String {
        format!("{}/s/{}/{}", self.public_url, owner_slug, public_id)
    }
    pub fn recipient_link(&self, owner_slug: &str, public_id: &str, token: &str) -> String {
        format!("{}?r={}", self.file_link(owner_slug, public_id), token)
    }
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("handout.db")
    }
    pub fn blob_dir(&self) -> PathBuf {
        self.data_dir.join("blobs")
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn trim_slash(s: &str) -> String {
    s.trim_end_matches('/').to_string()
}

pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split('/').next().unwrap_or(rest).to_string()
}

fn load_or_create_secret(data_dir: &std::path::Path) -> Result<[u8; 32]> {
    let path = data_dir.join("secret");
    if let Ok(hex) = std::fs::read_to_string(&path) {
        let bytes = decode_hex(hex.trim()).context("secret file is not hex")?;
        if bytes.len() == 32 {
            return Ok(bytes.try_into().unwrap());
        }
    }
    let mut secret = [0u8; 32];
    rand::fill(&mut secret);
    std::fs::write(&path, encode_hex(&secret)).context("write secret file")?;
    Ok(secret)
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
