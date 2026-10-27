//! What this PC can run: total RAM and dedicated graphics memory, and a rough
//! rating of whether a model of a given size will run on it.

use console::{StyledObject, style};
use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_QWORD, RRF_RT_REG_SZ, RegGetValueW};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use crate::ui;
use crate::volumes::{from_wide, wide};

const GIB: f64 = 1_073_741_824.0;

/// Display adapters ("Display" device class) in the registry.
const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

#[derive(Debug, Clone)]
pub struct Gpu {
    pub name: String,
    /// Dedicated video memory; 0 for integrated graphics that share RAM.
    pub vram_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Hardware {
    pub ram_bytes: u64,
    /// RAM not in use by other apps when the program started.
    pub free_ram_bytes: u64,
    pub gpu: Option<Gpu>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Fits in dedicated graphics memory.
    Fast,
    /// Fits comfortably in RAM (plus any graphics memory).
    Good,
    /// Will load, but leaves little memory for anything else.
    Tight,
    /// Needs more memory than this PC has.
    TooBig,
}

impl Fit {
    pub fn label(self) -> &'static str {
        match self {
            Fit::Fast => "runs fast",
            Fit::Good => "runs well",
            Fit::Tight => "tight fit",
            Fit::TooBig => "too big",
        }
    }

    /// e.g. "✓ runs well", padded to a fixed width before colouring.
    pub fn styled(self) -> StyledObject<String> {
        let glyph = match self {
            Fit::Fast | Fit::Good => ui::OK,
            Fit::Tight => ui::WARN,
            Fit::TooBig => ui::ERR,
        };
        let text = style(format!("{} {:<9}", glyph, self.label()));
        match self {
            Fit::Fast | Fit::Good => text.green(),
            Fit::Tight => text.yellow(),
            Fit::TooBig => text.red(),
        }
    }

    pub fn explain(self, hw: &Hardware) -> String {
        let ram = format!("{:.0} GB RAM", hw.ram_bytes as f64 / GIB);
        match self {
            Fit::Fast => "fits in your graphics memory".to_string(),
            Fit::Good => format!("with your {ram}"),
            Fit::Tight => format!("on your {ram}: it will run slowly, with little room for other apps"),
            Fit::TooBig => format!("for your {ram}: it will download but probably won't run"),
        }
    }
}

impl Hardware {
    /// Rated against total memory, since other apps can be closed.
    /// Windows needs roughly a quarter of RAM.
    pub fn fit(&self, model_bytes: u64) -> Fit {
        let need = memory_needed(model_bytes);
        let ram = self.ram_bytes as f64;
        let vram = self.vram();
        if vram > 0.0 && need <= vram {
            Fit::Fast
        } else if need <= ram * 0.75 + vram {
            Fit::Good
        } else if need <= ram + vram {
            Fit::Tight
        } else {
            Fit::TooBig
        }
    }

    /// True when the model would fit on this PC, but not in the memory that
    /// other apps have left free right now.
    pub fn short_on_free_memory(&self, model_bytes: u64) -> bool {
        let need = memory_needed(model_bytes);
        need <= self.ram_bytes as f64 + self.vram() && need > self.free_ram_bytes as f64 + self.vram()
    }

    fn vram(&self) -> f64 {
        self.gpu.as_ref().map_or(0, |g| g.vram_bytes) as f64
    }

    /// "16 GB RAM (6 GB free) · NVIDIA GeForce RTX 3060 (12 GB)"
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{:.0} GB RAM ({:.0} GB free)",
            self.ram_bytes as f64 / GIB,
            self.free_ram_bytes as f64 / GIB
        );
        if let Some(gpu) = &self.gpu {
            if gpu.vram_bytes > 0 {
                s += &format!(" · {} ({:.0} GB)", gpu.name, gpu.vram_bytes as f64 / GIB);
            } else {
                s += &format!(" · {} (shares system memory)", gpu.name);
            }
        }
        s
    }
}

/// Rough rule of thumb: a model needs about its file size plus 20% and
/// 1 GB for context and the runtime.
fn memory_needed(model_bytes: u64) -> f64 {
    model_bytes as f64 * 1.2 + GIB
}

pub fn detect() -> Hardware {
    let (ram_bytes, free_ram_bytes) = ram();
    Hardware {
        ram_bytes,
        free_ram_bytes,
        gpu: detect_gpu(),
    }
}

/// (total, free) physical memory in bytes.
fn ram() -> (u64, u64) {
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
fn detect_gpu() -> Option<Gpu> {
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

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_073_741_824;

    fn pc(ram_gb: u64, vram_gb: u64) -> Hardware {
        Hardware {
            ram_bytes: ram_gb * GB,
            free_ram_bytes: ram_gb * GB,
            gpu: Some(Gpu {
                name: "Test GPU".into(),
                vram_bytes: vram_gb * GB,
            }),
        }
    }

    #[test]
    fn rates_models_for_a_16gb_laptop() {
        let hw = pc(16, 0);
        assert_eq!(hw.fit(5 * GB), Fit::Good); // 8B model
        assert_eq!(hw.fit(10 * GB), Fit::Tight); // 14B model
        assert_eq!(hw.fit(20 * GB), Fit::TooBig); // 32B model
    }

    #[test]
    fn graphics_memory_makes_it_fast() {
        let hw = pc(32, 12);
        assert_eq!(hw.fit(5 * GB), Fit::Fast);
        assert_eq!(hw.fit(20 * GB), Fit::Good);
    }

    #[test]
    fn warns_when_other_apps_use_the_memory() {
        let hw = Hardware {
            free_ram_bytes: 5 * GB,
            ..pc(16, 0)
        };
        assert!(!hw.short_on_free_memory(3 * GB)); // fits in what's free
        assert!(hw.short_on_free_memory(5 * GB)); // fits once apps are closed
        assert!(!hw.short_on_free_memory(20 * GB)); // too big either way; fit() covers it
    }

    #[test]
    fn detects_some_ram() {
        let hw = detect();
        assert!(hw.ram_bytes > GB, "every test machine has over 1 GB of RAM");
        assert!(hw.free_ram_bytes <= hw.ram_bytes);
    }
}
