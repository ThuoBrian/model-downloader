//! Arrow-key menus shown when the program runs with no arguments
//! (including when the .exe is double-clicked).

use anyhow::{Result, bail};
use console::style;
use dialoguer::Select;
use futures_util::future::join_all;

use crate::catalog::{self, ModelEntry};
use crate::downloader::{self, Source};
use crate::hardware::{self, Fit, Hardware};
use crate::volumes::{self, Volume};
use crate::{DownloadRequest, ui};

pub async fn run() -> Result<()> {
    let hw = hardware::detect();

    println!();
    println!("  {}", style("Model Downloader").cyan().bold());
    println!(
        "  {}",
        style("Open-weights reasoning models for llama.cpp, LM Studio and Ollama").dim()
    );
    println!("  {}", style(format!("Your PC: {}", hw.summary())).dim());
    println!(
        "  {}",
        style("Arrow keys to move, Enter to choose, Esc to go back.").dim()
    );

    loop {
        println!();
        let choice = Select::with_theme(&ui::theme())
            .with_prompt("What would you like to do?")
            .items(&[
                "Download a model",
                "Browse models",
                "Most downloaded models",
                "Abliterated models (uncensored)",
                "Show drives",
                "Quit",
            ])
            .default(0)
            .interact_opt()?;

        match choice {
            Some(0) => {
                if let Some(model) = pick_model("Which model? (the best fit for your PC is selected)", &hw)? {
                    download(model, &hw).await?;
                }
            }
            Some(1) => browse(&hw)?,
            Some(2) => {
                if let Err(err) = most_downloaded(&hw).await {
                    ui::print_error(&err);
                }
            }
            Some(3) => abliterated_menu(&hw).await?,
            Some(4) => crate::cmd_volumes()?,
            _ => return Ok(()),
        }
    }
}

/// "Qwen3-14B                       14B   ~9.4 GB   ⚠ tight fit   Dedicated reasoning"
fn model_row(m: &ModelEntry, hw: &Hardware) -> String {
    // Names of models found on Hugging Face can be long; keep the columns lined up.
    let name: String = if m.display_name.chars().count() > 30 {
        m.display_name.chars().take(29).chain(['…']).collect()
    } else {
        m.display_name.to_string()
    };
    let params = if m.params_b > 0.0 {
        format!("{:.0}B", m.params_b)
    } else {
        "?".to_string()
    };
    format!(
        "{:<30} {:>5} {:>9}   {}  {}",
        name,
        params,
        format!("~{:.1} GB", m.size_gb_q4),
        hw.fit(m.estimated_bytes(catalog::DEFAULT_QUANT)).styled(),
        m.reasoning_strength.label()
    )
}

/// The largest model that runs well here, or the smallest if none do.
fn best_fit(models: &[&ModelEntry], hw: &Hardware) -> usize {
    models
        .iter()
        .rposition(|m| matches!(hw.fit(m.estimated_bytes(catalog::DEFAULT_QUANT)), Fit::Fast | Fit::Good))
        .unwrap_or(0)
}

fn pick_model(prompt: &str, hw: &Hardware) -> Result<Option<&'static ModelEntry>> {
    let models = catalog::by_size();
    let rows: Vec<String> = models.iter().map(|m| model_row(m, hw)).collect();
    let choice = Select::with_theme(&ui::theme())
        .with_prompt(prompt)
        .items(&rows)
        .default(best_fit(&models, hw))
        .max_length(14)
        .interact_opt()?;
    Ok(choice.map(|i| models[i]))
}

fn browse(hw: &Hardware) -> Result<()> {
    while let Some(model) = pick_model("Which model? (Esc to go back)", hw)? {
        crate::print_info(model, false);
    }
    Ok(())
}

/// Catalog models ranked by Hugging Face downloads over the last 30 days.
/// Popular models are often bigger than this PC can run, so picking one
/// that won't fit well shows a warning before its details.
async fn most_downloaded(hw: &Hardware) -> Result<()> {
    let client = &crate::http_client()?;
    let spin = ui::spinner("Getting download counts from Hugging Face…");
    let counts = join_all(catalog::catalog().iter().map(|m| async move {
        let repo = m.hf_repo?;
        downloader::hf_downloads(client, repo).await.ok().map(|n| (m, n))
    }))
    .await;
    spin.finish_and_clear();

    let mut ranked: Vec<(&ModelEntry, u64)> = counts.into_iter().flatten().collect();
    if ranked.is_empty() {
        bail!("Couldn't get download counts from Hugging Face. Check your internet connection.");
    }
    ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));

    let rows: Vec<String> = ranked
        .iter()
        .map(|(m, n)| {
            format!(
                "{:<30} {:>8} / month   {}",
                m.display_name,
                ui::format_count(*n),
                hw.fit(m.estimated_bytes(catalog::DEFAULT_QUANT)).styled()
            )
        })
        .collect();

    loop {
        let Some(i) = Select::with_theme(&ui::theme())
            .with_prompt("Most downloaded on Hugging Face (Esc to go back)")
            .items(&rows)
            .default(0)
            .max_length(14)
            .interact_opt()?
        else {
            return Ok(());
        };

        let model = ranked[i].0;
        let fit = hw.fit(model.estimated_bytes(catalog::DEFAULT_QUANT));
        if matches!(fit, Fit::Tight | Fit::TooBig) {
            ui::warn(format!(
                "{} is popular, but it's a {} {}. A smaller model may suit this PC better.",
                model.display_name,
                fit.label(),
                fit.explain(hw)
            ));
        }
        crate::print_info(model, false);
    }
}

/// The curated abliterated models, plus a way to search Hugging Face for more.
async fn abliterated_menu(hw: &Hardware) -> Result<()> {
    println!();
    ui::warn(catalog::ABLITERATED_NOTICE);

    let mut models: Vec<&ModelEntry> = catalog::abliterated().iter().collect();
    models.sort_by(|a, b| a.size_gb_q4.total_cmp(&b.size_gb_q4));
    let mut rows: Vec<String> = models.iter().map(|m| model_row(m, hw)).collect();
    rows.push(format!("{} Check for more on Hugging Face…", ui::ARROW));

    loop {
        let choice = Select::with_theme(&ui::theme())
            .with_prompt("Which abliterated model? (Esc to go back)")
            .items(&rows)
            .default(best_fit(&models, hw))
            .interact_opt()?;
        match choice {
            Some(i) if i < models.len() => download(models[i], hw).await?,
            Some(_) => {
                if let Err(err) = more_abliterated(hw).await {
                    ui::print_error(&err);
                }
            }
            None => return Ok(()),
        }
    }
}

/// bartowski's most downloaded abliterated models that aren't in the curated list.
async fn more_abliterated(hw: &Hardware) -> Result<()> {
    let client = crate::http_client()?;
    let spin = ui::spinner("Searching Hugging Face for abliterated models…");
    let found = downloader::hf_abliterated(&client).await;
    spin.finish_and_clear();

    let curated: Vec<&str> = catalog::abliterated().iter().filter_map(|m| m.hf_repo).collect();
    let live: Vec<(&ModelEntry, u64)> = found?
        .into_iter()
        .filter(|l| !curated.contains(&l.repo.as_str()))
        .map(|l| (catalog::live_entry(&l.repo, l.q4_bytes), l.downloads))
        .collect();
    if live.is_empty() {
        bail!("No other abliterated models found on Hugging Face.");
    }

    ui::warn("These aren't checked by model-downloader. Many aren't reasoning models, and quality varies.");
    let rows: Vec<String> = live
        .iter()
        .map(|(m, n)| format!("{}  {:>7} / month", model_row(m, hw), ui::format_count(*n)))
        .collect();

    loop {
        let Some(i) = Select::with_theme(&ui::theme())
            .with_prompt("Most downloaded on Hugging Face (Esc to go back)")
            .items(&rows)
            .default(0)
            .max_length(14)
            .interact_opt()?
        else {
            return Ok(());
        };
        download(live[i].0, hw).await?;
    }
}

/// Ask the remaining questions for `model`, then download it. A failed
/// download is shown and the user stays in the menu rather than exiting.
async fn download(model: &'static ModelEntry, hw: &Hardware) -> Result<()> {
    if let Some(req) = download_wizard(model, hw)?
        && let Err(err) = crate::run_download(req, false).await
    {
        ui::print_error(&err);
    }
    Ok(())
}

/// Walk through source → quant → drive. `None` means the user backed out.
fn download_wizard(model: &'static ModelEntry, hw: &Hardware) -> Result<Option<DownloadRequest>> {
    let Some(source) = pick_source(model)? else {
        return Ok(None);
    };

    let quant = match source {
        Source::HuggingFace => match pick_quant(model, hw)? {
            Some(q) => q,
            None => return Ok(None),
        },
        // Ollama tags carry their own quant; this value is unused.
        Source::Ollama => catalog::DEFAULT_QUANT.to_string(),
    };

    let Some(volume) = pick_drive()? else {
        return Ok(None);
    };

    Ok(Some(DownloadRequest {
        model,
        source,
        quant,
        volume,
    }))
}

fn pick_source(model: &ModelEntry) -> Result<Option<Source>> {
    let mut options = Vec::new();
    if model.hf_repo.is_some() {
        options.push((Source::HuggingFace, "Hugging Face   choose the quant (recommended)"));
    }
    if model.ollama_tag.is_some() {
        options.push((Source::Ollama, "Ollama         fixed quant, same file Ollama uses"));
    }
    if options.len() == 1 {
        return Ok(Some(options[0].0));
    }

    let rows: Vec<&str> = options.iter().map(|(_, row)| *row).collect();
    let choice = Select::with_theme(&ui::theme())
        .with_prompt("Download from")
        .items(&rows)
        .default(0)
        .interact_opt()?;
    Ok(choice.map(|i| options[i].0))
}

/// "Q4_K_M   ~5.2 GB   ✓ runs well   recommended balance of size and quality"
fn pick_quant(model: &ModelEntry, hw: &Hardware) -> Result<Option<String>> {
    let quants = catalog::available_quants();
    let rows: Vec<String> = quants
        .iter()
        .map(|q| {
            let bytes = model.estimated_bytes(q);
            format!(
                "{:<8} {:>9}   {}  {}",
                q,
                format!("~{:.1} GB", bytes as f64 / 1e9),
                hw.fit(bytes).styled(),
                catalog::quant_note(q)
            )
        })
        .collect();
    let default = quants
        .iter()
        .position(|q| *q == catalog::DEFAULT_QUANT)
        .unwrap_or(0);
    let choice = Select::with_theme(&ui::theme())
        .with_prompt("Which quant? (smaller = less disk and RAM, lower quality)")
        .items(&rows)
        .default(default)
        .interact_opt()?;
    Ok(choice.map(|i| quants[i].to_string()))
}

fn pick_drive() -> Result<Option<Volume>> {
    loop {
        let vols = volumes::detect_volumes()?;

        let mut rows: Vec<String> = vols.iter().map(volumes::describe).collect();
        let folder = rows.len();
        rows.push(format!("{} Choose another folder…", ui::ARROW));
        rows.push(format!("{} Rescan drives", ui::RESCAN));
        rows.push(format!("{} Back", ui::BACK));

        let choice = Select::with_theme(&ui::theme())
            .with_prompt("Where should the model go? (\"This PC\" saves to your user folder)")
            .items(&rows)
            .default(0)
            .interact_opt()?;

        let vol = match choice {
            Some(i) if i < folder => vols[i].clone(),
            Some(i) if i == folder => match ask_folder()? {
                Some(vol) => vol,
                None => continue,
            },
            Some(i) if i == folder + 1 => continue,
            _ => return Ok(None),
        };
        if vol.is_fat32() {
            ui::warn(format!(
                "{} is FAT32, which can't hold files over 4 GB. Reformat it as exFAT or NTFS, or pick another drive.",
                vol.name()
            ));
            continue;
        }
        return Ok(Some(vol));
    }
}

/// Ask for a folder path; empty input goes back. Models are saved in a
/// `models` folder inside it.
fn ask_folder() -> Result<Option<Volume>> {
    let path: String = dialoguer::Input::with_theme(&ui::theme())
        .with_prompt("Folder to save in, e.g. D:\\AI (leave empty to go back)")
        .allow_empty(true)
        .interact_text()?;
    let path = path.trim().trim_matches('"');
    if path.is_empty() {
        return Ok(None);
    }
    match volumes::from_folder(std::path::Path::new(path)) {
        Ok(vol) => Ok(Some(vol)),
        Err(err) => {
            ui::print_error(&err);
            Ok(None)
        }
    }
}
