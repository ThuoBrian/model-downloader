//! RAM and graphics on macOS, read with sysctl.

use std::ffi::CStr;

use super::Gpu;

/// (total, free) physical memory in bytes. "Free" is the share macOS
/// reports as available (`kern.memorystatus_level`), the same figure behind
/// Activity Monitor's memory pressure graph; it counts memory that cached
/// files are holding but can be reclaimed.
pub(super) fn ram() -> (u64, u64) {
    let total = sysctl_int(c"hw.memsize").unwrap_or(0);
    let free_percent = sysctl_int(c"kern.memorystatus_level").unwrap_or(0).min(100);
    (total, total / 100 * free_percent)
}

/// Apple Silicon's graphics share system memory, so they count as 0 GB of
/// dedicated memory, like integrated graphics on a PC. The chip name stands
/// in for the GPU name ("Apple M2 Pro"). Intel Macs report no GPU.
pub(super) fn detect_gpu() -> Option<Gpu> {
    if sysctl_int(c"hw.optional.arm64") != Some(1) {
        return None;
    }
    let name = sysctl_string(c"machdep.cpu.brand_string")?;
    Some(Gpu { name, vram_bytes: 0 })
}

/// A 32- or 64-bit integer sysctl.
fn sysctl_int(name: &CStr) -> Option<u64> {
    let mut buf = [0u8; 8];
    let mut size = buf.len();
    // SAFETY: name is NUL-terminated; buf is writable and size holds its
    // length, so the kernel can't write past it. No new value is set.
    let err = unsafe {
        libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0)
    };
    match (err, size) {
        (0, 4) => Some(u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]).into()),
        (0, 8) => Some(u64::from_ne_bytes(buf)),
        _ => None,
    }
}

fn sysctl_string(name: &CStr) -> Option<String> {
    let mut buf = [0u8; 256];
    let mut size = buf.len();
    // SAFETY: as in sysctl_int.
    let err = unsafe {
        libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0)
    };
    if err != 0 {
        return None;
    }
    let s = CStr::from_bytes_until_nul(&buf[..size]).ok()?.to_str().ok()?.trim();
    (!s.is_empty()).then(|| s.to_string())
}
