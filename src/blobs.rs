//! Content-addressed blob store: `{root}/{sha[0..2]}/{sha}`. Writes go through a temp file.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

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

    pub async fn remove(&self, sha: &str) -> std::io::Result<()> {
        match tokio::fs::remove_file(self.path(sha)).await {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }

    /// Blobs last modified before `cutoff`. Newer ones are left alone: an upload writes its blob before
    /// the version row that references it exists, and a dedupe hit touches the existing blob.
    pub async fn stale(&self, cutoff: SystemTime) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let mut dirs = tokio::fs::read_dir(&self.root)
            .await
            .context("list blob dir")?;
        while let Some(dir) = dirs.next_entry().await? {
            let prefix = dir.file_name().to_string_lossy().into_owned();
            if prefix.len() != 2 || !dir.file_type().await?.is_dir() {
                continue;
            }
            let mut files = tokio::fs::read_dir(dir.path()).await?;
            while let Some(f) = files.next_entry().await? {
                let sha = f.file_name().to_string_lossy().into_owned();
                if sha.len() == 64
                    && sha.starts_with(&prefix)
                    && f.metadata().await?.modified()? < cutoff
                {
                    out.push(sha);
                }
            }
        }
        Ok(out)
    }

    /// Delete temp files from before `cutoff` (left behind by a crash mid-upload).
    pub async fn prune_tmp(&self, cutoff: SystemTime) -> Result<usize> {
        let mut n = 0;
        let mut files = tokio::fs::read_dir(self.root.join("tmp"))
            .await
            .context("list tmp dir")?;
        while let Some(f) = files.next_entry().await? {
            if f.metadata().await?.modified()? < cutoff {
                tokio::fs::remove_file(f.path()).await?;
                n += 1;
            }
        }
        Ok(n)
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
            // the blob is about to gain a new reference: keep it inside the sweep's grace period
            if let Ok(f) = std::fs::File::options().write(true).open(&final_path) {
                f.set_modified(SystemTime::now()).ok();
            }
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
