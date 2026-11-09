//! One model download, split in two so the window can show a summary and ask
//! before anything is written: `plan` looks the files up and checks the
//! destination, `execute` downloads them.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::catalog::{self, ModelEntry};
use crate::downloader::{self, Event, Events, RemoteFile, Source};
use crate::format::{COMPUTER, format_size};
use crate::hardware::{Fit, Hardware};
use crate::volumes::{self, Volume};

pub fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("model-downloader/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// Everything the user chose for one download.
#[derive(Clone)]
pub struct DownloadRequest {
    pub model: &'static ModelEntry,
    pub source: Source,
    /// Hugging Face only; Ollama tags come in a fixed quant.
    pub quant: String,
    pub volume: Volume,
}

/// A download that has been looked up and checked, ready to start.
pub struct DownloadPlan {
    pub req: DownloadRequest,
    pub files: Vec<RemoteFile>,
    /// Where each of `files` will be saved.
    pub dests: Vec<PathBuf>,
    /// Total size, when the source reports every file's size.
    pub total: Option<u64>,
    /// Bytes already on disk from an earlier run.
    pub have: u64,
    pub fit: Option<Fit>,
    /// Reasons to think twice, shown before the download starts.
    pub warnings: Vec<String>,
}

impl DownloadPlan {
    /// The folder the model is saved in.
    pub fn folder(&self) -> PathBuf {
        self.dests[0]
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default()
    }

    /// True when the model probably won't run on this computer.
    pub fn too_big(&self) -> bool {
        self.fit == Some(Fit::TooBig)
    }

    /// Bytes of the model on disk right now, complete or partial.
    pub fn saved_bytes(&self) -> u64 {
        self.dests
            .iter()
            .map(|d| downloader::existing_bytes(d))
            .sum()
    }

    /// Bytes still to download, if the total is known.
    pub fn remaining(&self) -> Option<u64> {
        self.total.map(|t| t.saturating_sub(self.have))
    }
}

/// Look up the files for `req` and check they'll fit where they're going.
pub async fn plan(
    client: &reqwest::Client,
    req: DownloadRequest,
    hw: &Hardware,
) -> Result<DownloadPlan> {
    let files = downloader::resolve_download(client, req.model, req.source, &req.quant)
        .await
        .with_context(|| {
            format!(
                "Couldn't get {} from {}",
                req.model.display_name,
                req.source.label()
            )
        })?;

    let dests: Vec<PathBuf> = files
        .iter()
        .map(|f| downloader::model_dest_path(&req.volume.path, req.model, &f.filename))
        .collect();
    let have: u64 = dests.iter().map(|d| downloader::existing_bytes(d)).sum();
    let sizes: Vec<Option<u64>> = files.iter().map(|f| f.size).collect();
    check_destination(&req.volume, &sizes, have)?;

    let total: Option<u64> = sizes.iter().copied().sum();
    let fit = total.map(|t| hw.fit(t));
    let mut warnings = Vec::new();
    match fit {
        Some(Fit::TooBig) => warnings.push(format!(
            "This model is too big for this {COMPUTER}. It will download, but probably won't run. \
             A smaller model or quant will work better."
        )),
        Some(Fit::Tight) => {
            warnings.push("This model will only just fit. Expect it to run slowly, and close other apps first.".into());
        }
        _ => {}
    }
    if let Some(t) = total
        && hw.short_on_free_memory(t)
    {
        warnings.push(format!(
            "Only {} of memory is free right now. Close other apps, like browsers, before running this model.",
            format_size(hw.free_ram_bytes)
        ));
    }
    if catalog::is_abliterated(req.model) {
        warnings.push(catalog::ABLITERATED_NOTICE.into());
    }

    Ok(DownloadPlan {
        req,
        files,
        dests,
        total,
        have,
        fit,
        warnings,
    })
}

/// Refuse a destination the files can't be written to: FAT32 caps files at
/// 4 GB, and the drive needs room for what isn't downloaded yet. Unknown
/// sizes (`None`) count as too big for FAT32; a free space of 0 means it
/// couldn't be measured, so it isn't held against the drive.
pub fn check_destination(volume: &Volume, sizes: &[Option<u64>], have: u64) -> Result<()> {
    if volume.is_fat32()
        && sizes
            .iter()
            .any(|s| s.is_none_or(|s| s > volumes::FAT32_MAX_FILE))
    {
        bail!(
            "{} is formatted as FAT32, which can't hold files over 4 GB.\n\
             Reformat it as {}, or choose another drive.",
            volume.name(),
            volumes::BIG_FILE_FORMATS
        );
    }

    let total: Option<u64> = sizes.iter().copied().sum();
    if let Some(needed) = total.map(|t| t.saturating_sub(have))
        && needed > volume.free_bytes
        && volume.free_bytes > 0
    {
        bail!(
            "Not enough free space on {}: need {}, have {}.\n\
             Free up space or choose another drive.",
            volume.name(),
            format_size(needed),
            format_size(volume.free_bytes)
        );
    }
    Ok(())
}

/// Download every file in the plan, in order. Each file announces itself with
/// `Event::File` before its progress events.
pub async fn execute(client: &reqwest::Client, plan: &DownloadPlan, events: &Events) -> Result<()> {
    for (index, (file, dest)) in plan.files.iter().zip(&plan.dests).enumerate() {
        downloader::ensure_parent_dir(dest).await?;
        let _ = events.send(Event::File { index });
        downloader::download_with_progress(client, file, dest, events)
            .await
            .context("Download failed")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volumes::Kind;

    const GB: u64 = 1_000_000_000;

    fn drive(fs: &str, free: u64) -> Volume {
        Volume {
            letter: "E:".into(),
            label: "Test".into(),
            filesystem: fs.into(),
            path: PathBuf::from("E:\\"),
            free_bytes: free,
            total_bytes: free * 2,
            kind: Kind::Drive,
        }
    }

    #[test]
    fn fat32_takes_small_files_only() {
        let fat = drive("FAT32", 100 * GB);
        assert!(check_destination(&fat, &[Some(3 * GB)], 0).is_ok());
        let err = check_destination(&fat, &[Some(5 * GB)], 0).unwrap_err();
        assert!(err.to_string().contains("FAT32"), "{err}");
        assert!(
            check_destination(&fat, &[None], 0).is_err(),
            "unknown size could be over 4 GB"
        );
    }

    #[test]
    fn needs_room_for_what_is_left() {
        let vol = drive("exFAT", 10 * GB);
        assert!(check_destination(&vol, &[Some(8 * GB)], 0).is_ok());
        let err = check_destination(&vol, &[Some(6 * GB), Some(6 * GB)], 0).unwrap_err();
        assert!(err.to_string().contains("Not enough free space"), "{err}");
        assert!(
            check_destination(&vol, &[Some(6 * GB), Some(6 * GB)], 4 * GB).is_ok(),
            "bytes from an earlier run don't need room again"
        );
    }

    #[test]
    fn unmeasured_free_space_is_not_held_against_the_drive() {
        assert!(check_destination(&drive("NTFS", 0), &[Some(50 * GB)], 0).is_ok());
    }
}
