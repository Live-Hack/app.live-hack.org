//! Challenge files on local disk, under `DATA_DIR/files/<tenant>/<key>`.
//! ponytail: local disk only; an S3 backend would implement the same functions.
use crate::helper::crypto::hex;
use futures::{Stream, StreamExt};
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

pub static DATA_DIR: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into())));

/// Largest accepted upload, in bytes (MAX_UPLOAD_MB, default 200).
pub static MAX_UPLOAD: Lazy<u64> = Lazy::new(|| {
    std::env::var("MAX_UPLOAD_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200u64)
        * 1024
        * 1024
});

pub fn path(tenant: &str, key: &str) -> Option<PathBuf> {
    // keys are generated uuids; reject anything else so a key can never escape the folder
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    (ok(tenant) && ok(key)).then(|| DATA_DIR.join("files").join(tenant).join(key))
}

pub struct Stored {
    pub key: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug)]
pub enum StoreError {
    TooLarge,
    Io(std::io::Error),
}

/// Streams bytes to a new file, hashing on the way. Removes the partial file on error.
pub async fn store<S, E>(tenant: &str, mut body: S) -> Result<Stored, StoreError>
where
    S: Stream<Item = Result<actix_web::web::Bytes, E>> + Unpin,
{
    let key = uuid::Uuid::new_v4().to_string();
    let dest = path(tenant, &key).ok_or(StoreError::Io(std::io::ErrorKind::InvalidInput.into()))?;
    tokio::fs::create_dir_all(dest.parent().unwrap())
        .await
        .map_err(StoreError::Io)?;
    let tmp = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(StoreError::Io)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let result = async {
        while let Some(chunk) = body.next().await {
            let chunk =
                chunk.map_err(|_| StoreError::Io(std::io::ErrorKind::ConnectionAborted.into()))?;
            size += chunk.len() as u64;
            if size > *MAX_UPLOAD {
                return Err(StoreError::TooLarge);
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(StoreError::Io)?;
        }
        file.flush().await.map_err(StoreError::Io)?;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    tokio::fs::rename(&tmp, &dest)
        .await
        .map_err(StoreError::Io)?;
    Ok(Stored {
        key,
        sha256: hex(&hasher.finalize()),
        size,
    })
}

/// Stores an in-memory blob (imports).
pub async fn store_bytes(tenant: &str, data: Vec<u8>) -> Result<Stored, StoreError> {
    let stream =
        futures::stream::once(
            async move { Ok::<_, std::io::Error>(actix_web::web::Bytes::from(data)) },
        );
    store(tenant, Box::pin(stream)).await
}

pub async fn remove(tenant: &str, key: &str) {
    if let Some(p) = path(tenant, key) {
        let _ = tokio::fs::remove_file(p).await;
    }
}

pub async fn read(tenant: &str, key: &str) -> std::io::Result<Vec<u8>> {
    match path(tenant, key) {
        Some(p) => tokio::fs::read(p).await,
        None => Err(std::io::ErrorKind::InvalidInput.into()),
    }
}

/// Keeps only the last path component and printable characters of an uploaded file name.
pub fn clean_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let s: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    let s = s.trim().trim_start_matches('.').to_string();
    if s.is_empty() { "file".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_and_keys() {
        assert_eq!(clean_filename("../../etc/passwd"), "passwd");
        assert_eq!(clean_filename("C:\\x\\dump.raw"), "dump.raw");
        assert_eq!(clean_filename(".hidden"), "hidden");
        assert_eq!(clean_filename(""), "file");
        assert!(path("abc-1", "../x").is_none());
        assert!(path("abc-1", "0f-2").is_some());
    }
}
