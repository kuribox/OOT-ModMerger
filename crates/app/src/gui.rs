//! The window: pick the game's RomFS, drop the mod folders in order, see what is in conflict, choose
//! who wins where a file cannot be merged, and merge.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

use modmerge_core::merge::{self, Mod, Outcome};

use crate::i18n::{self, Lang};
use crate::theme;

#[derive(Clone)]
struct RowView {
    action: &'static str,
    file: String,
    detail: String,
}

struct Job(Result<Outcome, String>);

enum Action {
    Swap(usize, usize),
    Remove(usize),
    Move(usize, usize),
}

pub struct App {
    lang: Lang,
    romfs: String,
    out: String,
    name: String,
    all: bool,
    mods: Vec<(String, String)>,
    rows: Vec<RowView>,
    warnings: Vec<String>,
    advice: Option<String>,
    summary: String,
    filter: String,
    overrides: BTreeMap<String, usize>,
    busy: bool,
    rx: Option<Receiver<Job>>,
    drag_from: Option<usize>,
    drop_hint: bool,
}

impl Default for App {
    fn default() -> Self {
        let lang = Lang::from_system();
        Self {
            lang,
            romfs: String::new(),
            out: String::new(),
            name: "zzzz-merged".to_string(),
            all: false,
            mods: Vec::new(),
            rows: Vec::new(),
            warnings: Vec::new(),
            advice: None,
            summary: i18n::t(lang, "drop_hint").to_string(),
            filter: String::new(),
            overrides: BTreeMap::new(),
            busy: false,
            rx: None,
            drag_from: None,
            drop_hint: false,
        }
    }
}

impl App {
    fn t(&self, key: &str) -> &'static str {
        i18n::t(self.lang, key)
    }

    fn add(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        if !self.mods.iter().any(|(_, p)| *p == path.display().to_string()) {
            self.mods.push((name, path.display().to_string()));
        }
    }

    fn start(&mut self, write: bool) {
        if self.mods.len() < 2 {
            self.summary = self.t("need_two").to_string();
            return;
        }
        let Some(romfs) = existing(&self.romfs) else {
            self.summary = self.t("need_romfs").to_string();
            return;
        };
        let out = if write {
            match existing(&self.out) {
                Some(o) => o,
                None => {
                    self.summary = self.t("need_out").to_string();
                    return;
                }
            }
        } else {
            std::env::temp_dir()
        };
        let mods: Vec<Mod> = self
            .mods
            .iter()
            .filter_map(|(name, path)| {
                let p = PathBuf::from(path);
                let romfs = if p.join("romfs").is_dir() { p.join("romfs") } else { p.clone() };
                romfs.is_dir().then(|| Mod { name: name.clone(), romfs })
            })
            .collect();
        if mods.len() < 2 {
            self.summary = self.t("not_a_mod").to_string();
            return;
        }
        let (name, all, overrides) = (self.name.clone(), self.all, self.overrides.clone());
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.busy = true;
        self.summary = self.t(if write { "merging" } else { "scanning" }).to_string();
        std::thread::spawn(move || {
            let _ = tx.send(Job(merge::run_with(&romfs, &mods, &out, &name, all, !write, &overrides)));
        });
    }

    fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        if let Ok(Job(result)) = rx.try_recv() {
            self.busy = false;
            self.rx = None;
            match result {
                Ok(o) => {
                    self.rows = o
                        .rows
                        .iter()
                        .map(|r| RowView { action: r.action, file: r.file.clone(), detail: r.detail.clone() })
                        .collect();
                    self.warnings = o.warnings.clone();
                    self.advice = o.advice.clone();
                    let written = if o.written > 0 {
                        format!(
                            ", {} {} {}",
                            o.written,
                            self.t("written_to"),
                            Path::new(&self.out).join(&self.name).display()
                        )
                    } else {
                        String::new()
                    };
                    self.summary = format!(
                        "{} {} · {} {}: {} merged, {} kept{}",
                        o.files, self.t("files"), o.overlaps, self.t("in_conflict"), o.merged, o.kept, written
                    );
                }
                Err(e) => self.summary = format!("{}: {e}", self.t("error")),
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            theme::triforce(ui, 16.0);
            ui.add_space(2.0);
            ui.label(egui::RichText::new(self.t("title")).heading().color(theme::GOLD));
            ui.add_space(8.0);
            ui.add(
                egui::Label::new(egui::RichText::new(self.t("subtitle")).small().color(theme::PARCHMENT.gamma_multiply(0.7))),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("lang")
                    .selected_text(self.lang.code())
                    .width(52.0)
                    .show_ui(ui, |ui| {
                        for l in Lang::all() {
                            if ui.selectable_value(&mut self.lang, l, l.code()).clicked() {
                                self.summary = i18n::t(l, "drop_hint").to_string();
                            }
                        }
                    })
                    .response
                    .on_hover_text(i18n::t(self.lang, "lang"));
            });
        });
        ui.add_space(2.0);
        ui.separator();
    }

    fn game_card(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0, theme::GOLD_DIM))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(self.t("game")).strong().color(theme::GOLD));
                egui::Grid::new("game").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(self.t("romfs"));
                    let hint = self.t("romfs_hint");
                    ui.add(egui::TextEdit::singleline(&mut self.romfs).hint_text(hint).desired_width(f32::INFINITY));
                    if ui.button(self.t("browse")).clicked() {
                        if let Some(d) = rfd::FileDialog::new().set_title(self.t("pick_romfs")).pick_folder() {
                            self.romfs = d.display().to_string();
                        }
                    }
                    ui.end_row();
                    ui.label(self.t("output"));
                    let hint = self.t("out_hint");
                    ui.add(egui::TextEdit::singleline(&mut self.out).hint_text(hint).desired_width(f32::INFINITY));
                    if ui.button(self.t("browse")).clicked() {
                        if let Some(d) = rfd::FileDialog::new().set_title(self.t("pick_out")).pick_folder() {
                            self.out = d.display().to_string();
                        }
                    }
                    ui.end_row();
                });
            });
    }

    fn mods_card(&mut self, ui: &mut egui::Ui) {
        let stroke = if self.drop_hint {
            egui::Stroke::new(2.0, theme::GOLD)
        } else {
            egui::Stroke::new(1.0, theme::GOLD_DIM)
        };
        egui::Frame::group(ui.style())
            .fill(theme::PANEL)
            .stroke(stroke)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(self.t("mods")).strong().color(theme::GOLD));
                    ui.add(
                        egui::Label::new(egui::RichText::new(self.t("mods_hint")).small().color(theme::PARCHMENT.gamma_multiply(0.7))),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.t("add")).clicked() {
                            if let Some(d) = rfd::FileDialog::new().set_title(self.t("pick_mod")).pick_folder() {
                                self.add(d);
                            }
                        }
                    });
                });
                ui.add_space(4.0);
                let mut action = None;
                egui::ScrollArea::vertical().max_height(190.0).id_salt("mods").show(ui, |ui| {
                    for i in 0..self.mods.len() {
                        let mut handle_resp = None;
                        let shown = egui::Frame::group(ui.style())
                            .fill(theme::STONE)
                            .stroke(egui::Stroke::new(1.0, theme::GOLD_DIM.gamma_multiply(0.6)))
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                let handle_resp = &mut handle_resp;
                                ui.horizontal(|ui| {
                                    let handle = ui.add(
                                        egui::Label::new(egui::RichText::new("⣿").color(theme::GOLD_DIM))
                                            .sense(egui::Sense::drag()),
                                    );
                                    *handle_resp = Some(handle);
                                    ui.label(egui::RichText::new(format!("{}", i + 1)).monospace().color(theme::GOLD));
                                    ui.label(egui::RichText::new(&self.mods[i].0).strong());
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&self.mods[i].1).monospace().small().color(theme::PARCHMENT.gamma_multiply(0.55)),
                                        )
                                        .truncate(),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("✕").clicked() {
                                            action = Some(Action::Remove(i));
                                        }
                                        if ui.small_button("↓").clicked() && i + 1 < self.mods.len() {
                                            action = Some(Action::Swap(i, i + 1));
                                        }
                                        if ui.small_button("↑").clicked() && i > 0 {
                                            action = Some(Action::Swap(i, i - 1));
                                        }
                                    });
                                });
                            });
                        let over_this = shown.response.hovered();
                        if handle_resp.map(|h| h.drag_started()).unwrap_or(false) {
                            self.drag_from = Some(i);
                        }
                        if let Some(from) = self.drag_from {
                            if from != i && over_this && ui.input(|s| s.pointer.any_released()) {
                                action = Some(Action::Move(from, i));
                                self.drag_from = None;
                            }
                        }
                    }
                });
                match action {
                    Some(Action::Swap(a, b)) => self.mods.swap(a, b),
                    Some(Action::Remove(i)) => {
                        self.mods.remove(i);
                    }
                    Some(Action::Move(from, to)) => {
                        let m = self.mods.remove(from);
                        self.mods.insert(to.min(self.mods.len()), m);
                    }
                    None => {}
                }
            });
    }

    fn output_card(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0, theme::GOLD_DIM))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(self.t("output_name")).strong().color(theme::GOLD));
                    ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(190.0));
                    let copy_all = self.t("copy_all");
                    ui.checkbox(&mut self.all, copy_all);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(!self.busy, |ui| {
                            if ui
                                .add(egui::Button::new(egui::RichText::new(self.t("merge")).strong().color(theme::BG)).fill(theme::GOLD))
                                .clicked()
                            {
                                self.start(true);
                            }
                            if ui.button(self.t("scan")).clicked() {
                                self.start(false);
                            }
                        });
                        if self.busy {
                            ui.spinner();
                        }
                    });
                });
                ui.add_space(2.0);
                match &self.advice {
                    Some(a) => {
                        ui.label(egui::RichText::new(format!("⚠ {a}")).color(theme::WARN));
                    }
                    None => {
                        ui.label(egui::RichText::new(self.t("name_hint")).small().color(theme::PARCHMENT.gamma_multiply(0.6)));
                    }
                }
            });
    }

    fn conflicts(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(self.t("conflicts")).strong().color(theme::GOLD));
            let hint = self.t("filter");
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text(hint).desired_width(220.0));
            if !self.warnings.is_empty() {
                ui.label(
                    egui::RichText::new(format!("{} {}", self.warnings.len(), self.t("cannot_merge")))
                        .color(theme::WARN)
                        .strong(),
                )
                .on_hover_text(self.warnings.join("\n"));
            }
        });
        if !self.warnings.is_empty() {
            egui::Frame::group(ui.style())
                .fill(theme::WARN_BG)
                .stroke(egui::Stroke::new(1.0, theme::WARN.gamma_multiply(0.6)))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(self.t("warn_title")).strong().color(theme::WARN));
                    for w in self.warnings.iter().take(5) {
                        ui.label(egui::RichText::new(format!("• {w}")).small());
                    }
                    if self.warnings.len() > 5 {
                        ui.label(egui::RichText::new(format!("{} {}", self.t("warn_more"), self.warnings.len() - 5)).small());
                    }
                });
            ui.add_space(4.0);
        }
        let names: Vec<String> = self.mods.iter().map(|(n, _)| n.clone()).collect();
        egui::ScrollArea::both().max_height(300.0).id_salt("rows").show(ui, |ui| {
            egui::Grid::new("rowsgrid").num_columns(3).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                ui.strong(self.t("col_file"));
                ui.strong(self.t("col_what"));
                ui.strong(self.t("col_detail"));
                ui.end_row();
                for row in &self.rows {
                    if !self.filter.is_empty() && !row.file.to_lowercase().contains(&self.filter.to_lowercase()) {
                        continue;
                    }
                    let (color, label) = if row.action == "merged" {
                        (theme::OK, self.t("merged"))
                    } else {
                        (theme::WARN, self.t("kept"))
                    };
                    ui.add(egui::Label::new(egui::RichText::new(&row.file).monospace().small()).truncate());
                    ui.label(egui::RichText::new(label).color(color).strong());
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(egui::RichText::new(&row.detail).small()).truncate());
                        if row.action != "merged" && names.len() > 1 {
                            let current = self.overrides.get(&row.file).copied().unwrap_or(names.len() - 1);
                            let mut pick = current;
                            egui::ComboBox::from_id_salt(format!("win-{}", row.file))
                                .selected_text(names.get(current).cloned().unwrap_or_default())
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    for (i, n) in names.iter().enumerate() {
                                        ui.selectable_value(&mut pick, i, n);
                                    }
                                });
                            if pick != current {
                                self.overrides.insert(row.file.clone(), pick);
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        let ctx = ui.ctx().clone();
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            if p.is_dir() {
                self.add(p);
            }
        }
        self.drop_hint = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::Panel::top("head").show(ui, |ui| self.header(ui));
        egui::Panel::bottom("foot").show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&self.summary).small().color(theme::PARCHMENT.gamma_multiply(0.85)));
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.game_card(ui);
                ui.add_space(6.0);
                self.mods_card(ui);
                ui.add_space(6.0);
                self.output_card(ui);
                ui.add_space(8.0);
                self.conflicts(ui);
            });
        });
    }
}

fn existing(path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path.trim());
    p.is_dir().then_some(p)
}

pub fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 800.0])
            .with_min_inner_size([780.0, 580.0])
            .with_title("OoT Mod Merge"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "OoT Mod Merge",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::<App>::default())
        }),
    )
}
