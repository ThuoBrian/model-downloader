//! The window: a model list on the left, and on the right whatever step the
//! user is on (choose options, confirm, downloading, done).
//!
//! Network and disk work runs on the tokio runtime. Each frame checks on it
//! and, while anything is running, asks to be redrawn shortly so progress
//! keeps moving.

use std::collections::VecDeque;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, Ui};
use futures_util::FutureExt;
use futures_util::future::join_all;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::task::JoinHandle;

use crate::catalog::{self, ModelEntry};
use crate::download::{self, DownloadPlan, DownloadRequest};
use crate::downloader::{self, Event, Source};
use crate::format::{COMPUTER, format_count, format_duration, format_size, free_space};
use crate::hardware::{self, Fit, Hardware};
use crate::volumes::{self, Kind, Volume};

const GIB: f64 = 1_073_741_824.0;
/// How often to redraw while something runs in the background.
const TICK: Duration = Duration::from_millis(100);

type Ranked = Vec<(&'static ModelEntry, u64)>;

pub struct App {
    rt: Handle,
    client: reqwest::Client,
    hw: Hardware,
    volumes: Vec<Volume>,
    volume_error: Option<String>,
    /// A folder the user picked instead of a drive.
    folder: Option<Volume>,

    tab: Tab,
    popular: Job<Ranked>,
    more_abliterated: Job<Ranked>,

    selected: Option<&'static ModelEntry>,
    source: Source,
    quant: &'static str,
    dest: Option<Volume>,

    stage: Stage,
    /// Asking whether to pause the running download and quit.
    confirm_quit: bool,
    quitting: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    All,
    Popular,
    Abliterated,
}

enum Stage {
    Choose,
    Planning(JoinHandle<anyhow::Result<DownloadPlan>>),
    Confirm(Arc<DownloadPlan>),
    Downloading(Box<Running>),
    Paused {
        plan: Arc<DownloadPlan>,
        saved: u64,
    },
    Done {
        plan: Arc<DownloadPlan>,
        took: Duration,
    },
    Failed {
        error: String,
        plan: Option<Arc<DownloadPlan>>,
    },
}

/// Background work whose result the window shows when it arrives.
enum Job<T> {
    Idle,
    Running(JoinHandle<anyhow::Result<T>>),
    Ready(T),
    Failed(String),
}

impl<T: Send + 'static> Job<T> {
    fn start(rt: &Handle, work: impl Future<Output = anyhow::Result<T>> + Send + 'static) -> Self {
        Job::Running(rt.spawn(work))
    }

    /// Collect the result if the work has finished. True while it's still running.
    fn poll(&mut self) -> bool {
        let Job::Running(handle) = self else {
            return false;
        };
        if !handle.is_finished() {
            return true;
        }
        *self = match handle.now_or_never() {
            Some(Ok(Ok(value))) => Job::Ready(value),
            Some(Ok(Err(err))) => Job::Failed(error_text(&err)),
            _ => Job::Failed("That stopped unexpectedly. Try again.".into()),
        };
        false
    }
}

/// A download in progress, and what it has reported so far.
struct Running {
    plan: Arc<DownloadPlan>,
    handle: JoinHandle<anyhow::Result<()>>,
    events: UnboundedReceiver<Event>,
    started: Instant,
    /// Which of the plan's files is downloading.
    file: usize,
    /// Bytes of that file on disk, and its size.
    done: u64,
    file_total: u64,
    seen_progress: bool,
    /// Re-reading a partial file from an earlier run: (done, total).
    checking: Option<(u64, u64)>,
    notes: Vec<String>,
    /// (time, overall bytes) over the last few seconds, for speed and ETA.
    samples: VecDeque<(Instant, u64)>,
}

impl Running {
    fn drain_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::File { index } => {
                    self.file = index;
                    self.done = 0;
                    self.file_total = 0;
                    self.seen_progress = false;
                    self.samples.clear();
                }
                Event::Checking { done, total } => self.checking = Some((done, total)),
                Event::Progress { done, total } => {
                    if self.checking.take().is_some() {
                        self.samples.clear();
                    }
                    self.done = done;
                    self.file_total = total;
                    self.seen_progress = true;
                }
                Event::Note(note) => self.notes.push(note),
            }
        }
        if self.seen_progress {
            let now = Instant::now();
            self.samples.push_back((now, self.overall()));
            while self.samples.len() > 2
                && self
                    .samples
                    .front()
                    .is_some_and(|(t, _)| now - *t > Duration::from_secs(5))
            {
                self.samples.pop_front();
            }
        }
    }

    /// Bytes of the files before the current one.
    fn base(&self) -> u64 {
        self.plan.files[..self.file]
            .iter()
            .filter_map(|f| f.size)
            .sum()
    }

    fn overall(&self) -> u64 {
        self.base() + self.done
    }

    fn total(&self) -> Option<u64> {
        self.plan
            .total
            .or_else(|| (self.file_total > 0).then(|| self.base() + self.file_total))
    }

    /// Bytes per second over the last few seconds.
    fn speed(&self) -> Option<f64> {
        let (first, last) = (self.samples.front()?, self.samples.back()?);
        let secs = (last.0 - first.0).as_secs_f64();
        (secs >= 1.0).then(|| last.1.saturating_sub(first.1) as f64 / secs)
    }
}

impl App {
    pub fn new(rt: Handle) -> Self {
        let (volumes, volume_error) = match volumes::detect_volumes() {
            Ok(v) => (v, None),
            Err(err) => (Vec::new(), Some(error_text(&err))),
        };
        let hw = hardware::detect();
        let models = catalog::by_size();
        let selected = Some(models[best_fit(&models, &hw)]);
        let dest = volumes.iter().find(|v| !v.is_fat32()).cloned();
        let client = download::http_client().unwrap_or_default();

        let mut app = App {
            rt,
            client,
            hw,
            volumes,
            volume_error,
            folder: None,
            tab: Tab::All,
            popular: Job::Idle,
            more_abliterated: Job::Idle,
            selected: None,
            source: Source::HuggingFace,
            quant: catalog::DEFAULT_QUANT,
            dest,
            stage: Stage::Choose,
            confirm_quit: false,
            quitting: false,
        };
        if let Some(model) = selected {
            app.select(model);
        }
        app
    }

    fn select(&mut self, model: &'static ModelEntry) {
        self.selected = Some(model);
        self.source = if model.hf_repo.is_some() {
            Source::HuggingFace
        } else {
            Source::Ollama
        };
        self.quant = catalog::DEFAULT_QUANT;
    }

    fn busy(&self) -> bool {
        matches!(
            self.stage,
            Stage::Planning(_) | Stage::Confirm(_) | Stage::Downloading(_)
        )
    }

    /// Move finished background work along. True while anything is running.
    fn poll(&mut self) -> bool {
        let mut running = self.popular.poll() | self.more_abliterated.poll();

        match &mut self.stage {
            Stage::Planning(handle) if handle.is_finished() => {
                self.stage = match handle.now_or_never() {
                    Some(Ok(Ok(plan))) => Stage::Confirm(Arc::new(plan)),
                    Some(Ok(Err(err))) => Stage::Failed {
                        error: error_text(&err),
                        plan: None,
                    },
                    _ => Stage::Failed {
                        error: "Checking the download stopped unexpectedly. Try again.".into(),
                        plan: None,
                    },
                };
            }
            Stage::Planning(_) => running = true,
            Stage::Downloading(run) => {
                run.drain_events();
                if run.handle.is_finished() {
                    let plan = run.plan.clone();
                    let took = run.started.elapsed();
                    self.stage = match (&mut run.handle).now_or_never() {
                        Some(Ok(Ok(()))) => Stage::Done { plan, took },
                        Some(Ok(Err(err))) => Stage::Failed {
                            error: error_text(&err),
                            plan: Some(plan),
                        },
                        _ => Stage::Failed {
                            error: "The download stopped unexpectedly. Start it again to resume."
                                .into(),
                            plan: Some(plan),
                        },
                    };
                } else {
                    running = true;
                }
            }
            _ => {}
        }
        running
    }

    fn start_plan(&mut self) {
        let (Some(model), Some(volume)) = (self.selected, self.dest.clone()) else {
            return;
        };
        let req = DownloadRequest {
            model,
            source: self.source,
            quant: self.quant.to_string(),
            volume,
        };
        let (client, hw) = (self.client.clone(), self.hw.clone());
        self.stage = Stage::Planning(
            self.rt
                .spawn(async move { download::plan(&client, req, &hw).await }),
        );
    }

    fn start_download(&mut self, plan: Arc<DownloadPlan>) {
        let (tx, rx) = unbounded_channel();
        let (client, work) = (self.client.clone(), plan.clone());
        let handle = self
            .rt
            .spawn(async move { download::execute(&client, &work, &tx).await });
        self.stage = Stage::Downloading(Box::new(Running {
            plan,
            handle,
            events: rx,
            started: Instant::now(),
            file: 0,
            done: 0,
            file_total: 0,
            seen_progress: false,
            checking: None,
            notes: Vec::new(),
            samples: VecDeque::new(),
        }));
    }

    fn pause(&mut self) {
        if let Stage::Downloading(run) = &self.stage {
            // Dropping the task mid-stream keeps the .part file for resuming.
            run.handle.abort();
            let plan = run.plan.clone();
            let saved = plan.saved_bytes();
            self.stage = Stage::Paused { plan, saved };
        }
    }

    fn rescan(&mut self) {
        match volumes::detect_volumes() {
            Ok(v) => {
                self.volumes = v;
                self.volume_error = None;
            }
            Err(err) => self.volume_error = Some(error_text(&err)),
        }
        // Forget a drive that was unplugged.
        if let Some(dest) = &self.dest
            && dest.kind != Kind::Folder
            && !self.volumes.iter().any(|v| v.path == dest.path)
        {
            self.dest = None;
        }
    }

    fn choose_folder(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a folder to save models in")
            .pick_folder()
        else {
            return;
        };
        match volumes::from_folder(&path) {
            Ok(vol) => {
                self.dest = Some(vol.clone());
                self.folder = Some(vol);
            }
            Err(err) => {
                self.stage = Stage::Failed {
                    error: error_text(&err),
                    plan: None,
                }
            }
        }
    }

    // ── Left: the model list ────────────────────────────────────────────────

    fn model_list(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::All, "All models");
            ui.selectable_value(&mut self.tab, Tab::Popular, "Most downloaded");
            ui.selectable_value(&mut self.tab, Tab::Abliterated, "Abliterated");
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| match self.tab {
                Tab::All => {
                    let rows: Vec<_> = catalog::by_size().into_iter().map(|m| (m, None)).collect();
                    self.model_table(ui, "all", &rows);
                    legend(ui);
                }
                Tab::Popular => self.popular_tab(ui),
                Tab::Abliterated => self.abliterated_tab(ui),
            });
    }

    fn popular_tab(&mut self, ui: &mut Ui) {
        if matches!(self.popular, Job::Idle) {
            let client = self.client.clone();
            self.popular = Job::start(&self.rt, async move { most_downloaded(&client).await });
        }
        ui.weak("Hugging Face downloads in the last 30 days");
        match &self.popular {
            Job::Idle | Job::Running(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Getting download counts from Hugging Face…");
                });
            }
            Job::Ready(ranked) => {
                let rows: Vec<_> = ranked.iter().map(|(m, n)| (*m, Some(*n))).collect();
                self.model_table(ui, "popular", &rows);
            }
            Job::Failed(error) => {
                let error = error.clone();
                error_box(ui, &error);
                if ui.button("Try again").clicked() {
                    self.popular = Job::Idle;
                }
            }
        }
    }

    fn abliterated_tab(&mut self, ui: &mut Ui) {
        warning(ui, catalog::ABLITERATED_NOTICE);
        ui.add_space(6.0);
        let mut curated: Vec<&'static ModelEntry> = catalog::abliterated().iter().collect();
        curated.sort_by(|a, b| a.size_gb_q4.total_cmp(&b.size_gb_q4));
        let rows: Vec<_> = curated.into_iter().map(|m| (m, None)).collect();
        self.model_table(ui, "abliterated", &rows);

        ui.add_space(10.0);
        match &self.more_abliterated {
            Job::Idle => {
                if ui.button("Check Hugging Face for more…").clicked() {
                    let client = self.client.clone();
                    self.more_abliterated =
                        Job::start(&self.rt, async move { more_abliterated(&client).await });
                }
            }
            Job::Running(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Searching Hugging Face for abliterated models…");
                });
            }
            Job::Ready(found) => {
                ui.strong("More on Hugging Face");
                warning(
                    ui,
                    "These aren't checked by Model Downloader. Many aren't reasoning models, and quality varies.",
                );
                ui.add_space(4.0);
                let rows: Vec<_> = found.iter().map(|(m, n)| (*m, Some(*n))).collect();
                self.model_table(ui, "more-abliterated", &rows);
            }
            Job::Failed(error) => {
                let error = error.clone();
                error_box(ui, &error);
                if ui.button("Try again").clicked() {
                    self.more_abliterated = Job::Idle;
                }
            }
        }
    }

    /// One row per model: name, size, a download count when given, and a
    /// coloured mark for how well it runs here (details on hover).
    fn model_table(&mut self, ui: &mut Ui, id: &str, rows: &[(&'static ModelEntry, Option<u64>)]) {
        let counts = rows.iter().any(|(_, n)| n.is_some());
        let mut clicked = None;
        egui::Grid::new(id)
            .striped(true)
            .num_columns(if counts { 4 } else { 3 })
            .spacing([18.0, 8.0])
            .show(ui, |ui| {
                for (model, count) in rows {
                    let selected = self.selected.is_some_and(|s| std::ptr::eq(s, *model));
                    if ui.selectable_label(selected, model.display_name).clicked() {
                        clicked = Some(*model);
                    }
                    ui.weak(format!("{:.1} GB", model.size_gb_q4));
                    if let Some(n) = count {
                        ui.weak(format!("{} / mo", format_count(*n)));
                    }
                    let fit = self.hw.fit(model.estimated_bytes(catalog::DEFAULT_QUANT));
                    ui.label(RichText::new(fit.glyph()).color(fit_color(ui, fit)))
                        .on_hover_text(format!("{} {}", fit.label(), fit.explain(&self.hw)));
                    ui.end_row();
                }
            });
        if let Some(model) = clicked {
            self.select(model);
        }
    }

    // ── Right: the current step ─────────────────────────────────────────────

    fn step(&mut self, ui: &mut Ui) {
        let mut next = None;
        match &self.stage {
            Stage::Choose => self.choose(ui),
            Stage::Planning(_) => {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.label(format!("Contacting {}…", self.source.label()));
                });
            }
            Stage::Confirm(plan) => next = self.confirm(ui, plan.clone()),
            Stage::Downloading(run) => {
                if downloading(ui, run) {
                    self.pause();
                }
            }
            Stage::Paused { plan, saved } => {
                ui.heading(format!("Paused: {}", plan.req.model.display_name));
                ui.add_space(6.0);
                ui.label(match plan.total {
                    Some(t) => format!("{} of {} saved.", format_size(*saved), format_size(t)),
                    None => format!("{} saved.", format_size(*saved)),
                });
                ui.label("Resume now, or pick the same model again later: it carries on from where it stopped.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(primary_button(ui, "Resume")).clicked() {
                        next = Some(Next::Start(plan.clone()));
                    }
                    if ui.button("Back to models").clicked() {
                        next = Some(Next::Back);
                    }
                });
            }
            Stage::Done { plan, took } => {
                if done(ui, plan, *took) {
                    next = Some(Next::Back);
                }
            }
            Stage::Failed { error, plan } => {
                error_box(ui, error);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if let Some(plan) = plan
                        && ui.add(primary_button(ui, "Try again")).clicked()
                    {
                        next = Some(Next::Start(plan.clone()));
                    }
                    if ui.button("Back to models").clicked() {
                        next = Some(Next::Back);
                    }
                });
            }
        }
        match next {
            Some(Next::Start(plan)) => self.start_download(plan),
            Some(Next::Back) => self.stage = Stage::Choose,
            None => {}
        }
    }

    /// The selected model, and the quality and place to save it.
    fn choose(&mut self, ui: &mut Ui) {
        let Some(model) = self.selected else {
            ui.label("Pick a model on the left.");
            return;
        };

        ui.heading(RichText::new(model.display_name).strong());
        let mut facts = Vec::new();
        if model.params_b > 0.0 {
            facts.push(format!("{:.0}B parameters", model.params_b));
        }
        facts.push(model.reasoning_strength.label().to_string());
        ui.weak(facts.join(" · "));
        ui.add_space(2.0);
        ui.label(model.description);
        if catalog::is_abliterated(model) {
            warning(ui, catalog::ABLITERATED_NOTICE);
        }

        card(ui, "Quality", |ui| self.quality(ui, model));
        card(ui, "Save to", |ui| self.destination(ui));

        egui::CollapsingHeader::new("More options").show(ui, |ui| {
            if model.hf_repo.is_some() && model.ollama_tag.is_some() {
                ui.label("Download from");
                ui.radio_value(
                    &mut self.source,
                    Source::HuggingFace,
                    "Hugging Face (recommended)",
                );
                ui.radio_value(
                    &mut self.source,
                    Source::Ollama,
                    "Ollama, the same file Ollama uses",
                );
            }
            if let Some(repo) = model.hf_repo {
                ui.weak(format!("Hugging Face: {repo}"));
            }
            if let Some(tag) = model.ollama_tag {
                ui.weak(format!("Ollama: {tag}"));
            }
        });

        ui.add_space(12.0);
        let ready = self.dest.is_some();
        if ui
            .add_enabled(ready, primary_button(ui, "Review download"))
            .clicked()
        {
            self.start_plan();
        }
        if !ready {
            ui.weak("Choose where to save the model first.");
        }
    }

    fn quality(&mut self, ui: &mut Ui, model: &'static ModelEntry) {
        let bytes = match self.source {
            Source::HuggingFace => {
                egui::ComboBox::from_id_salt("quant")
                    .width(ui.available_width().min(380.0))
                    .selected_text(format!(
                        "{} · {}",
                        self.quant,
                        catalog::quant_note(self.quant)
                    ))
                    .show_ui(ui, |ui| {
                        for q in catalog::available_quants() {
                            let fit = self.hw.fit(model.estimated_bytes(q));
                            let text =
                                RichText::new(format!("{} {}", fit.glyph(), quant_row(model, q)))
                                    .color(fit_color(ui, fit));
                            ui.selectable_value(&mut self.quant, q, text);
                        }
                    });
                model.estimated_bytes(self.quant)
            }
            Source::Ollama => {
                ui.label("Ollama's own file, at a fixed quality");
                model.estimated_bytes(catalog::DEFAULT_QUANT)
            }
        };
        let fit = self.hw.fit(bytes);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("About {:.1} GB ·", bytes as f64 / 1e9));
            ui.label(fit_text(ui, fit));
            ui.weak(fit.explain(&self.hw));
        });
    }

    fn destination(&mut self, ui: &mut Ui) {
        if let Some(error) = &self.volume_error {
            error_box(ui, error);
        }
        let mut places: Vec<Volume> = self.volumes.clone();
        places.extend(self.folder.clone());

        let mut picked = None;
        egui::Grid::new("drives")
            .num_columns(2)
            .spacing([18.0, 6.0])
            .show(ui, |ui| {
                for vol in &places {
                    let selected = self.dest.as_ref().is_some_and(|d| d.path == vol.path);
                    let usable = !vol.is_fat32();
                    let row = ui
                        .add_enabled_ui(usable, |ui| ui.selectable_label(selected, vol.name()))
                        .inner;
                    if row.clicked() {
                        picked = Some(vol.clone());
                    }
                    if usable {
                        row.on_hover_text(format!(
                            "Saves to {} · {} · {:.0} GB drive",
                            vol.path.join("models").display(),
                            vol.filesystem,
                            vol.total_gb()
                        ));
                    } else {
                        row.on_disabled_hover_text(format!(
                            "FAT32 can't hold files over 4 GB. Reformat it as {} to use it.",
                            volumes::BIG_FILE_FORMATS
                        ));
                    }
                    if usable {
                        ui.label(
                            RichText::new(free_space(vol.free_bytes))
                                .color(free_color(ui, vol.free_bytes)),
                        );
                    } else {
                        ui.weak("FAT32, can't be used");
                    }
                    ui.end_row();
                }
            });
        if let Some(vol) = picked {
            self.dest = Some(vol);
        }

        ui.horizontal(|ui| {
            if ui.small_button("Choose a folder…").clicked() {
                self.choose_folder();
            }
            if ui.small_button("Rescan").clicked() {
                self.rescan();
            }
        });
    }

    /// The download summary. Returns what to do next, if a button was pressed.
    fn confirm(&self, ui: &mut Ui, plan: Arc<DownloadPlan>) -> Option<Next> {
        let req = &plan.req;
        ui.heading(RichText::new("Ready to download").strong());
        ui.add_space(4.0);
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12))
            .corner_radius(8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("summary")
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        ui.weak("Model");
                        ui.strong(req.model.display_name);
                        ui.end_row();

                        ui.weak("Source");
                        ui.label(match req.source {
                            Source::HuggingFace => {
                                format!("{} · {}", req.source.label(), req.quant)
                            }
                            Source::Ollama => format!(
                                "{} · {}",
                                req.source.label(),
                                req.model.ollama_tag.unwrap_or("?")
                            ),
                        });
                        ui.end_row();

                        ui.weak(if req.volume.kind == Kind::Folder {
                            "Folder"
                        } else {
                            "Drive"
                        });
                        ui.label(format!(
                            "{} ({}) · {}",
                            req.volume.name(),
                            req.volume.filesystem,
                            free_space(req.volume.free_bytes)
                        ));
                        ui.end_row();

                        ui.weak("Saving to");
                        ui.label(plan.folder().display().to_string());
                        ui.end_row();

                        for (i, file) in plan.files.iter().enumerate() {
                            ui.weak(if i == 0 { "Files" } else { "" });
                            ui.label(&file.filename);
                            ui.end_row();
                        }

                        if let Some(total) = plan.total {
                            ui.weak("Size");
                            if plan.have > 0 {
                                ui.label(format!(
                                    "{} ({} already downloaded)",
                                    format_size(total),
                                    format_size(plan.have)
                                ));
                            } else {
                                ui.label(format_size(total));
                            }
                            ui.end_row();
                        }

                        if let Some(fit) = plan.fit {
                            ui.weak(format!("Your {COMPUTER}"));
                            ui.horizontal_wrapped(|ui| {
                                ui.label(fit_text(ui, fit));
                                ui.weak(fit.explain(&self.hw));
                            });
                            ui.end_row();
                        }
                    });
            });

        ui.add_space(8.0);
        for w in &plan.warnings {
            if plan.too_big() && w.starts_with("This model is too big") {
                ui.colored_label(ui.visuals().error_fg_color, format!("✖ {w}"));
            } else {
                warning(ui, w);
            }
            ui.add_space(4.0);
        }

        ui.add_space(10.0);
        let mut next = None;
        ui.horizontal(|ui| {
            let label = match plan.remaining() {
                Some(0) => "Check and finish".to_string(),
                _ if plan.too_big() => "Download anyway".to_string(),
                Some(n) => format!("Download {}", format_size(n)),
                None => "Download".to_string(),
            };
            // Don't make a download that probably won't run the obvious choice.
            let button = if plan.too_big() {
                egui::Button::new(label)
            } else {
                primary_button(ui, &label)
            };
            if ui.add(button).clicked() {
                next = Some(Next::Start(plan.clone()));
            }
            if ui.button("Back").clicked() {
                next = Some(Next::Back);
            }
        });
        next
    }

    fn quit_dialog(&mut self, ctx: &egui::Context) {
        let mut close = false;
        egui::Modal::new(egui::Id::unique("quit")).show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.heading("A download is running");
            ui.label("Pause it and quit? Pick the same model next time and it carries on from where it stopped.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Pause and quit").strong()).clicked() {
                    close = true;
                }
                if ui.button("Keep downloading").clicked() {
                    self.confirm_quit = false;
                }
            });
        });
        if close {
            self.pause();
            self.quitting = true;
            self.confirm_quit = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

enum Next {
    Start(Arc<DownloadPlan>),
    Back,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.poll() {
            ctx.request_repaint_after(TICK);
        }

        if ctx.input(|i| i.viewport().close_requested())
            && matches!(self.stage, Stage::Downloading(_))
            && !self.quitting
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_quit = true;
        }

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Model Downloader").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("Your {COMPUTER}: {}", self.hw.summary()));
                });
            });
            ui.add_space(8.0);
        });

        let busy = self.busy();
        egui::Panel::left("models")
            .resizable(true)
            .default_size(420.0)
            .min_size(340.0)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.add_enabled_ui(!busy, |ui| self.model_list(ui));
            });

        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    self.step(ui);
                });
        });

        if self.confirm_quit {
            self.quit_dialog(&ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Closing while paused or between downloads: nothing to stop. Closing
        // mid-download after "Pause and quit" already aborted it.
        if let Stage::Downloading(run) = &self.stage {
            run.handle.abort();
        }
    }
}

// ── Views that don't need the whole app ─────────────────────────────────────

/// Progress for a running download. Returns true when Pause is pressed.
fn downloading(ui: &mut Ui, run: &Running) -> bool {
    let plan = &run.plan;
    ui.heading(format!("Downloading {}", plan.req.model.display_name));
    ui.weak(format!("to {}", plan.folder().display()));
    ui.add_space(10.0);

    if let Some((done, total)) = run.checking {
        ui.label("Checking the part already downloaded…");
        ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).show_percentage());
    } else {
        let overall = run.overall();
        let total = run.total();
        let fraction = total.map_or(0.0, |t| overall as f32 / t.max(1) as f32);
        ui.add(
            egui::ProgressBar::new(fraction)
                .desired_height(22.0)
                .show_percentage()
                .animate(run.seen_progress),
        );
        ui.add_space(4.0);

        let mut line = match total {
            Some(t) => format!("{} of {}", format_size(overall), format_size(t)),
            None => format_size(overall),
        };
        if let Some(speed) = run.speed().filter(|s| *s > 0.0) {
            line += &format!("  ·  {:.1} MB/s", speed / 1_048_576.0);
            if let Some(t) = total {
                let eta = Duration::from_secs_f64(t.saturating_sub(overall) as f64 / speed);
                line += &format!("  ·  about {} left", format_duration(eta));
            }
        } else if !run.seen_progress {
            line = "Starting…".into();
        }
        ui.label(line);
    }
    if plan.files.len() > 1 {
        ui.weak(format!("Part {} of {}", run.file + 1, plan.files.len()));
    }

    for note in run.notes.iter().rev().take(4).rev() {
        ui.weak(note);
    }

    ui.add_space(12.0);
    let pause = ui.button(RichText::new("Pause").strong()).clicked();
    ui.weak("Paused downloads carry on from where they stopped.");
    pause
}

/// The finished download. Returns true when the user goes back to the list.
fn done(ui: &mut Ui, plan: &DownloadPlan, took: Duration) -> bool {
    let green = green(ui);
    ui.heading(RichText::new(format!("✔ Done in {}", format_duration(took))).color(green));
    ui.add_space(6.0);
    ui.label(format!("{} is saved to:", plan.req.model.display_name));
    ui.monospace(plan.dests[0].display().to_string());
    if plan.dests.len() > 1 {
        ui.weak(format!(
            "It's split into {} parts. Point your app at the first one and keep the parts together.",
            plan.dests.len()
        ));
    }

    ui.add_space(10.0);
    let command = format!("llama-cli -m \"{}\"", plan.dests[0].display());
    ui.horizontal(|ui| {
        if ui.button("Open folder").clicked() {
            open_folder(&plan.folder());
        }
        if ui
            .button("Copy llama.cpp command")
            .on_hover_text(&command)
            .clicked()
        {
            ui.ctx().copy_text(command.clone());
        }
    });
    ui.add_space(6.0);
    ui.weak("Or open the folder in LM Studio, or import the file into Ollama with a Modelfile.");

    ui.add_space(14.0);
    ui.button("Download another model").clicked()
}

fn open_folder(path: &Path) {
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    let _ = std::process::Command::new(opener).arg(path).spawn();
}

/// A titled, rounded box that groups one choice.
fn card<R>(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add_space(10.0);
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(12))
        .corner_radius(8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong(title);
            ui.add_space(2.0);
            add_contents(ui)
        })
        .inner
}

/// The one button on a screen that moves things forward.
fn primary_button(ui: &Ui, text: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(text)
            .strong()
            .size(15.0)
            .color(Color32::WHITE),
    )
    .fill(ui.visuals().selection.bg_fill)
    .corner_radius(6)
    .min_size(egui::vec2(170.0, 34.0))
}

/// What the marks in the model list mean.
fn legend(ui: &mut Ui) {
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        for fit in [Fit::Good, Fit::Tight, Fit::TooBig] {
            ui.label(fit_text(ui, fit).small());
            ui.add_space(6.0);
        }
        ui.weak(RichText::new(format!("on your {COMPUTER}")).small());
    });
}

fn warning(ui: &mut Ui, text: &str) {
    let color = ui.visuals().warn_fg_color;
    ui.colored_label(color, format!("⚠ {text}"));
}

/// An error and its causes, one per line.
fn error_box(ui: &mut Ui, error: &str) {
    let color = ui.visuals().error_fg_color;
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, color))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut lines = error.lines();
            ui.colored_label(
                color,
                RichText::new(format!("✖ {}", lines.next().unwrap_or_default())).strong(),
            );
            for line in lines {
                ui.label(line);
            }
        });
}

fn error_text(err: &anyhow::Error) -> String {
    err.chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn green(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(110, 200, 120)
    } else {
        Color32::from_rgb(30, 130, 60)
    }
}

fn fit_color(ui: &Ui, fit: Fit) -> Color32 {
    match fit {
        Fit::Fast | Fit::Good => green(ui),
        Fit::Tight => ui.visuals().warn_fg_color,
        Fit::TooBig => ui.visuals().error_fg_color,
    }
}

fn fit_text(ui: &Ui, fit: Fit) -> RichText {
    RichText::new(format!("{} {}", fit.glyph(), fit.label())).color(fit_color(ui, fit))
}

/// Free space coloured by how comfortable it is for multi-GB models.
fn free_color(ui: &Ui, bytes: u64) -> Color32 {
    match bytes as f64 / GIB {
        gb if gb < 20.0 => ui.visuals().error_fg_color,
        gb if gb < 100.0 => ui.visuals().warn_fg_color,
        _ => green(ui),
    }
}

/// "Q4_K_M · ~5.2 GB · recommended balance of size and quality"
fn quant_row(model: &ModelEntry, quant: &str) -> String {
    format!(
        "{quant} · ~{:.1} GB · {}",
        model.estimated_bytes(quant) as f64 / 1e9,
        catalog::quant_note(quant)
    )
}

/// The largest model that runs well here, or the smallest if none do.
fn best_fit(models: &[&ModelEntry], hw: &Hardware) -> usize {
    models
        .iter()
        .rposition(|m| {
            hw.fit(m.estimated_bytes(catalog::DEFAULT_QUANT))
                .runs_well()
        })
        .unwrap_or(0)
}

/// Catalog models ranked by Hugging Face downloads over the last 30 days.
async fn most_downloaded(client: &reqwest::Client) -> anyhow::Result<Ranked> {
    let counts = join_all(catalog::catalog().iter().map(|m| async move {
        let repo = m.hf_repo?;
        downloader::hf_downloads(client, repo)
            .await
            .ok()
            .map(|n| (m, n))
    }))
    .await;
    let mut ranked: Ranked = counts.into_iter().flatten().collect();
    if ranked.is_empty() {
        anyhow::bail!(
            "Couldn't get download counts from Hugging Face. Check your internet connection."
        );
    }
    ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    Ok(ranked)
}

/// bartowski's most downloaded abliterated models that aren't in the curated list.
async fn more_abliterated(client: &reqwest::Client) -> anyhow::Result<Ranked> {
    let curated: Vec<&str> = catalog::abliterated()
        .iter()
        .filter_map(|m| m.hf_repo)
        .collect();
    let live: Ranked = downloader::hf_abliterated(client)
        .await?
        .into_iter()
        .filter(|l| !curated.contains(&l.repo.as_str()))
        .map(|l| (catalog::live_entry(&l.repo, l.q4_bytes), l.downloads))
        .collect();
    if live.is_empty() {
        anyhow::bail!("No other abliterated models found on Hugging Face.");
    }
    Ok(live)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_fit_picks_the_biggest_model_that_runs_well() {
        let hw = Hardware {
            ram_bytes: 16 * 1_073_741_824,
            free_ram_bytes: 16 * 1_073_741_824,
            gpu: None,
        };
        let models = catalog::by_size();
        let pick = models[best_fit(&models, &hw)];
        assert!(
            hw.fit(pick.estimated_bytes(catalog::DEFAULT_QUANT))
                .runs_well()
        );
        assert!(
            models
                .iter()
                .filter(|m| m.size_gb_q4 > pick.size_gb_q4)
                .all(|m| !hw
                    .fit(m.estimated_bytes(catalog::DEFAULT_QUANT))
                    .runs_well())
        );
    }
}
