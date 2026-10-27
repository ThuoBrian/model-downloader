use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use indicatif::ProgressBar;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::catalog::ModelEntry;
use crate::ui;

const HF_BASE: &str = "https://huggingface.co";
const OLLAMA_REGISTRY: &str = "https://registry.ollama.ai";

#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum Source {
    #[value(name = "hf", alias = "huggingface")]
    HuggingFace,
    Ollama,
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::HuggingFace => "Hugging Face",
            Source::Ollama => "Ollama",
        }
    }
}

/// One file to download. Large quants on Hugging Face are split into several
/// parts (`*-00001-of-00002.gguf`), so a model may resolve to more than one.
#[derive(Debug)]
pub struct RemoteFile {
    pub url: String,
    pub filename: String,
    pub size: Option<u64>,
    /// SHA-256 the source publishes for the file, as lowercase hex.
    pub sha256: Option<String>,
}

/// Resolve the download URL(s) and output filename(s) for a model from a given source.
pub async fn resolve_download(
    client: &reqwest::Client,
    model: &ModelEntry,
    source: Source,
    quant: &str,
) -> Result<Vec<RemoteFile>> {
    match source {
        Source::HuggingFace => resolve_hf(client, model, quant).await,
        Source::Ollama => resolve_ollama(client, model).await.map(|f| vec![f]),
    }
}

// ── Hugging Face ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct HfEntry {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    size: Option<u64>,
    /// Present for files stored in Git LFS (all GGUFs); `oid` is the SHA-256.
    lfs: Option<HfLfs>,
}

#[derive(Deserialize)]
struct HfLfs {
    oid: String,
}

async fn hf_list(client: &reqwest::Client, repo: &str, subdir: &str) -> Result<Vec<HfEntry>> {
    let url = format!("{}/api/models/{}/tree/main/{}", HF_BASE, repo, subdir);
    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to reach Hugging Face API: {}", url))?;

    if !resp.status().is_success() {
        bail!(
            "Hugging Face API returned {} for repo {}",
            resp.status(),
            repo
        );
    }

    resp.json()
        .await
        .context("Failed to parse Hugging Face file listing")
}

#[derive(Deserialize)]
struct HfModelInfo {
    /// Downloads over the last 30 days.
    downloads: u64,
}

/// How many times a Hugging Face repo was downloaded in the last 30 days.
pub async fn hf_downloads(client: &reqwest::Client, repo: &str) -> Result<u64> {
    let url = format!("{}/api/models/{}", HF_BASE, repo);
    let info: HfModelInfo = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to reach Hugging Face API: {}", url))?
        .error_for_status()?
        .json()
        .await
        .context("Failed to parse Hugging Face model info")?;
    Ok(info.downloads)
}

#[derive(Deserialize)]
struct HfSearchHit {
    id: String,
    downloads: u64,
}

/// An abliterated GGUF repo found by searching Hugging Face, not from the catalog.
pub struct LiveModel {
    pub repo: String,
    pub q4_bytes: u64,
    /// Downloads over the last 30 days.
    pub downloads: u64,
}

/// bartowski's most downloaded abliterated GGUF repos that have a
/// default-quant file, so every one returned can be downloaded.
pub async fn hf_abliterated(client: &reqwest::Client) -> Result<Vec<LiveModel>> {
    let url = format!(
        "{}/api/models?author=bartowski&search=abliterated&sort=downloads&direction=-1&limit=20",
        HF_BASE
    );
    let hits: Vec<HfSearchHit> = client
        .get(&url)
        .send()
        .await
        .context("Failed to reach Hugging Face API")?
        .error_for_status()?
        .json()
        .await
        .context("Failed to parse Hugging Face search results")?;

    let found = futures_util::future::join_all(hits.into_iter().map(|hit| async move {
        let files = resolve_hf_repo(client, &hit.id, crate::catalog::DEFAULT_QUANT).await.ok()?;
        let q4_bytes = files.iter().map(|f| f.size).sum::<Option<u64>>()?;
        Some(LiveModel {
            repo: hit.id,
            q4_bytes,
            downloads: hit.downloads,
        })
    }))
    .await;
    Ok(found.into_iter().flatten().collect())
}

/// HF repos store GGUF files on "main" with names like `<model>-Q4_K_M.gguf`.
/// Quants too large for one file live in a `<model>-Q8_0/` directory as split
/// parts. We match the quant exactly (so Q6_K never picks Q6_K_L) and return
/// every file needed.
async fn resolve_hf(client: &reqwest::Client, model: &ModelEntry, quant: &str) -> Result<Vec<RemoteFile>> {
    let repo = model
        .hf_repo
        .ok_or_else(|| anyhow::anyhow!("Model {} has no Hugging Face repo", model.id))?;
    resolve_hf_repo(client, repo, quant).await
}

async fn resolve_hf_repo(client: &reqwest::Client, repo: &str, quant: &str) -> Result<Vec<RemoteFile>> {
    let entries = hf_list(client, repo, "").await?;

    let quant_lower = quant.to_lowercase();
    let file_suffix = format!("-{}.gguf", quant_lower);
    let dir_suffix = format!("-{}", quant_lower);

    let single = entries
        .iter()
        .find(|e| e.kind == "file" && e.path.to_lowercase().ends_with(&file_suffix));

    let selected: Vec<&HfEntry>;
    let split_entries;
    if let Some(file) = single {
        selected = vec![file];
    } else if let Some(dir) = entries
        .iter()
        .find(|e| e.kind == "directory" && e.path.to_lowercase().ends_with(&dir_suffix))
    {
        split_entries = hf_list(client, repo, &dir.path).await?;
        let mut parts: Vec<&HfEntry> = split_entries
            .iter()
            .filter(|e| e.kind == "file" && e.path.to_lowercase().ends_with(".gguf"))
            .collect();
        parts.sort_by(|a, b| a.path.cmp(&b.path));
        if parts.is_empty() {
            bail!("Directory {} in HF repo {} contains no .gguf files", dir.path, repo);
        }
        selected = parts;
    } else {
        let mut available: Vec<&str> = entries
            .iter()
            .filter_map(|e| {
                let name = e.path.strip_suffix(".gguf").unwrap_or(&e.path);
                let is_quant = e.kind == "directory" || e.path.ends_with(".gguf");
                is_quant.then(|| name.rsplit('-').next().unwrap_or(name))
            })
            .filter(|q| !q.eq_ignore_ascii_case("imatrix"))
            .collect();
        available.sort_unstable();
        available.dedup();
        bail!(
            "No {} GGUF found in HF repo {}.\nQuants in this repo: {}",
            quant,
            repo,
            available.join(", ")
        );
    }

    Ok(selected
        .into_iter()
        .map(|e| {
            // Keep only the basename for the local file — HF paths may include
            // the split directory, and we defend against traversal.
            let filename = e.path.rsplit('/').next().unwrap_or(&e.path).to_string();
            RemoteFile {
                url: format!("{}/{}/resolve/main/{}", HF_BASE, repo, e.path),
                filename,
                size: e.size,
                sha256: e.lfs.as_ref().map(|l| l.oid.to_lowercase()),
            }
        })
        .collect())
}

// ── Ollama ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OllamaManifest {
    layers: Vec<OllamaLayer>,
}

#[derive(Deserialize)]
struct OllamaLayer {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest: String,
    size: u64,
}

/// Ollama stores models as manifest + blob layers. The layer with
/// mediaType "application/vnd.ollama.image.model" is the model data
/// (which is the GGUF content for GGUF-based models). The quant is fixed
/// by the tag, so there is no quant parameter here.
async fn resolve_ollama(client: &reqwest::Client, model: &ModelEntry) -> Result<RemoteFile> {
    let tag = model
        .ollama_tag
        .ok_or_else(|| anyhow::anyhow!("Model {} has no Ollama tag", model.id))?;

    // Ollama tags look like "library:tag", e.g. "deepseek-r1:14b"
    let Some((library, tag_name)) = tag.split_once(':') else {
        bail!("Invalid Ollama tag format: {} (expected 'library:tag')", tag);
    };

    let manifest_url = format!(
        "{}/v2/library/{}/manifests/{}",
        OLLAMA_REGISTRY, library, tag_name
    );

    let resp = client
        .get(&manifest_url)
        .header("Accept", "application/vnd.docker.distribution.manifest.v2+json")
        .send()
        .await
        .with_context(|| format!("Failed to reach Ollama registry: {}", manifest_url))?;

    if !resp.status().is_success() {
        bail!(
            "Ollama registry returned {} for {}:{}",
            resp.status(),
            library,
            tag_name
        );
    }

    let manifest: OllamaManifest = resp
        .json()
        .await
        .context("Failed to parse Ollama manifest")?;

    let model_layer = manifest
        .layers
        .iter()
        .find(|l| l.media_type == "application/vnd.ollama.image.model")
        .ok_or_else(|| anyhow::anyhow!("No model layer found in Ollama manifest for {}", tag))?;

    Ok(RemoteFile {
        url: format!(
            "{}/v2/library/{}/blobs/{}",
            OLLAMA_REGISTRY, library, model_layer.digest
        ),
        filename: format!("{}-{}.gguf", library, tag_name),
        size: Some(model_layer.size),
        // Blobs are content-addressed: the digest is the file's SHA-256.
        sha256: model_layer.digest.strip_prefix("sha256:").map(str::to_lowercase),
    })
}

// ── Download with progress + resume ─────────────────────────────────────────

/// Path of the in-progress download for `dest` (`<dest>.part`).
fn part_path(dest: &Path) -> PathBuf {
    let mut s: OsString = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Bytes of `dest` already on disk, complete or partial.
pub fn existing_bytes(dest: &Path) -> u64 {
    [dest.to_path_buf(), part_path(dest)]
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .max()
        .unwrap_or(0)
}

/// Download a file with streaming, progress bar, and resume support.
/// Data is written to `<dest>.part` and renamed to `dest` only once the full
/// file has arrived and, when the source publishes a SHA-256, matches it. So a
/// file at `dest` is always complete. `label` prefixes the progress bar
/// (e.g. "part 1/2").
pub async fn download_with_progress(
    client: &reqwest::Client,
    file: &RemoteFile,
    dest: &Path,
    label: &str,
) -> Result<()> {
    let url = file.url.as_str();
    let part = part_path(dest);
    let name = dest.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();

    // Get total size — either from hint or HEAD request
    let total_size = match file.size {
        Some(s) => s,
        None => {
            let head = client
                .head(url)
                .send()
                .await
                .context("Failed to get file size")?;
            if !head.status().is_success() {
                bail!("Download failed with HTTP {}", head.status());
            }
            // Read the header directly: for HEAD responses the body is empty,
            // so reqwest's content_length() reports 0.
            head.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        }
    };

    if let Ok(meta) = tokio::fs::metadata(dest).await {
        if total_size == 0 || meta.len() == total_size {
            ui::success(format!("Already downloaded: {name}"));
            return Ok(());
        }
        // A wrong-sized file at `dest` (e.g. left by an older version of this
        // tool that wrote partial data in place) — resume it as a .part file.
        if !part.exists() {
            tokio::fs::rename(dest, &part).await?;
        } else {
            tokio::fs::remove_file(dest).await?;
        }
    }

    // Check for existing partial file to resume from
    let mut existing_size = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    if total_size > 0 && existing_size > total_size {
        // Partial is larger than the real file — it's corrupt, start over.
        existing_size = 0;
    }

    // Bytes kept from an earlier run go into the checksum first.
    let mut hasher = Sha256::new();
    if file.sha256.is_some() && existing_size > 0 {
        hasher = hash_prefix(&part, existing_size, &name).await?;
    }

    if total_size > 0 && existing_size == total_size {
        check_hash(hasher, file, &part, &name)?;
        tokio::fs::rename(&part, dest).await?;
        ui::success(format!("Already downloaded: {name}"));
        return Ok(());
    }

    // Set up range request for resume
    let mut req = client.get(url);
    if existing_size > 0 {
        req = req.header("Range", format!("bytes={}-", existing_size));
    }

    let resp = req.send().await.context("Failed to start download")?;

    if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && total_size == 0 {
        // Size unknown and the server has nothing past our offset — treat as complete.
        check_hash(hasher, file, &part, &name)?;
        tokio::fs::rename(&part, dest).await?;
        ui::success(format!("Already downloaded: {name}"));
        return Ok(());
    }
    if !resp.status().is_success() {
        bail!("Download failed with HTTP {}", resp.status());
    }

    // A 200 (not 206) means the server ignored our Range header — start over.
    let is_resuming = existing_size > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let offset = if is_resuming { existing_size } else { 0 };
    if !is_resuming {
        hasher = Sha256::new();
    }
    let content_length = resp.content_length().unwrap_or(0);
    let expected_total = if total_size > 0 { total_size } else { offset + content_length };

    if is_resuming {
        ui::hint(format!(
            "Resuming {name} at {} ({:.0}%)",
            ui::format_size(offset),
            (offset as f64 / expected_total.max(1) as f64) * 100.0
        ));
    }

    let pb = ProgressBar::new(expected_total.max(1))
        .with_style(ui::download_style())
        .with_prefix(label.to_string())
        .with_position(offset);
    // Keep the spinner and ETA moving even while the network stalls.
    pb.enable_steady_tick(std::time::Duration::from_millis(200));
    pb.reset_eta();

    // Open .part for append if resuming, truncate otherwise
    let mut out = if is_resuming {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part)
            .await
            .context("Failed to open partial file for append")?
    } else {
        File::create(&part)
            .await
            .with_context(|| format!("Failed to create file: {}", part.display()))?
    };

    let mut stream = resp.bytes_stream();
    let mut written = offset;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("Error reading download stream (re-run to resume)")?;
        out.write_all(&chunk)
            .await
            .context("Failed to write downloaded data")?;
        hasher.update(&chunk);
        written += chunk.len() as u64;
        pb.inc(chunk.len() as u64);
    }

    out.flush().await?;
    drop(out);
    pb.finish();

    if expected_total > 0 && written != expected_total {
        bail!(
            "Incomplete download: got {} of {} bytes. Re-run the same command to resume.",
            written,
            expected_total
        );
    }

    check_hash(hasher, file, &part, &name)?;
    tokio::fs::rename(&part, dest)
        .await
        .with_context(|| format!("Failed to move {} into place", part.display()))?;

    Ok(())
}

/// SHA-256 of the first `len` bytes of a partial download. Runs on a
/// blocking thread with its own progress bar: a large part can take a while.
async fn hash_prefix(part: &Path, len: u64, name: &str) -> Result<Sha256> {
    let pb = ProgressBar::new(len)
        .with_style(ui::task_style())
        .with_message(format!("Checking the part of {name} already downloaded"));
    let (part, bar) = (part.to_path_buf(), pb.clone());
    let hasher = tokio::task::spawn_blocking(move || -> std::io::Result<Sha256> {
        use std::io::Read;
        let mut reader = std::fs::File::open(&part)?.take(len);
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                return Ok(hasher);
            }
            hasher.update(&buf[..n]);
            bar.inc(n as u64);
        }
    })
    .await?
    .context("Couldn't read the partly downloaded file")?;
    pb.finish_and_clear();
    Ok(hasher)
}

/// Compare against the published SHA-256. On a mismatch the file is useless,
/// so delete it rather than leave a broken model behind.
fn check_hash(hasher: Sha256, file: &RemoteFile, part: &Path, name: &str) -> Result<()> {
    let Some(expected) = &file.sha256 else {
        return Ok(());
    };
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if actual == *expected {
        ui::success(format!("Checksum OK: {name}"));
        return Ok(());
    }
    let _ = std::fs::remove_file(part);
    bail!(
        "{name} doesn't match its published checksum, so it was deleted.\n\
         It was probably damaged in transit. Run the download again."
    );
}

/// Compute the destination path for a model file on a given volume.
pub fn model_dest_path(volume_path: &Path, model: &ModelEntry, filename: &str) -> PathBuf {
    let mut path = volume_path.to_path_buf();
    path.push("models");
    path.push(model.id);
    path.push(filename);
    path
}

/// Ensure the parent directory exists for a file path.
pub async fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // SHA-256 of "abc" (FIPS 180-2 test vector).
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn remote(sha256: Option<&str>) -> RemoteFile {
        RemoteFile {
            url: String::new(),
            filename: "test.gguf".into(),
            size: Some(3),
            sha256: sha256.map(str::to_string),
        }
    }

    fn temp_file(tag: &str, contents: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("model-downloader-{tag}-{}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// Talks to Hugging Face, so it's skipped by default:
    /// `cargo test -- --ignored finds_abliterated_models_online`
    #[tokio::test]
    #[ignore]
    async fn finds_abliterated_models_online() {
        let found = hf_abliterated(&crate::http_client().unwrap()).await.unwrap();
        assert!(found.len() >= 5, "found {}", found.len());
        assert!(found.iter().all(|m| m.q4_bytes > 0));
    }

    #[test]
    fn matching_checksum_passes_and_keeps_the_file() {
        let path = temp_file("ok", b"abc");
        let mut hasher = Sha256::new();
        hasher.update(b"abc");
        check_hash(hasher, &remote(Some(ABC)), &path, "test.gguf").unwrap();
        assert!(path.exists());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn wrong_checksum_fails_and_deletes_the_file() {
        let path = temp_file("bad", b"abd");
        let mut hasher = Sha256::new();
        hasher.update(b"abd");
        let err = check_hash(hasher, &remote(Some(ABC)), &path, "test.gguf").unwrap_err();
        assert!(err.to_string().contains("doesn't match"), "{err}");
        assert!(!path.exists(), "a corrupt download is deleted");
    }

    #[test]
    fn no_published_checksum_is_accepted() {
        let path = temp_file("none", b"anything");
        check_hash(Sha256::new(), &remote(None), &path, "test.gguf").unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn resuming_hashes_only_the_kept_prefix() {
        // A part file with 3 good bytes plus junk past the resume point.
        let path = temp_file("prefix", b"abcJUNK");
        let hasher = hash_prefix(&path, 3, "test.gguf").await.unwrap();
        check_hash(hasher, &remote(Some(ABC)), &path, "test.gguf").unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
