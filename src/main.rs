#[cfg(not(windows))]
compile_error!("model-downloader is a Windows tool; build it on Windows (x86_64-pc-windows-msvc).");

mod catalog;
mod downloader;
mod hardware;
mod interactive;
mod ui;
mod volumes;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use catalog::{ModelEntry, ReasoningStrength};
use clap::{CommandFactory, Parser, Subcommand};
use console::{StyledObject, style};
use downloader::Source;
use volumes::Volume;

#[derive(Parser)]
#[command(
    name = "model-downloader",
    about = "Download open-weights reasoning models (GGUF) to any drive or folder",
    long_about = "Download open-weights reasoning models (GGUF) to any drive or folder.\n\n\
                  Run with no arguments (or double-click the .exe) for an interactive menu.",
    version,
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List curated reasoning models
    List {
        /// Filter by reasoning strength
        #[arg(short, long)]
        filter: Option<ReasoningStrength>,
    },
    /// Show details for a specific model
    Info {
        /// Model ID (e.g. dsr1-distill-qwen-14b)
        model_id: String,
    },
    /// List the drives models can be saved to
    Volumes,
    /// Download a model
    Download {
        /// Model ID (e.g. dsr1-distill-qwen-14b)
        model_id: String,
        /// Download source
        #[arg(short, long, value_enum, default_value = "hf")]
        source: Source,
        /// Quantization level (Q4_K_M, Q3_K_M, Q5_K_M, Q6_K, Q8_0). Hugging Face only;
        /// Ollama tags come in a fixed quant. [default: Q4_K_M]
        #[arg(short, long)]
        quant: Option<String>,
        /// Drive to download to, by letter (E:) or label (skips the picker)
        #[arg(short, long)]
        volume: Option<String>,
        /// Folder to download to instead of a drive (created if missing)
        #[arg(short, long, conflicts_with = "volume")]
        dir: Option<PathBuf>,
        /// Start without asking for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    ui::install_ctrl_c_handler();

    let result = match cli.command {
        None if ui::is_interactive() => interactive::run().await.map(|()| ExitCode::SUCCESS),
        None => {
            let _ = Cli::command().print_help();
            Ok(ExitCode::SUCCESS)
        }
        Some(Commands::List { filter }) => {
            cmd_list(filter);
            Ok(ExitCode::SUCCESS)
        }
        Some(Commands::Info { model_id }) => catalog::lookup(&model_id).map(|m| {
            print_info(m, true);
            ExitCode::SUCCESS
        }),
        Some(Commands::Volumes) => cmd_volumes().map(|()| ExitCode::SUCCESS),
        Some(Commands::Download {
            model_id,
            source,
            quant,
            volume,
            dir,
            yes,
        }) => cmd_download(&model_id, source, quant.as_deref(), volume, dir, yes).await,
    };

    match result {
        Ok(code) => code,
        Err(err) => {
            ui::print_error(&err);
            ui::pause_before_exit();
            ExitCode::FAILURE
        }
    }
}

// ── list / info / volumes ───────────────────────────────────────────────────

fn styled_strength(s: ReasoningStrength, width: usize) -> StyledObject<String> {
    let text = style(format!("{:<width$}", s.label()));
    match s {
        ReasoningStrength::Dedicated => text.green(),
        ReasoningStrength::Strong => text.yellow(),
        ReasoningStrength::Moderate => text.dim(),
    }
}

fn cmd_list(filter: Option<ReasoningStrength>) {
    let filtered: Vec<&ModelEntry> = catalog::by_size()
        .into_iter()
        .filter(|m| filter.is_none_or(|f| m.reasoning_strength == f))
        .collect();

    if filtered.is_empty() {
        ui::warn("No models match the filter.");
        return;
    }

    let hw = hardware::detect();
    ui::heading("Curated reasoning models (GGUF)");
    ui::kv("Your PC", hw.summary());
    println!();
    println!(
        "  {}",
        style(format!(
            "{:<24} {:<20} {:>6} {:>9}  On your PC",
            "ID", "Strength", "Params", "Q4 size"
        ))
        .dim()
    );

    for m in &filtered {
        println!(
            "  {} {} {:>5.0}B {}  {}",
            style(format!("{:<24}", m.id)).cyan().bold(),
            styled_strength(m.reasoning_strength, 20),
            m.params_b,
            style(format!("{:>6.1} GB", m.size_gb_q4)).dim(),
            hw.fit(m.estimated_bytes(catalog::DEFAULT_QUANT)).styled()
        );
    }

    println!();
    ui::hint("Details:  model-downloader info <id>");
    ui::hint("Download: model-downloader download <id>   (or run with no arguments for a menu)");
    println!();
}

/// Print a model's details. `show_commands` adds copy-paste CLI examples.
pub(crate) fn print_info(model: &ModelEntry, show_commands: bool) {
    ui::heading(model.display_name);
    ui::kv("ID", style(model.id).cyan().bold());
    ui::kv("Parameters", format!("{:.0}B", model.params_b));
    ui::kv("Reasoning", styled_strength(model.reasoning_strength, 0));
    ui::kv("Q4_K_M size", format!("~{:.1} GB", model.size_gb_q4));
    if let Some(repo) = model.hf_repo {
        ui::kv("HF repo", repo);
    }
    if let Some(tag) = model.ollama_tag {
        ui::kv("Ollama tag", tag);
    }
    ui::kv("Quants", catalog::available_quants().join(", "));

    println!("\n  {}\n", model.description);

    if show_commands {
        ui::hint("Download it:");
        println!(
            "      {}",
            style(format!("model-downloader download {} --quant {}", model.id, catalog::DEFAULT_QUANT)).cyan()
        );
        if model.ollama_tag.is_some() {
            println!("      {}", style(format!("model-downloader download {} --source ollama", model.id)).cyan());
        }
        println!();
    }
}

pub(crate) fn cmd_volumes() -> Result<()> {
    let vols = volumes::detect_volumes()?;
    volumes::print_volumes(&vols);
    ui::hint("To use any other folder: model-downloader download <id> --dir D:\\Models");
    println!();
    if vols.iter().any(|v| v.is_fat32()) {
        ui::warn("FAT32 drives can't hold files over 4 GB, which rules out most models.");
        ui::hint("Reformat them as exFAT or NTFS to use them.");
        println!();
    }

    Ok(())
}

// ── download ────────────────────────────────────────────────────────────────

/// Everything needed to download one model, however it was chosen.
pub(crate) struct DownloadRequest {
    pub model: &'static ModelEntry,
    pub source: Source,
    pub quant: String,
    pub volume: Volume,
}

pub(crate) enum Outcome {
    Done,
    Cancelled,
    Paused,
}

async fn cmd_download(
    model_id: &str,
    source: Source,
    quant: Option<&str>,
    volume_name: Option<String>,
    dir: Option<PathBuf>,
    assume_yes: bool,
) -> Result<ExitCode> {
    let model = catalog::lookup(model_id)?;

    // Validate quant
    let quant = quant.map(str::to_uppercase);
    if source == Source::Ollama && quant.is_some() {
        ui::warn(format!(
            "--quant is ignored for Ollama; the tag {} has a fixed quant.",
            model.ollama_tag.unwrap_or("?")
        ));
    }
    let quant = quant.unwrap_or_else(|| catalog::DEFAULT_QUANT.to_string());
    if !catalog::available_quants().contains(&quant.as_str()) {
        bail!(
            "Invalid quant '{}'.\nChoose one of: {}",
            quant,
            catalog::available_quants().join(", ")
        );
    }

    // Check source availability for this model
    let available = match source {
        Source::HuggingFace => model.hf_repo.is_some(),
        Source::Ollama => model.ollama_tag.is_some(),
    };
    if !available {
        bail!("{} is not available on {}.", model.display_name, source.label());
    }

    let volume = match (dir, volume_name) {
        (Some(dir), _) => volumes::from_folder(&dir)?,
        (None, name) => {
            let vols = volumes::detect_volumes()?;
            match name {
                Some(name) => vols.iter().find(|v| v.matches(&name)).cloned().with_context(|| {
                    format!("Drive '{name}' not found.\nRun 'model-downloader volumes' to see connected drives.")
                })?,
                None => volumes::pick_volume(&vols)?.clone(),
            }
        }
    };

    let req = DownloadRequest {
        model,
        source,
        quant,
        volume,
    };
    Ok(match run_download(req, assume_yes).await? {
        Outcome::Done | Outcome::Cancelled => ExitCode::SUCCESS,
        Outcome::Paused => ExitCode::from(130),
    })
}

pub(crate) fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("model-downloader/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// Resolve, summarise, confirm, then download — shared by the CLI and the menu.
pub(crate) async fn run_download(req: DownloadRequest, assume_yes: bool) -> Result<Outcome> {
    let DownloadRequest {
        model,
        source,
        quant,
        volume,
    } = req;

    let client = http_client()?;

    println!();
    let spin = ui::spinner(format!("Contacting {}…", source.label()));
    let resolved = downloader::resolve_download(&client, model, source, &quant).await;
    spin.finish_and_clear();
    let files = resolved.with_context(|| format!("Couldn't get {} from {}", model.display_name, source.label()))?;

    let total: Option<u64> = files.iter().map(|f| f.size).sum();
    let count = match files.len() {
        1 => "1 file".to_string(),
        n => format!("{n} files"),
    };
    ui::success(match total {
        Some(t) => format!("Found {count} ({})", ui::format_size(t)),
        None => format!("Found {count}"),
    });

    let dests: Vec<_> = files
        .iter()
        .map(|f| downloader::model_dest_path(&volume.path, model, &f.filename))
        .collect();
    let have: u64 = dests.iter().map(|d| downloader::existing_bytes(d)).sum();

    ui::heading("Download summary");
    ui::kv("Model", style(model.display_name).bold());
    ui::kv(
        "Source",
        match source {
            Source::HuggingFace => format!("{} · {quant}", source.label()),
            Source::Ollama => format!("{} · {}", source.label(), model.ollama_tag.unwrap_or("?")),
        },
    );
    ui::kv(
        if volume.kind == volumes::Kind::Folder { "Folder" } else { "Drive" },
        format!("{} ({}) · {}", volume.name(), volume.filesystem, ui::free_space(volume.free_bytes)),
    );
    if let Some(folder) = dests[0].parent() {
        ui::kv("Saving to", folder.display());
    }
    for (i, f) in files.iter().enumerate() {
        ui::kv(if i == 0 { "Files" } else { "" }, &f.filename);
    }
    if let Some(t) = total {
        let mut size = ui::format_size(t);
        if have > 0 {
            size += &style(format!("  ({} already downloaded)", ui::format_size(have))).dim().to_string();
        }
        ui::kv("Size", size);
    }
    let hw = hardware::detect();
    let fit = total.map(|t| hw.fit(t));
    if let Some(fit) = fit {
        ui::kv("Your PC", format!("{} {}", fit.styled(), style(fit.explain(&hw)).dim()));
    }
    println!();

    // FAT32 caps files at 4 GB; the write would fail partway through.
    if volume.is_fat32() && files.iter().any(|f| f.size.is_none_or(|s| s > volumes::FAT32_MAX_FILE)) {
        bail!(
            "{} is formatted as FAT32, which can't hold files over 4 GB.\n\
             Reformat it as exFAT or NTFS, or choose another drive.",
            volume.name()
        );
    }

    // Check free space, counting what's already on disk from an earlier run
    let remaining = total.map(|t| t.saturating_sub(have));
    if let Some(needed) = remaining
        && needed > volume.free_bytes
        && volume.free_bytes > 0
    {
        bail!(
            "Not enough free space on {}: need {}, have {}.\n\
             Free up space or choose another drive.",
            volume.name(),
            ui::format_size(needed),
            ui::format_size(volume.free_bytes)
        );
    }

    let nothing_left = remaining == Some(0);
    if !assume_yes && !nothing_left {
        let prompt = match remaining {
            Some(n) => format!("Download {} to {}?", ui::format_size(n), volume.name()),
            None => format!("Start the download to {}?", volume.name()),
        };
        // Default to "no" for a model this PC can't run, so Enter doesn't
        // start a download that will be wasted.
        let too_big = fit == Some(hardware::Fit::TooBig);
        if too_big {
            ui::warn("This model is too big for this PC. A smaller model or quant will work better.");
        }
        if !ui::confirm(&prompt, !too_big)? {
            ui::hint("Cancelled. Nothing was downloaded.");
            return Ok(Outcome::Cancelled);
        }
    }

    let started = Instant::now();
    let _guard = ui::DownloadGuard::new();
    for (i, (file, dest)) in files.iter().zip(&dests).enumerate() {
        downloader::ensure_parent_dir(dest).await?;

        let label = if files.len() > 1 {
            format!("part {}/{} ", i + 1, files.len())
        } else {
            String::new()
        };
        tokio::select! {
            result = downloader::download_with_progress(&client, file, dest, &label) => {
                result.context("Download failed")?;
            }
            () = ui::interrupted() => {
                let saved: u64 = dests.iter().map(|d| downloader::existing_bytes(d)).sum();
                println!();
                ui::warn(match total {
                    Some(t) => format!("Paused: {} of {} saved.", ui::format_size(saved), ui::format_size(t)),
                    None => format!("Paused: {} saved.", ui::format_size(saved)),
                });
                ui::hint("Run the same download again to pick up where it stopped.");
                return Ok(Outcome::Paused);
            }
        }
    }

    println!();
    ui::success(style(format!("Done in {}", ui::format_duration(started.elapsed()))).green().bold());
    ui::kv("Saved to", dests[0].display());
    if dests.len() > 1 {
        ui::hint(format!(
            "Split into {} parts. Point your app at the first one and keep the parts together.",
            dests.len()
        ));
    }
    ui::hint("Run it with llama.cpp:");
    println!("      {}", style(format!("llama-cli -m \"{}\"", dests[0].display())).cyan());
    ui::hint("Or open the folder in LM Studio, or import it into Ollama with a Modelfile.");
    println!();

    Ok(Outcome::Done)
}
