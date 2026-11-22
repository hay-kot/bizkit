use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use async_trait::async_trait;
use bytes::Bytes;

use super::{Blob, BlobKey, BlobStore};

/// Blobs as files: content under `data/<key>`, its MIME type under `meta/<key>`.
#[derive(Debug, Clone)]
pub struct FsBlobs {
    root: PathBuf,
}

impl FsBlobs {
    /// A store rooted at `root`, created on first write.
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn paths(&self, key: &BlobKey) -> (PathBuf, PathBuf) {
        (self.root.join("data").join(key.as_str()), self.root.join("meta").join(key.as_str()))
    }
}

async fn write(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    tokio::fs::write(path, contents).await.with_context(|| format!("writing {}", path.display()))
}

async fn remove(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_file(path).await {
        Err(err) if err.kind() != ErrorKind::NotFound => {
            Err(err).with_context(|| format!("removing {}", path.display()))
        }
        _ => Ok(()),
    }
}

#[async_trait]
impl BlobStore for FsBlobs {
    async fn put(&self, key: &BlobKey, body: Bytes, content_type: &str) -> anyhow::Result<()> {
        let (data, meta) = self.paths(key);
        write(&data, &body).await?;
        write(&meta, content_type.as_bytes()).await
    }

    async fn get(&self, key: &BlobKey) -> anyhow::Result<Option<Blob>> {
        let (data, meta) = self.paths(key);
        let body = match tokio::fs::read(&data).await {
            Ok(body) => body,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err).with_context(|| format!("reading {}", data.display())),
        };
        let content_type = tokio::fs::read_to_string(&meta)
            .await
            .unwrap_or_else(|_| "application/octet-stream".to_owned());
        Ok(Some(Blob { body: Bytes::from(body), content_type }))
    }

    async fn delete(&self, key: &BlobKey) -> anyhow::Result<()> {
        let (data, meta) = self.paths(key);
        remove(&data).await?;
        remove(&meta).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trips_a_blob() {
        let dir = tempfile::tempdir().unwrap();
        let blobs = FsBlobs::new(dir.path().to_path_buf());
        let key = BlobKey::new("forms/a/upload.txt").unwrap();
        blobs.put(&key, Bytes::from_static(b"hello"), "text/plain").await.unwrap();
        let blob = blobs.get(&key).await.unwrap().unwrap();
        assert_eq!(
            (blob.body.as_ref(), blob.content_type.as_str()),
            (&b"hello"[..], "text/plain"),
            "blob"
        );
        blobs.delete(&key).await.unwrap();
        assert!(blobs.get(&key).await.unwrap().is_none(), "deleted");
        blobs.delete(&key).await.unwrap();
    }
}
