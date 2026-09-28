//! Downloads models from the Ollama registry into Makora's own model directory.
//!
//! The directory uses Ollama's on-disk layout (`manifests/…` and `blobs/sha256-…`), so
//! [`llm::resolve_model`](crate::core::llm::resolve_model) finds a downloaded model the same
//! way it finds one pulled by Ollama itself. Only the weights layer is fetched; that is all
//! llama.cpp needs.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::info;

const REGISTRY: &str = "https://registry.ollama.ai";
const MODEL_LAYER: &str = "application/vnd.ollama.image.model";
const CHUNK: usize = 256 * 1024;

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

/// Downloads `name` into `root`, calling `progress(downloaded, total)` as bytes arrive.
/// Returns early if `cancel` is set; a partial file is kept so the next attempt resumes.
pub fn download(
    root: &Path,
    name: &str,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    let (repo, tag) = parse_name(name)?;
    let manifest_url = format!("{REGISTRY}/v2/{repo}/manifests/{tag}");
    let manifest_bytes = ureq::get(&manifest_url)
        .header("Accept", "application/vnd.docker.distribution.manifest.v2+json")
        .call()
        .map_err(|err| {
            anyhow!("fetching {manifest_url}: {err} (is \"{name}\" on ollama.com/library?)")
        })?
        .body_mut()
        .read_to_vec()
        .context("reading manifest")?;
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
        let url = format!("{REGISTRY}/v2/{repo}/blobs/{}", layer.digest);
        if !fetch_blob(&url, &partial, layer.size, &expected, cancel, progress)? {
            return Ok(());
        }
        fs::rename(&partial, &blob)?;
    }

    // The manifest goes last: a model only counts as installed once its weights are complete.
    let path = manifest_path(root, &repo, &tag);
    fs::create_dir_all(path.parent().expect("manifest path has a parent"))?;
    fs::write(&path, &manifest_bytes)?;
    info!(model = name, "model downloaded");
    Ok(())
}

/// Streams `url` into `partial`, resuming what is already there. `false` means cancelled.
fn fetch_blob(
    url: &str,
    partial: &Path,
    total: u64,
    expected: &str,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<bool> {
    let mut hasher = Sha256::new();
    let mut have = fs::metadata(partial).map(|m| m.len()).unwrap_or(0);
    if have > total {
        fs::remove_file(partial)?;
        have = 0;
    }
    let mut request = ureq::get(url);
    if have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let mut response = request
        .call()
        .map_err(|err| anyhow!("downloading {url}: {err}"))?;
    let resumed = have > 0 && response.status().as_u16() == 206;

    let mut file = if resumed {
        // The hash must cover the bytes already on disk too.
        let mut existing = File::open(partial)?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = existing.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        OpenOptions::new().append(true).open(partial)?
    } else {
        have = 0;
        File::create(partial)?
    };

    let mut reader = response.body_mut().as_reader();
    let mut buf = vec![0u8; CHUNK];
    let mut done = have;
    progress(done, total);
    loop {
        if cancel.load(Ordering::Acquire) {
            return Ok(false);
        }
        let n = reader.read(&mut buf).context("download interrupted")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        done += n as u64;
        progress(done, total);
    }
    file.flush()?;
    drop(file);

    if done != total || hex(&hasher.finalize()) != expected {
        let _ = fs::remove_file(partial);
        bail!("download is corrupt (size or checksum mismatch); try again");
    }
    Ok(true)
}
