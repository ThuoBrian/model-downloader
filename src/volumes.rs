use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::format::COMPUTER;

#[cfg_attr(windows, path = "volumes/windows.rs")]
#[cfg_attr(target_os = "macos", path = "volumes/macos.rs")]
pub(crate) mod sys;

/// Largest file a FAT32 volume can hold (4 GiB - 1 byte).
pub const FAT32_MAX_FILE: u64 = 4 * 1_073_741_824 - 1;

/// Formats that hold files over 4 GB and that this OS can write to.
#[cfg(windows)]
pub const BIG_FILE_FORMATS: &str = "exFAT or NTFS";
#[cfg(target_os = "macos")]
pub const BIG_FILE_FORMATS: &str = "exFAT or APFS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A drive other than the system drive; models go in its root.
    Drive,
    /// The drive the OS is on; models go in the user's folder.
    System,
    /// A folder the user chose.
    Folder,
}

/// Somewhere models can be saved: a drive or a chosen folder. Models land
/// in `<path>/models/<model-id>/`.
#[derive(Debug, Clone)]
pub struct Volume {
    /// Drive letter with colon, e.g. "E:" (empty on macOS)
    pub letter: String,
    /// Volume label, e.g. "MyExternal" (may be empty)
    pub label: String,
    /// File system name, e.g. "NTFS", "exFAT", "FAT32", "APFS"
    pub filesystem: String,
    pub path: PathBuf,
    /// Available space in bytes (0 if we couldn't determine it)
    pub free_bytes: u64,
    /// Total capacity in bytes
    pub total_bytes: u64,
    pub kind: Kind,
}

impl Volume {
    /// "E: MyExternal", "E:" when the drive has no label, "MyExternal" on a
    /// Mac, "C: This PC" or "This Mac" for the system drive, or the folder
    /// path for a chosen folder.
    pub fn name(&self) -> String {
        match self.kind {
            Kind::System if self.letter.is_empty() => format!("This {}", COMPUTER),
            Kind::System => format!("{} This {}", self.letter, COMPUTER),
            Kind::Folder => self.path.display().to_string(),
            Kind::Drive if self.label.is_empty() => self.letter.clone(),
            Kind::Drive if self.letter.is_empty() => self.label.clone(),
            Kind::Drive => format!("{} {}", self.letter, self.label),
        }
    }

    pub fn is_fat32(&self) -> bool {
        self.filesystem.eq_ignore_ascii_case("FAT32") || self.filesystem.eq_ignore_ascii_case("FAT")
    }

    pub fn total_gb(&self) -> f64 {
        self.total_bytes as f64 / 1_073_741_824.0
    }
}

/// Use any folder (created if missing) as the place to save models.
pub fn from_folder(path: &Path) -> Result<Volume> {
    // Expand ~ to the home folder, as a shell would.
    let path = match (cfg!(unix), path.strip_prefix("~"), std::env::var_os("HOME")) {
        (true, Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => path.to_path_buf(),
    };
    let folder = std::path::absolute(&path)
        .with_context(|| format!("'{}' isn't a valid folder path.", path.display()))?;
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("Couldn't create the folder {}.", folder.display()))?;

    let mut vol = sys::folder_volume(&folder)?;
    vol.path = folder;
    vol.kind = Kind::Folder;
    Ok(vol)
}

/// Find places to save models: external disks, extra internal disks and
/// network drives first (largest free space first), then the system drive,
/// which saves into the user's folder.
pub fn detect_volumes() -> Result<Vec<Volume>> {
    let (mut volumes, system) = sys::detect_volumes()?;
    volumes.sort_by_key(|v| std::cmp::Reverse(v.free_bytes));
    volumes.extend(system);
    Ok(volumes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vol(letter: &str, label: &str, fs: &str) -> Volume {
        Volume {
            letter: letter.into(),
            label: label.into(),
            filesystem: fs.into(),
            path: PathBuf::from(format!("{letter}\\")),
            free_bytes: 0,
            total_bytes: 0,
            kind: Kind::Drive,
        }
    }

    #[test]
    fn mac_drives_go_by_name() {
        let v = vol("", "MyExternal", "exFAT");
        assert_eq!(v.name(), "MyExternal");
    }

    #[test]
    fn detects_fat32() {
        assert!(vol("E:", "", "FAT32").is_fat32());
        assert!(!vol("E:", "", "exFAT").is_fat32());
        assert!(!vol("E:", "", "NTFS").is_fat32());
    }

    #[test]
    fn system_drive_is_listed_last_and_uses_the_user_folder() {
        let vols = detect_volumes().unwrap();
        let last = vols.last().expect("the system drive is always listed");
        assert_eq!(last.kind, Kind::System);
        let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        assert_eq!(Some(last.path.as_os_str()), std::env::var_os(home).as_deref());
        assert_eq!(vols.iter().filter(|v| v.kind == Kind::System).count(), 1);
    }

    #[test]
    fn any_folder_can_be_used() {
        let dir = std::env::temp_dir().join(format!("model-downloader-test-{}", std::process::id()));
        let vol = from_folder(&dir).unwrap();
        assert!(dir.is_dir(), "the folder is created");
        assert_eq!(vol.kind, Kind::Folder);
        assert_eq!(vol.path, dir);
        assert!(vol.free_bytes > 0 && !vol.filesystem.is_empty());
        std::fs::remove_dir(&dir).unwrap();
    }
}
