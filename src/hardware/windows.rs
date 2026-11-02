//! RAM and graphics on Windows, read from the Win32 API and the registry.

use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_QWORD, RRF_RT_REG_SZ, RegGetValueW};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use super::Gpu;
use crate::volumes::sys::{from_wide, wide};

/// Display adapters ("Display" device class) in the registry.
const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// (total, free) physical memory in bytes.
pub(super) fn ram() -> (u64, u64) {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        dwMemoryLoad: 0,
        ullTotalPhys: 0,
        ullAvailPhys: 0,
        ullTotalPageFile: 0,
        ullAvailPageFile: 0,
        ullTotalVirtual: 0,
        ullAvailVirtual: 0,
        ullAvailExtendedVirtual: 0,
    };
    // SAFETY: status is a valid MEMORYSTATUSEX with dwLength set, as the call requires.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok != 0 { (status.ullTotalPhys, status.ullAvailPhys) } else { (0, 0) }
}

/// The adapter with the most dedicated memory. Discrete cards record it as
/// `HardwareInformation.qwMemorySize`; integrated graphics don't, so they
/// count as 0 (they share system RAM, which `fit` already accounts for).
pub(super) fn detect_gpu() -> Option<Gpu> {
    (0..16)
        .filter_map(|i| {
            let key = format!(r"{DISPLAY_CLASS}\{i:04}");
            let name = reg_string(&key, "DriverDesc")?;
            let vram_bytes = reg_u64(&key, "HardwareInformation.qwMemorySize").unwrap_or(0);
            Some(Gpu { name, vram_bytes })
        })
        .max_by_key(|g| g.vram_bytes)
}

fn reg_u64(subkey: &str, value: &str) -> Option<u64> {
    let (subkey, value) = (wide(subkey), wide(value));
    let mut data = 0u64;
    let mut size = size_of::<u64>() as u32;
    // SAFETY: subkey/value are NUL-terminated UTF-16; data is a writable u64
    // and size holds its byte length, so the call can't write past it.
    let err = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_QWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u64).cast(),
            &mut size,
        )
    };
    (err == 0).then_some(data)
}

fn reg_string(subkey: &str, value: &str) -> Option<String> {
    let (subkey, value) = (wide(subkey), wide(value));
    let mut buf = [0u16; 256];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: as in reg_u64; size is the buffer's length in bytes, and
    // RRF_RT_REG_SZ makes the call NUL-terminate the string within it.
    let err = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    let s = from_wide(&buf);
    (err == 0 && !s.is_empty()).then_some(s)
}
