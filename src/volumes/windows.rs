//! Drives on Windows: every drive letter, read with the Win32 API.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, GetVolumePathNameW,
};
use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOTE, DRIVE_REMOVABLE};

use super::{Kind, Volume};

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

/// The drive an existing folder is on.
pub(super) fn folder_volume(folder: &Path) -> Result<Volume> {
    let folder_w = wide(&folder.to_string_lossy());
    let mut root = [0u16; 261];
    // SAFETY: folder_w is NUL-terminated UTF-16; root is a writable buffer
    // whose length in characters is what we pass.
    let ok = unsafe { GetVolumePathNameW(folder_w.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
    if ok == 0 {
        bail!("Couldn't tell which drive {} is on.", folder.display());
    }
    let root = from_wide(&root);
    Ok(query_volume(root.trim_end_matches('\\').to_string(), &root))
}

/// USB and external disks, extra internal disks and mapped network drives,
/// then the system drive, which saves into the user's folder. CD/DVD drives,
/// RAM disks and empty card readers are skipped.
pub(super) fn detect_volumes() -> Result<(Vec<Volume>, Option<Volume>)> {
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
    Ok((volumes, system))
}
