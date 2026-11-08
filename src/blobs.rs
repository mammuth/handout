//! Content-addressed blob store: `{root}/{sha[0..2]}/{sha}`. Writes go through a temp file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

pub struct BlobStore {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StoredBlob {
    pub sha256: String,
    pub size: i64,
}

impl BlobStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(root.join("tmp")).context("create blob dir")?;
        Ok(Self { root })
    }

    pub fn path(&self, sha: &str) -> PathBuf {
        self.root.join(&sha[..2]).join(sha)
    }

    pub async fn writer(&self) -> Result<BlobWriter> {
        let tmp_path = self.root.join("tmp").join(crate::ids::random_base58(16));
        let file = tokio::fs::File::create(&tmp_path)
            .await
            .context("create temp blob")?;
        Ok(BlobWriter {
            file,
            tmp_path,
            hasher: Sha256::new(),
            size: 0,
            root: self.root.clone(),
        })
    }

    pub async fn put_bytes(&self, data: &[u8]) -> Result<StoredBlob> {
        let mut w = self.writer().await?;
        w.write(data).await?;
        w.finish().await
    }

    pub async fn open(&self, sha: &str) -> std::io::Result<tokio::fs::File> {
        tokio::fs::File::open(self.path(sha)).await
    }

    pub async fn read(&self, sha: &str) -> std::io::Result<Vec<u8>> {
        tokio::fs::read(self.path(sha)).await
    }
}

pub struct BlobWriter {
    file: tokio::fs::File,
    tmp_path: PathBuf,
    hasher: Sha256,
    size: u64,
    root: PathBuf,
}

impl BlobWriter {
    pub async fn write(&mut self, chunk: &[u8]) -> Result<()> {
        self.hasher.update(chunk);
        self.size += chunk.len() as u64;
        self.file
            .write_all(chunk)
            .await
            .context("write blob chunk")?;
        Ok(())
    }

    pub async fn finish(mut self) -> Result<StoredBlob> {
        self.file.flush().await?;
        self.file.sync_all().await.ok();
        drop(self.file);
        let sha = crate::config::encode_hex(&self.hasher.finalize());
        let final_path = self.root.join(&sha[..2]).join(&sha);
        if tokio::fs::metadata(&final_path).await.is_ok() {
            tokio::fs::remove_file(&self.tmp_path).await.ok();
        } else {
            tokio::fs::create_dir_all(final_path.parent().unwrap()).await?;
            tokio::fs::rename(&self.tmp_path, &final_path)
                .await
                .context("move blob into place")?;
        }
        Ok(StoredBlob {
            sha256: sha,
            size: self.size as i64,
        })
    }

    pub async fn abort(self) {
        drop(self.file);
        tokio::fs::remove_file(&self.tmp_path).await.ok();
    }
}

pub fn ensure_dir(p: &Path) -> Result<()> {
    std::fs::create_dir_all(p).with_context(|| format!("create {}", p.display()))
}
