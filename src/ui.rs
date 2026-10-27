//! Shared terminal look and feel: colours, glyphs, spinners, prompts, errors.

use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use console::{Emoji, StyledObject, Term, style};
use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::Notify;

// Each glyph falls back to plain ASCII in terminals that can't draw it
// (the classic conhost window, or output redirected to a file).
pub const OK: Emoji = Emoji("✓", "v");
pub const ERR: Emoji = Emoji("✗", "x");
pub const WARN: Emoji = Emoji("⚠", "!");
pub const ARROW: Emoji = Emoji("→", ">");
pub const RESCAN: Emoji = Emoji("↻", "*");
pub const BACK: Emoji = Emoji("←", "<");
const RULE: Emoji = Emoji("─", "-");

const GIB: f64 = 1_073_741_824.0;

/// True when both stdin and stdout are a real console, so prompts can be shown.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && Term::stdout().is_term()
}

pub fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

pub fn heading(title: impl Display) {
    let title = title.to_string();
    println!("\n  {}", style(&title).bold());
    println!("  {}", style(RULE.to_string().repeat(title.chars().count())).dim());
}

pub fn success(msg: impl Display) {
    println!("{} {}", style(OK).green().bold(), msg);
}

pub fn warn(msg: impl Display) {
    println!("{} {}", style(WARN).yellow().bold(), style(msg).yellow());
}

pub fn hint(msg: impl Display) {
    println!("  {} {}", style(ARROW).dim(), style(msg).dim());
}

/// An aligned "label  value" line, used for summaries and model details.
pub fn kv(label: &str, value: impl Display) {
    println!("  {} {}", style(format!("{label:<11}")).dim(), value);
}

/// "4.7 GB", or "57 MB" below 1 GB so small amounts don't read as "0.0 GB".
pub fn format_size(bytes: u64) -> String {
    let gb = bytes as f64 / GIB;
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / 1_048_576.0)
    }
}

/// Free space coloured by how comfortable it is for multi-GB models.
pub fn free_space(bytes: u64) -> StyledObject<String> {
    let gb = bytes as f64 / GIB;
    let s = style(format!("{gb:.0} GB free"));
    if gb < 20.0 {
        s.red()
    } else if gb < 100.0 {
        s.yellow()
    } else {
        s.green()
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

/// Whether the terminal can draw Unicode glyphs (Windows Terminal can; the
/// classic console window often can't).
fn fancy_glyphs() -> bool {
    Term::stdout().features().wants_emoji()
}

fn tick_strings() -> &'static [&'static str] {
    if fancy_glyphs() {
        &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", " "]
    } else {
        &["|", "/", "-", "\\", " "]
    }
}

/// A spinner for work with no known length, like talking to an API.
pub fn spinner(msg: impl Into<String>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(tick_strings()),
    );
    pb.set_message(msg.into());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// The download bar, e.g.
///   ⠹ part 1/2 [━━━━━━━━━━╸─────────]  52%  30.4 GiB/58.5 GiB  48.2 MiB/s  ETA 9m
/// Like all indicatif bars it only draws in a real console, not when output
/// is redirected to a file.
pub fn download_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "  {spinner:.cyan} {prefix:.bold}[{wide_bar:.cyan/blue}] {percent:>3}%  {bytes}/{total_bytes}  {bytes_per_sec}  ETA {eta}",
    )
    .unwrap()
    .tick_strings(tick_strings())
    .progress_chars(if fancy_glyphs() { "━╸─" } else { "=>-" })
}

/// A bar for local work with a known size, like checking a file.
pub fn task_style() -> ProgressStyle {
    ProgressStyle::with_template("  {spinner:.cyan} {msg} [{wide_bar:.cyan/blue}] {percent:>3}%")
        .unwrap()
        .tick_strings(tick_strings())
        .progress_chars(if fancy_glyphs() { "━╸─" } else { "=>-" })
}

/// Ask a yes/no question. Without a console to ask on, take the default.
/// Esc counts as "no".
pub fn confirm(prompt: &str, default: bool) -> Result<bool> {
    if !is_interactive() {
        return Ok(default);
    }
    Ok(Confirm::with_theme(&theme())
        .with_prompt(prompt)
        .default(default)
        .interact_opt()?
        .unwrap_or(false))
}

/// Print an error and its causes, e.g.
///   ✗ Failed to resolve download from Hugging Face
///     because: No Q5_K_M GGUF found in HF repo …
///              Quants in this repo: …
pub fn print_error(err: &anyhow::Error) {
    let mut chain = err.chain();
    if let Some(top) = chain.next() {
        let text = top.to_string();
        let mut lines = text.lines();
        eprintln!(
            "\n{} {}",
            style(ERR).red().bold(),
            style(lines.next().unwrap_or_default()).red().bold()
        );
        for line in lines {
            eprintln!("  {}", style(line).dim());
        }
    }
    for cause in chain {
        let text = cause.to_string();
        let mut lines = text.lines();
        eprintln!("  {} {}", style("because:").dim(), lines.next().unwrap_or_default());
        for line in lines {
            eprintln!("           {}", line);
        }
    }
}

/// True when Windows opened a console just for this program, which happens
/// when the .exe is double-clicked rather than run from a terminal.
pub fn launched_by_double_click() -> bool {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;
    let mut pids = [0u32; 2];
    // SAFETY: pids is a valid, writable buffer of the length we pass; the call
    // only writes up to that many process IDs and returns the total count.
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) };
    count == 1
}

/// Keep a double-clicked window open so its last message can be read.
pub fn pause_before_exit() {
    if launched_by_double_click() {
        println!("\n{}", style("Press Enter to close this window.").dim());
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}

// ── Ctrl+C ──────────────────────────────────────────────────────────────────
//
// Once a Ctrl+C handler is installed it replaces Windows' default "exit now",
// so a single handler decides: during a download, signal it to pause cleanly;
// anywhere else, restore the cursor (menus hide it) and exit.

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static INTERRUPT: Notify = Notify::const_new();

pub fn install_ctrl_c_handler() {
    tokio::spawn(async {
        while tokio::signal::ctrl_c().await.is_ok() {
            if DOWNLOADING.load(Ordering::SeqCst) {
                INTERRUPT.notify_one();
            } else {
                let _ = Term::stdout().show_cursor();
                println!();
                std::process::exit(130);
            }
        }
    });
}

/// Marks a download as running so Ctrl+C pauses it instead of exiting.
pub struct DownloadGuard;

impl DownloadGuard {
    pub fn new() -> Self {
        DOWNLOADING.store(true, Ordering::SeqCst);
        DownloadGuard
    }
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        DOWNLOADING.store(false, Ordering::SeqCst);
    }
}

/// Resolves when the user presses Ctrl+C during a download.
pub async fn interrupted() {
    INTERRUPT.notified().await;
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A fake 100-column terminal that records the last line drawn.
    #[derive(Debug, Default, Clone)]
    struct Capture(std::sync::Arc<std::sync::Mutex<String>>);

    impl indicatif::TermLike for Capture {
        fn width(&self) -> u16 {
            100
        }
        fn move_cursor_up(&self, _: usize) -> std::io::Result<()> {
            Ok(())
        }
        fn move_cursor_down(&self, _: usize) -> std::io::Result<()> {
            Ok(())
        }
        fn move_cursor_right(&self, _: usize) -> std::io::Result<()> {
            Ok(())
        }
        fn move_cursor_left(&self, _: usize) -> std::io::Result<()> {
            Ok(())
        }
        fn write_line(&self, s: &str) -> std::io::Result<()> {
            self.write_str(s)
        }
        fn write_str(&self, s: &str) -> std::io::Result<()> {
            if !s.trim().is_empty() {
                *self.0.lock().unwrap() = console::strip_ansi_codes(s).into_owned();
            }
            Ok(())
        }
        fn clear_line(&self) -> std::io::Result<()> {
            Ok(())
        }
        fn flush(&self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn download_bar_shows_percent_and_sizes() {
        let term = Capture::default();
        let pb = ProgressBar::with_draw_target(
            Some(10 * 1_073_741_824),
            indicatif::ProgressDrawTarget::term_like(Box::new(term.clone())),
        )
        .with_style(download_style())
        .with_prefix("part 1/2 ");
        pb.set_position(5 * 1_073_741_824);
        pb.tick();

        let line = term.0.lock().unwrap().clone();
        println!("{line}");
        assert!(line.contains("part 1/2 ["), "{line}");
        assert!(line.contains(" 50%"), "{line}");
        assert!(line.contains("5.00 GiB/10.00 GiB"), "{line}");
        assert!(line.contains("ETA"), "{line}");
    }
}
