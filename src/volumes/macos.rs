//! Drives on macOS: everything mounted under /Volumes, read with statfs.

use anyhow::{Context, Result};
use std::ffi::{CString, c_char};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::{Kind, Volume};

/// File system details for the volume holding `path`.
fn statfs(path: &Path) -> Option<libc::statfs> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st = MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: path is NUL-terminated and outlives the call; st is a writable
    // statfs for the call to fill in.
    let ok = unsafe { libc::statfs(path.as_ptr(), st.as_mut_ptr()) } == 0;
    // SAFETY: statfs returned 0, so it filled in st.
    ok.then(|| unsafe { st.assume_init() })
}

fn c_string(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// "apfs" → "APFS", short enough for the drive table.
fn fs_name(fstype: &str) -> String {
    match fstype {
        "apfs" => "APFS",
        "hfs" => "HFS+",
        "exfat" => "exFAT",
        // FAT12/16/32 all show up as msdos, and all cap files at 4 GB.
        "msdos" => "FAT32",
        "ntfs" => "NTFS",
        "smbfs" => "SMB",
        "afpfs" => "AFP",
        "nfs" => "NFS",
        other => other,
    }
    .to_string()
}

fn volume(st: &libc::statfs, label: String, path: PathBuf, kind: Kind) -> Volume {
    let block = u64::from(st.f_bsize);
    Volume {
        letter: String::new(),
        label,
        filesystem: fs_name(&c_string(&st.f_fstypename)),
        path,
        free_bytes: st.f_bavail * block,
        total_bytes: st.f_blocks * block,
        kind,
    }
}

/// The drive an existing folder is on.
pub(super) fn folder_volume(folder: &Path) -> Result<Volume> {
    let st = statfs(folder).with_context(|| format!("Couldn't tell which drive {} is on.", folder.display()))?;
    let mount = PathBuf::from(c_string(&st.f_mntonname));
    let label = mount.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(volume(&st, label, folder.to_path_buf(), Kind::Folder))
}

/// External, extra internal and network drives under /Volumes, then the
/// startup disk, which saves into the home folder. Read-only volumes are
/// skipped: that covers NTFS drives (macOS can only read them), disk images
/// and Time Machine snapshots.
pub(super) fn detect_volumes() -> Result<(Vec<Volume>, Option<Volume>)> {
    let home = std::env::var_os("HOME").map(PathBuf::from).context("HOME isn't set")?;
    let system = statfs(&home).map(|st| volume(&st, String::new(), home, Kind::System));

    let mut volumes = Vec::new();
    for entry in std::fs::read_dir("/Volumes").context("Failed to list /Volumes")?.flatten() {
        let path = entry.path();
        let Some(st) = statfs(&path) else { continue };
        // "Macintosh HD" is a link to /, and stray folders aren't mount
        // points; either way statfs reports a different mount.
        if Path::new(&c_string(&st.f_mntonname)) != path {
            continue;
        }
        if st.f_flags & (libc::MNT_RDONLY | libc::MNT_DONTBROWSE) as u32 != 0 {
            continue;
        }
        let label = entry.file_name().to_string_lossy().into_owned();
        volumes.push(volume(&st, label, path, Kind::Drive));
    }
    Ok((volumes, system))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_file_systems_like_disk_utility() {
        assert_eq!(fs_name("apfs"), "APFS");
        assert_eq!(fs_name("msdos"), "FAT32");
        assert_eq!(fs_name("exfat"), "exFAT");
        assert_eq!(fs_name("zfs"), "zfs");
    }
}
