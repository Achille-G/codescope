//! Model download utilities with progress reporting and checksum verification.

use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use tracing::{debug, info, warn};

/// Progress information during download.
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    /// Bytes downloaded so far.
    pub downloaded: u64,
    /// Total bytes (if known from Content-Length).
    pub total: Option<u64>,
}

/// Number of attempts per URL before giving up.
const MAX_ATTEMPTS: u32 = 3;
/// Base delay for exponential backoff between attempts (2s, 4s, 8s...).
const RETRY_BASE_DELAY_SECS: u64 = 2;
/// Default per-request timeout, overridable via `CODESCOPE_DOWNLOAD_TIMEOUT_SECS`.
const DEFAULT_TIMEOUT_SECS: u64 = 300;

fn download_timeout() -> std::time::Duration {
    let secs = std::env::var("CODESCOPE_DOWNLOAD_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    std::time::Duration::from_secs(secs)
}

/// Download a file from a URL with progress reporting and optional checksum verification.
///
/// # Arguments
/// * `url` - The URL to download from
/// * `dest` - Destination path for the downloaded file
/// * `expected_sha256` - Optional SHA256 hex string for verification
/// * `on_progress` - Optional callback for progress updates
///
/// # Returns
/// * `Ok(())` if download and verification succeeded
/// * `Err(...)` if download failed or checksum mismatch
pub fn download_file<F>(
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    mut on_progress: Option<F>,
) -> Result<()>
where
    F: FnMut(DownloadProgress),
{
    let mut last_error = None;

    for attempt in 1..=MAX_ATTEMPTS {
        match download_file_once(url, dest, expected_sha256, &mut on_progress) {
            Ok(()) => return Ok(()),
            Err(e) => {
                if attempt < MAX_ATTEMPTS {
                    let delay = RETRY_BASE_DELAY_SECS * 2u64.pow(attempt - 1);
                    warn!(
                        "Download attempt {attempt}/{MAX_ATTEMPTS} failed for {url}: {e}; \
                         retrying in {delay}s"
                    );
                    std::thread::sleep(std::time::Duration::from_secs(delay));
                }
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| Error::Download("Download failed".to_string())))
}

fn download_file_once<F>(
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    on_progress: &mut Option<F>,
) -> Result<()>
where
    F: FnMut(DownloadProgress),
{
    info!("Downloading {} to {}", url, dest.display());

    // Create parent directories if needed
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    // Use a temp file during download
    let temp_path = dest.with_extension("download");

    let result = download_to_temp(url, &temp_path, on_progress);

    if let Err(e) = &result {
        // Clean up temp file on error
        let _ = fs::remove_file(&temp_path);
        return Err(e.clone());
    }

    // Verify checksum if provided
    if let Some(expected) = expected_sha256 {
        info!("Verifying SHA256 checksum...");
        let actual = compute_sha256(&temp_path)?;
        if actual.to_lowercase() != expected.to_lowercase() {
            let _ = fs::remove_file(&temp_path);
            return Err(Error::ChecksumMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
        debug!("Checksum verified: {}", actual);
    }

    // Rename temp file to final destination
    fs::rename(&temp_path, dest)?;
    info!("Download complete: {}", dest.display());

    Ok(())
}

fn download_to_temp<F>(url: &str, temp_path: &Path, on_progress: &mut Option<F>) -> Result<()>
where
    F: FnMut(DownloadProgress),
{
    let client = reqwest::blocking::Client::builder()
        .timeout(download_timeout())
        .build()
        .map_err(|e| Error::Download(e.to_string()))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| Error::Download(e.to_string()))?;

    if !response.status().is_success() {
        return Err(Error::Download(format!(
            "HTTP {} from {}",
            response.status(),
            url
        )));
    }

    let total = response.content_length();
    let mut downloaded: u64 = 0;

    let file = File::create(temp_path)?;
    let mut writer = BufWriter::new(file);
    let mut reader = response;

    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| Error::Download(e.to_string()))?;

        if bytes_read == 0 {
            break;
        }

        writer.write_all(&buffer[..bytes_read])?;
        downloaded += bytes_read as u64;

        if let Some(cb) = on_progress.as_mut() {
            cb(DownloadProgress { downloaded, total });
        }
    }

    writer.flush()?;
    Ok(())
}

/// Compute SHA256 checksum of a file.
pub fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Download model files with fallback URLs.
///
/// Tries primary URL first, then falls back to mirrors if available.
pub fn download_with_fallback<F>(
    urls: &[&str],
    dest: &Path,
    expected_sha256: Option<&str>,
    on_progress: Option<F>,
) -> Result<()>
where
    F: FnMut(DownloadProgress) + Clone,
{
    if urls.is_empty() {
        return Err(Error::Download("No download URLs provided".to_string()));
    }

    let mut last_error = None;

    for (i, url) in urls.iter().enumerate() {
        match download_file(url, dest, expected_sha256, on_progress.clone()) {
            Ok(()) => return Ok(()),
            Err(e) => {
                warn!(
                    "Download failed from {} (attempt {}/{}): {}",
                    url,
                    i + 1,
                    urls.len(),
                    e
                );
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| Error::Download("All download attempts failed".to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_compute_sha256() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();

        let hash = compute_sha256(&file_path).unwrap();
        // SHA256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
