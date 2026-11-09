//! What this computer can run: total RAM and dedicated graphics memory, and a rough
//! rating of whether a model of a given size will run on it.

#[cfg_attr(windows, path = "hardware/windows.rs")]
#[cfg_attr(target_os = "macos", path = "hardware/macos.rs")]
mod sys;

const GIB: f64 = 1_073_741_824.0;

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

    /// "✔", "⚠" or "✖"; the GUI colours it.
    pub fn glyph(self) -> &'static str {
        match self {
            Fit::Fast | Fit::Good => "✔",
            Fit::Tight => "⚠",
            Fit::TooBig => "✖",
        }
    }

    /// True when the model runs fast or well, not just barely.
    pub fn runs_well(self) -> bool {
        matches!(self, Fit::Fast | Fit::Good)
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
    /// The operating system needs roughly a quarter of RAM.
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
    let (ram_bytes, free_ram_bytes) = sys::ram();
    Hardware {
        ram_bytes,
        free_ram_bytes,
        gpu: sys::detect_gpu(),
    }
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
