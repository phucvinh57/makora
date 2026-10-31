//! Downloads models from the Ollama registry into Makora's own model directory.
//!
//! The directory uses Ollama's on-disk layout (`manifests/…` and `blobs/sha256-…`), so
//! [`llm::resolve_model`](crate::ai::llm::resolve_model) finds a downloaded model the same
//! way it finds one pulled by Ollama itself. Only the weights layer is fetched; that is all
//! llama.cpp needs.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, warn};
use ureq::Agent;

use crate::logs::NETWORK_TARGET;
use crate::shared::sync::{lock, RunGuard};

const REGISTRY: &str = "https://registry.ollama.ai";
const REGISTRY_HOST: &str = "registry.ollama.ai";
const PURPOSE: &str = "model download";
const MODEL_LAYER: &str = "application/vnd.ollama.image.model";
const READ_BUF: usize = 256 * 1024;
const RANGE_BYTES: u64 = 64 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, thiserror::Error)]
pub enum ModelDownloadError {
    #[error("a model download is already running")]
    AlreadyRunning,
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

#[derive(Default)]
pub struct ModelDownloadState {
    running: AtomicBool,
    cancel: AtomicBool,
    model: Mutex<String>,
    downloaded: AtomicU64,
    total: AtomicU64,
}

#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ModelDownloadStatus {
    pub running: bool,
    pub model: String,
    pub downloaded: u64,
    pub total: u64,
}

impl ModelDownloadState {
    pub fn status(&self) -> ModelDownloadStatus {
        ModelDownloadStatus {
            running: self.running.load(Ordering::Acquire),
            model: lock(&self.model).clone(),
            downloaded: self.downloaded.load(Ordering::Acquire),
            total: self.total.load(Ordering::Acquire),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Downloads `model` into `root`, calling `on_progress(downloaded, total)` as bytes arrive.
    /// Returns `false` when the user stopped it; the partial file is kept for resuming.
    pub fn run(
        &self,
        root: &Path,
        model: &str,
        on_progress: &mut dyn FnMut(u64, u64),
    ) -> Result<bool, ModelDownloadError> {
        let _running = RunGuard::start(&self.running).ok_or(ModelDownloadError::AlreadyRunning)?;
        self.cancel.store(false, Ordering::Release);
        *lock(&self.model) = model.to_string();
        self.downloaded.store(0, Ordering::Release);
        self.total.store(0, Ordering::Release);

        let mut progress = |downloaded: u64, total: u64| {
            self.downloaded.store(downloaded, Ordering::Release);
            self.total.store(total, Ordering::Release);
            on_progress(downloaded, total);
        };
        download(root, model, &self.cancel, &mut progress)?;
        Ok(!self.cancel.load(Ordering::Acquire))
    }
}

#[derive(Deserialize)]
struct Manifest {
    layers: Vec<Layer>,
}

#[derive(Deserialize)]
struct Layer {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest: String,
    size: u64,
}

/// Splits `llama3.2:3b` into the registry repository (`library/llama3.2`) and tag (`3b`),
/// rejecting anything that could escape the model directory.
fn parse_name(name: &str) -> Result<(String, String)> {
    let (repo, tag) = name.split_once(':').unwrap_or((name, "latest"));
    let valid = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    let mut parts = repo.split('/');
    let (first, second, rest) = (parts.next(), parts.next(), parts.next());
    let ok = match (first, second, rest) {
        (Some(a), None, _) => valid(a),
        (Some(a), Some(b), None) => valid(a) && valid(b),
        _ => false,
    };
    if !ok || !valid(tag) {
        bail!("\"{name}\" is not a valid Ollama model name (expected e.g. llama3.2:3b)");
    }
    let repo = if repo.contains('/') {
        repo.to_string()
    } else {
        format!("library/{repo}")
    };
    Ok((repo, tag.to_string()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn manifest_path(root: &Path, repo: &str, tag: &str) -> PathBuf {
    root.join("manifests/registry.ollama.ai").join(repo).join(tag)
}

/// Bounds every request: no answer within [`CONNECT_TIMEOUT`] or a request not done within
/// [`REQUEST_TIMEOUT`] is an error, so a dead connection cannot hang a download.
fn agent() -> Agent {
    Agent::config_builder()
        .timeout_resolve(Some(CONNECT_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into()
}

/// Downloads `name` into `root`, calling `progress(downloaded, total)` as bytes arrive.
/// Returns early if `cancel` is set; a partial file is kept so the next attempt resumes.
pub fn download(
    root: &Path,
    name: &str,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    let (repo, tag) = parse_name(name)?;
    let agent = agent();
    let manifest_route = format!("/v2/{repo}/manifests/{tag}");
    let manifest_url = format!("{REGISTRY}{manifest_route}");
    let started = Instant::now();
    let response = agent
        .get(&manifest_url)
        .header("Accept", "application/vnd.docker.distribution.manifest.v2+json")
        .call();
    let mut response = match response {
        Ok(response) => response,
        Err(err) => {
            warn!(
                target: NETWORK_TARGET,
                method = "GET",
                host = REGISTRY_HOST,
                path = manifest_route,
                bytes_sent = 0,
                duration_ms = elapsed_ms(started),
                model = name,
                purpose = PURPOSE,
                error = %err,
                "HTTP request failed"
            );
            return Err(anyhow!("fetching {manifest_url}: {err} (is \"{name}\" on ollama.com/library?)"));
        }
    };
    let manifest_bytes = response.body_mut().read_to_vec();
    info!(
        target: NETWORK_TARGET,
        method = "GET",
        host = REGISTRY_HOST,
        path = manifest_route,
        status = response.status().as_u16(),
        bytes_sent = 0,
        bytes_received = manifest_bytes.as_ref().map_or(0, Vec::len),
        duration_ms = elapsed_ms(started),
        model = name,
        purpose = PURPOSE,
        "HTTP request"
    );
    let manifest_bytes = manifest_bytes.context("reading manifest")?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).context("parsing manifest")?;
    let layer = manifest
        .layers
        .iter()
        .find(|layer| layer.media_type == MODEL_LAYER)
        .ok_or_else(|| anyhow!("\"{name}\" has no model weights"))?;
    let expected = layer
        .digest
        .strip_prefix("sha256:")
        .filter(|d| d.len() == 64 && d.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow!("unsupported digest {}", layer.digest))?
        .to_ascii_lowercase();

    let blobs = root.join("blobs");
    fs::create_dir_all(&blobs)?;
    let blob = blobs.join(format!("sha256-{expected}"));
    if !blob.is_file() {
        let partial = blobs.join(format!("sha256-{expected}.partial"));
        let path = format!("/v2/{repo}/blobs/{}", layer.digest);
        let url = format!("{REGISTRY}{path}");
        let mut traffic = BlobTraffic::new(name, &path, layer.size);
        let fetched = fetch_blob(&agent, &url, &partial, &expected, cancel, progress, &mut traffic);
        traffic.end(&fetched);
        if !fetched? {
            return Ok(());
        }
        fs::rename(&partial, &blob)?;
    }

    // The manifest goes last: a model only counts as installed once its weights are complete.
    let path = manifest_path(root, &repo, &tag);
    fs::create_dir_all(path.parent().expect("manifest path has a parent"))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &manifest_bytes).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
    info!(model = name, "model downloaded");
    Ok(())
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The requests one blob download made and the bytes they brought in, for the network log.
struct BlobTraffic<'a> {
    model: &'a str,
    path: &'a str,
    total_bytes: u64,
    started: Instant,
    requests: u64,
    received: u64,
}

impl<'a> BlobTraffic<'a> {
    fn new(model: &'a str, path: &'a str, total_bytes: u64) -> Self {
        Self {
            model,
            path,
            total_bytes,
            started: Instant::now(),
            requests: 0,
            received: 0,
        }
    }

    fn start(&self, resume_from: u64) {
        info!(
            target: NETWORK_TARGET,
            host = REGISTRY_HOST,
            path = self.path,
            model = self.model,
            total_bytes = self.total_bytes,
            resume_from,
            purpose = PURPOSE,
            "model download started"
        );
    }

    fn end(&self, outcome: &Result<bool>) {
        let duration_ms = elapsed_ms(self.started);
        match outcome {
            Ok(finished) => info!(
                target: NETWORK_TARGET,
                host = REGISTRY_HOST,
                path = self.path,
                model = self.model,
                requests = self.requests,
                bytes_received = self.received,
                total_bytes = self.total_bytes,
                duration_ms,
                purpose = PURPOSE,
                "{}",
                if *finished { "model download finished" } else { "model download stopped" }
            ),
            Err(err) => warn!(
                target: NETWORK_TARGET,
                host = REGISTRY_HOST,
                path = self.path,
                model = self.model,
                requests = self.requests,
                bytes_received = self.received,
                total_bytes = self.total_bytes,
                duration_ms,
                purpose = PURPOSE,
                error = format!("{err:#}"),
                "model download failed"
            ),
        }
    }
}

/// What to do with the `have` bytes of a `.partial` file for a blob of `total` bytes.
#[derive(Debug, PartialEq, Eq)]
enum Resume {
    /// Longer than the blob, so not a prefix of it.
    Discard,
    /// Every byte is there; only the checksum is left to check.
    Complete,
    /// Fetch from this offset on.
    From(u64),
}

fn resume(have: u64, total: u64) -> Resume {
    match have.cmp(&total) {
        std::cmp::Ordering::Greater => Resume::Discard,
        std::cmp::Ordering::Equal => Resume::Complete,
        std::cmp::Ordering::Less => Resume::From(have),
    }
}

/// The inclusive byte range of the next request, at most `chunk` bytes from `done`.
fn next_range(done: u64, total: u64, chunk: u64) -> Option<(u64, u64)> {
    (done < total).then(|| (done, (done + chunk).min(total) - 1))
}

/// Fetches `url` into `partial` a range at a time, resuming what is already there. A range that
/// stalls fails the download; one cut short after some progress is asked for again from where it
/// stopped. `false` means cancelled.
fn fetch_blob(
    agent: &Agent,
    url: &str,
    partial: &Path,
    expected: &str,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
    traffic: &mut BlobTraffic,
) -> Result<bool> {
    let total = traffic.total_bytes;
    let have = fs::metadata(partial).map(|m| m.len()).unwrap_or(0);
    let mut done = match resume(have, total) {
        Resume::Discard => {
            fs::remove_file(partial)?;
            0
        }
        Resume::Complete => total,
        Resume::From(offset) => offset,
    };

    // The hash must cover the bytes already on disk too.
    let mut hasher = Sha256::new();
    if done > 0 {
        let mut existing = File::open(partial)?;
        let mut buf = vec![0u8; READ_BUF];
        loop {
            let n = existing.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }
    let mut file = OpenOptions::new().create(true).append(true).open(partial)?;
    progress(done, total);
    traffic.start(done);

    let mut buf = vec![0u8; READ_BUF];
    while let Some((start, end)) = next_range(done, total, RANGE_BYTES) {
        if cancel.load(Ordering::Acquire) {
            return Ok(false);
        }
        traffic.requests += 1;
        let mut response = agent
            .get(url)
            .header("Range", format!("bytes={start}-{end}"))
            .call()
            .map_err(|err| anyhow!("downloading {url}: {err}"))?;
        let status = response.status().as_u16();
        if status != 206 && !(status == 200 && start == 0) {
            bail!("downloading {url}: the server answered {status} to a ranged request");
        }
        let mut reader = response.body_mut().as_reader().take(end - start + 1);
        let mut got = 0u64;
        loop {
            if cancel.load(Ordering::Acquire) {
                file.flush()?;
                return Ok(false);
            }
            let n = match reader.read(&mut buf) {
                Ok(n) => n,
                Err(err) if got > 0 => {
                    warn!(%err, done, "download range cut short; continuing from where it stopped");
                    break;
                }
                Err(err) => return Err(anyhow!(err).context("download stalled")),
            };
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            got += n as u64;
            done += n as u64;
            traffic.received += n as u64;
            progress(done, total);
        }
        if got == 0 {
            bail!("download stalled: the server sent no data");
        }
    }
    file.flush()?;
    drop(file);

    if done != total || hex(&hasher.finalize()) != expected {
        let _ = fs::remove_file(partial);
        bail!("download is corrupt (size or checksum mismatch); try again");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_decides_from_the_partial_length() {
        assert_eq!(resume(0, 100), Resume::From(0));
        assert_eq!(resume(40, 100), Resume::From(40));
        assert_eq!(resume(100, 100), Resume::Complete);
        assert_eq!(resume(101, 100), Resume::Discard);
    }

    #[test]
    fn ranges_cover_the_rest_of_the_blob_in_chunks() {
        let mut ranges = Vec::new();
        let mut done = 10;
        while let Some((start, end)) = next_range(done, 35, 10) {
            ranges.push((start, end));
            done = end + 1;
        }
        assert_eq!(ranges, [(10, 19), (20, 29), (30, 34)]);
    }

    #[test]
    fn a_complete_blob_needs_no_range() {
        assert_eq!(next_range(100, 100, 10), None);
        assert_eq!(next_range(0, 0, 10), None);
    }

    #[test]
    fn a_range_resumes_mid_chunk() {
        assert_eq!(next_range(15, 100, 64), Some((15, 78)));
    }
}
