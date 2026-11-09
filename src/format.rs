//! Turning bytes, counts and durations into short, readable text.

use std::time::Duration;

const GIB: f64 = 1_073_741_824.0;

/// What to call the user's computer: "Your PC", "This Mac".
#[cfg(windows)]
pub const COMPUTER: &str = "PC";
#[cfg(target_os = "macos")]
pub const COMPUTER: &str = "Mac";

/// "4.7 GB", or "57 MB" below 1 GB so small amounts don't read as "0.0 GB".
pub fn format_size(bytes: u64) -> String {
    let gb = bytes as f64 / GIB;
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / 1_048_576.0)
    }
}

/// "950", "127.7k", "1.2M".
pub fn format_count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    match (secs / 3600, secs / 60 % 60, secs % 60) {
        (0, 0, s) => format!("{s}s"),
        (0, m, s) => format!("{m}m {s:02}s"),
        (h, m, _) => format!("{h}h {m:02}m"),
    }
}

/// Free space in whole GB, e.g. "212 GB free".
pub fn free_space(bytes: u64) -> String {
    format!("{:.0} GB free", bytes as f64 / GIB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_count() {
        assert_eq!(format_count(950), "950");
        assert_eq!(format_count(127_725), "127.7k");
        assert_eq!(format_count(1_234_567), "1.2M");
    }

    #[test]
    fn formats_size() {
        assert_eq!(format_size(0), "0 MB");
        assert_eq!(format_size(59_900_405), "57 MB");
        assert_eq!(format_size(5_027_784_224), "4.7 GB");
    }

    #[test]
    fn formats_duration() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration(Duration::from_secs(723)), "12m 03s");
        assert_eq!(format_duration(Duration::from_secs(3720)), "1h 02m");
    }
}
