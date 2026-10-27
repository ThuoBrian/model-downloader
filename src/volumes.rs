use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, GetVolumePathNameW,
};
use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOTE, DRIVE_REMOVABLE};

use crate::ui;

/// Largest file a FAT32 volume can hold (4 GiB - 1 byte).
pub const FAT32_MAX_FILE: u64 = 4 * 1_073_741_824 - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A drive other than the system drive; models go in its root.
    Drive,
    /// The drive Windows is on; models go in the user's folder.
    System,
    /// A folder the user chose.
    Folder,
}

/// Somewhere models can be saved: a drive or a chosen folder. Models land
/// in `<path>\models\<model-id>\`.
#[derive(Debug, Clone)]
pub struct Volume {
    /// Drive letter with colon, e.g. "E:"
    pub letter: String,
    /// Volume label, e.g. "MyExternal" (may be empty)
    pub label: String,
    /// File system name, e.g. "NTFS", "exFAT", "FAT32"
    pub filesystem: String,
    pub path: PathBuf,
    /// Available space in bytes (0 if we couldn't determine it)
    pub free_bytes: u64,
    /// Total capacity in bytes
    pub total_bytes: u64,
    pub kind: Kind,
}

impl Volume {
    /// "E: MyExternal", "E:" when the drive has no label, "C: This PC" for
    /// the system drive, or the folder path for a chosen folder.
    pub fn name(&self) -> String {
        match self.kind {
            Kind::System => format!("{} This PC", self.letter),
            Kind::Folder => self.path.display().to_string(),
            Kind::Drive if self.label.is_empty() => self.letter.clone(),
            Kind::Drive => format!("{} {}", self.letter, self.label),
        }
    }

    /// True if `query` is this drive's letter ("E", "e:", "E:\") or its label.
    pub fn matches(&self, query: &str) -> bool {
        if self.kind == Kind::Folder {
            return false;
        }
        let q = query.trim_end_matches(['\\', '/']).trim_end_matches(':');
        self.letter.trim_end_matches(':').eq_ignore_ascii_case(q)
            || (!self.label.is_empty() && self.label.eq_ignore_ascii_case(query))
    }

    pub fn is_fat32(&self) -> bool {
        self.filesystem.eq_ignore_ascii_case("FAT32") || self.filesystem.eq_ignore_ascii_case("FAT")
    }

    pub fn total_gb(&self) -> f64 {
        self.total_bytes as f64 / 1_073_741_824.0
    }
}

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Read label, file system and space for a drive root like "E:\".
fn query_volume(letter: String, root: &str) -> Volume {
    let root_w = wide(root);

    let mut label = [0u16; 261];
    let mut fs_name = [0u16; 261];
    // SAFETY: root_w is a NUL-terminated UTF-16 string that outlives the call;
    // both output buffers are valid for the lengths we pass; the optional
    // serial/length/flags pointers may be null per the Win32 docs.
    let ok = unsafe {
        GetVolumeInformationW(
            root_w.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs_name.as_mut_ptr(),
            fs_name.len() as u32,
        )
    };
    let (label, filesystem) = if ok != 0 {
        (from_wide(&label).trim().to_string(), from_wide(&fs_name))
    } else {
        (String::new(), String::new())
    };

    let mut free = 0u64;
    let mut total = 0u64;
    // SAFETY: root_w is NUL-terminated; free/total are valid u64 out-pointers;
    // the total-free-bytes pointer may be null.
    let ok = unsafe { GetDiskFreeSpaceExW(root_w.as_ptr(), &mut free, &mut total, std::ptr::null_mut()) };
    if ok == 0 {
        (free, total) = (0, 0);
    }

    Volume {
        letter,
        label,
        filesystem,
        path: PathBuf::from(root),
        free_bytes: free,
        total_bytes: total,
        kind: Kind::Drive,
    }
}

/// Use any folder (created if missing) as the place to save models.
pub fn from_folder(path: &Path) -> Result<Volume> {
    let folder = std::path::absolute(path)
        .with_context(|| format!("'{}' isn't a valid folder path.", path.display()))?;
    std::fs::create_dir_all(&folder)
        .with_context(|| format!("Couldn't create the folder {}.", folder.display()))?;

    let folder_w = wide(&folder.to_string_lossy());
    let mut root = [0u16; 261];
    // SAFETY: folder_w is NUL-terminated UTF-16; root is a writable buffer
    // whose length in characters is what we pass.
    let ok = unsafe { GetVolumePathNameW(folder_w.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
    if ok == 0 {
        bail!("Couldn't tell which drive {} is on.", folder.display());
    }
    let root = from_wide(&root);

    let mut vol = query_volume(root.trim_end_matches('\\').to_string(), &root);
    vol.path = folder;
    vol.kind = Kind::Folder;
    Ok(vol)
}

/// Find places to save models: USB and external disks, extra internal disks
/// and mapped network drives first (largest free space first), then the
/// system drive, which saves into the user's folder. CD/DVD drives, RAM disks
/// and empty card readers are skipped.
pub fn detect_volumes() -> Result<Vec<Volume>> {
    let system_drive = std::env::var("SystemDrive")
        .unwrap_or_else(|_| "C:".to_string())
        .to_uppercase();

    // SAFETY: GetLogicalDrives takes no arguments and only returns a bitmask.
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(std::io::Error::last_os_error()).context("Failed to list drive letters");
    }

    let mut volumes = Vec::new();
    let mut system = None;
    for i in 0..26u8 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = format!("{}:", (b'A' + i) as char);
        if letter == system_drive {
            // Standard users can't write to C:\ itself, so use their own folder.
            let root = format!("{letter}\\");
            let mut vol = query_volume(letter, &root);
            vol.kind = Kind::System;
            vol.path = std::env::var_os("USERPROFILE").map_or_else(|| PathBuf::from(root), PathBuf::from);
            system = Some(vol);
            continue;
        }
        let root = format!("{letter}\\");
        let root_w = wide(&root);
        // SAFETY: root_w is a NUL-terminated UTF-16 string that outlives the call.
        let kind = unsafe { GetDriveTypeW(root_w.as_ptr()) };
        if !matches!(kind, DRIVE_REMOVABLE | DRIVE_FIXED | DRIVE_REMOTE) {
            continue;
        }

        let vol = query_volume(letter, &root);
        // An empty card reader or ejected USB slot reports no file system.
        if vol.filesystem.is_empty() {
            continue;
        }
        volumes.push(vol);
    }

    // Largest free space first, then the system drive
    volumes.sort_by_key(|v| std::cmp::Reverse(v.free_bytes));
    volumes.extend(system);
    Ok(volumes)
}

/// One aligned line describing a drive, for tables and menus.
pub fn describe(vol: &Volume) -> String {
    // pad_str measures width without colour codes, unlike {:>14}.
    let free = ui::free_space(vol.free_bytes).to_string();
    format!(
        "{:<22} {:<6} {}  of {:.0} GB",
        vol.name(),
        vol.filesystem,
        console::pad_str(&free, 12, console::Alignment::Right, None),
        vol.total_gb()
    )
}

pub fn print_volumes(volumes: &[Volume]) {
    ui::heading("Drives");
    for vol in volumes {
        println!("  {}", describe(vol));
        if vol.kind == Kind::System {
            ui::hint(format!("saves to {}\\models", vol.path.display()));
        }
    }
    println!();
}

/// Choose a drive when neither `--volume` nor `--dir` was given: use the
/// only one, or ask.
pub fn pick_volume(volumes: &[Volume]) -> Result<&Volume> {
    if volumes.is_empty() {
        bail!("No drives found.\nPass a folder with --dir, e.g. --dir D:\\Models");
    }

    if volumes.len() == 1 {
        ui::success(format!("Saving to the only drive found: {}", volumes[0].name()));
        return Ok(&volumes[0]);
    }

    if !ui::is_interactive() {
        bail!(
            "Several drives are connected.\nChoose one with --volume (e.g. --volume {}) or a folder with --dir.",
            volumes[0].letter
        );
    }

    let rows: Vec<String> = volumes.iter().map(describe).collect();
    let choice = dialoguer::Select::with_theme(&ui::theme())
        .with_prompt("Which drive should the model go on?")
        .items(&rows)
        .default(0)
        .interact_opt()
        .context("Couldn't show the drive picker")?;

    match choice {
        Some(i) => Ok(&volumes[i]),
        None => bail!("No drive chosen."),
    }
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
    fn matches_letter_or_label() {
        let v = vol("E:", "MyExternal", "exFAT");
        for q in ["E", "e", "E:", "e:\\", "E:/", "MyExternal", "myexternal"] {
            assert!(v.matches(q), "{q} should match");
        }
        assert!(!v.matches("F:"));
        assert!(!vol("E:", "", "NTFS").matches(""));
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
        assert_eq!(Some(last.path.as_os_str()), std::env::var_os("USERPROFILE").as_deref());
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
        assert!(!vol.matches(&vol.letter), "folders aren't picked by drive letter");
        std::fs::remove_dir(&dir).unwrap();
    }
}
