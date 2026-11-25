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
    dirty: bool,
    romfs_rect: Option<egui::Rect>,
    out_rect: Option<egui::Rect>,
}

impl Default for App {
    fn default() -> Self {
        let saved = crate::config::Config::load();
        let lang = match saved.lang.as_str() {
            "es" => Lang::Es,
            "en" => Lang::En,
            _ => Lang::from_system(),
        };
        let mods: Vec<(String, String)> = saved
            .mods
            .iter()
            .map(|p| {
                let name = Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.clone());
                (name, p.clone())
            })
            .collect();
        let overrides = saved
            .wins
            .iter()
            .filter_map(|(file, who)| {
                let idx = mods.iter().position(|(n, _)| n == who)?;
                Some((file.clone(), idx))
            })
            .collect();
        Self {
            lang,
            romfs: saved.romfs,
            out: saved.out,
            name: if saved.name.is_empty() { "zzzzzz-merged".to_string() } else { saved.name },
            all: saved.all,
            mods,
            rows: Vec::new(),
            warnings: Vec::new(),
            advice: None,
            summary: i18n::t(lang, "drop_hint").to_string(),
            filter: String::new(),
            overrides,
            busy: false,
            rx: None,
            drag_from: None,
            drop_hint: false,
            dirty: false,
            romfs_rect: None,
            out_rect: None,
        }
    }
}

impl App {
    fn t(&self, key: &str) -> &'static str {
        i18n::t(self.lang, key)
    }

    /// Where a dropped folder belongs: the field it was dropped on, else what it looks like (a mod has
    /// a romfs/ inside, a game has romfs/ and mods/, the game's RomFS has Pack/ZsDic.pack.zs).
    fn drop_folder(&mut self, p: PathBuf, pointer: Option<egui::Pos2>) {
        let over = |rect: &Option<egui::Rect>| match (rect, pointer) {
            (Some(r), Some(q)) => r.contains(q),
            _ => false,
        };
        let romfs_like = p.join("Pack").join("ZsDic.pack.zs").is_file();
        let path = p.display().to_string();
        if over(&self.romfs_rect) || (romfs_like && self.romfs.trim().is_empty()) {
            self.romfs = path;
            self.dirty = true;
            return;
        }
        if over(&self.out_rect) {
            self.out = path;
            self.dirty = true;
            return;
        }
        if p.join("romfs").is_dir() && p.join("mods").is_dir() {
            self.romfs = p.join("romfs").display().to_string();
            self.out = p.join("mods").display().to_string();
            self.dirty = true;
            return;
        }
        if romfs_like {
            self.romfs = path;
            self.dirty = true;
            return;
        }
        if p.join("romfs").is_dir() {
            self.add(p);
            return;
        }
        if self.out.trim().is_empty() {
            self.out = path;
            self.dirty = true;
        } else {
            self.add(p);
        }
    }

    fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let names: Vec<String> = self.mods.iter().map(|(n, _)| n.clone()).collect();
        let cfg = crate::config::Config {
            lang: match self.lang {
                Lang::Es => "es".to_string(),
                Lang::En => "en".to_string(),
            },
            romfs: self.romfs.clone(),
            out: self.out.clone(),
            name: self.name.clone(),
            all: self.all,
            mods: self.mods.iter().map(|(_, p)| p.clone()).collect(),
            wins: self
                .overrides
                .iter()
                .filter_map(|(file, i)| names.get(*i).map(|n| (file.clone(), n.clone())))
                .collect(),
        };
        cfg.save();
        self.dirty = false;
    }

    fn add(&mut self, path: PathBuf) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        if !self.mods.iter().any(|(_, p)| *p == path.display().to_string()) {
            self.mods.push((name, path.display().to_string()));
            self.dirty = true;
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
                                self.dirty = true;
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

    fn game_card(&mut self, ui: &mut egui::Ui, hot: Option<egui::Pos2>) {
        let (l_romfs, l_out, browse, t_romfs, t_out) =
            (self.t("romfs"), self.t("output"), self.t("browse"), self.t("pick_romfs"), self.t("pick_out"));
        let missing = self.t("missing_path");
        egui::Frame::group(ui.style())
            .fill(theme::PANEL)
            .stroke(egui::Stroke::new(1.0, theme::GOLD_DIM))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(self.t("game")).strong().color(theme::GOLD));
                let mut changed = false;
                let over = |rect: &Option<egui::Rect>| match (rect, hot) {
                    (Some(r), Some(q)) => r.contains(q),
                    _ => false,
                };
                let (hot_romfs, hot_out) = (over(&self.romfs_rect), over(&self.out_rect));
                let mut rects = Vec::new();
                for (label, value, title, is_hot) in [
                    (l_romfs, &mut self.romfs, t_romfs, hot_romfs),
                    (l_out, &mut self.out, t_out, hot_out),
                ] {
                    let (typed, picked, rect) = path_row(ui, label, value, browse, title, is_hot);
                    rects.push(rect);
                    if let Some(d) = picked {
                        *value = d;
                        changed = true;
                    }
                    changed |= typed;
                    if !value.trim().is_empty() && !Path::new(value.trim()).is_dir() {
                        ui.label(egui::RichText::new(format!("{label}: {missing}")).small().color(theme::WARN));
                    }
                }
                if changed {
                    self.dirty = true;
                }
                self.romfs_rect = rects.first().copied();
                self.out_rect = rects.get(1).copied();
            });
    }

    /// Three ruled lines that mean "drag me": drawn, because the font has no glyph for it.
    fn drag_handle(&self, ui: &mut egui::Ui) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), egui::Sense::drag());
        let color = if resp.hovered() { theme::GOLD } else { theme::GOLD_DIM };
        let painter = ui.painter();
        for i in 0..3 {
            let y = rect.center().y + (i as f32 - 1.0) * 5.0;
            painter.line_segment(
                [egui::pos2(rect.left() + 2.0, y), egui::pos2(rect.right() - 2.0, y)],
                egui::Stroke::new(1.6, color),
            );
        }
        resp.on_hover_text(self.t("drag")).on_hover_cursor(egui::CursorIcon::Grab)
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
                                    let handle = self.drag_handle(ui);
                                    *handle_resp = Some(handle);
                                    ui.label(egui::RichText::new(format!("{}", i + 1)).monospace().color(theme::GOLD));
                                    ui.label(egui::RichText::new(&self.mods[i].0).strong());
                                    let gone = !Path::new(&self.mods[i].1).is_dir();
                                    let color = if gone { theme::WARN } else { theme::PARCHMENT.gamma_multiply(0.55) };
                                    let mut label =
                                        egui::RichText::new(&self.mods[i].1).monospace().small().color(color);
                                    if gone {
                                        label = label.strikethrough();
                                    }
                                    ui.add(egui::Label::new(label).truncate())
                                        .on_hover_text(self.mods[i].1.clone());
                                    let (up, down, remove) =
                                        (self.t("up"), self.t("down"), self.t("remove"));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button(remove).clicked() {
                                            action = Some(Action::Remove(i));
                                        }
                                        if ui.add_enabled(i + 1 < self.mods.len(), egui::Button::new(down).small()).clicked() {
                                            action = Some(Action::Swap(i, i + 1));
                                        }
                                        if ui.add_enabled(i > 0, egui::Button::new(up).small()).clicked() {
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
                    Some(Action::Swap(a, b)) => {
                        self.mods.swap(a, b);
                        self.dirty = true;
                    }
                    Some(Action::Remove(i)) => {
                        self.mods.remove(i);
                        self.overrides.clear();
                        self.dirty = true;
                    }
                    Some(Action::Move(from, to)) => {
                        let m = self.mods.remove(from);
                        self.mods.insert(to.min(self.mods.len()), m);
                        self.dirty = true;
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
                        ui.label(egui::RichText::new(format!("{} {a}", self.t("warn_prefix"))).color(theme::WARN));
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
                        ui.label(egui::RichText::new(format!("- {w}")).small());
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
                                self.dirty = true;
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
        self.save();
        let ctx = ui.ctx().clone();
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        for p in dropped {
            if p.is_dir() {
                self.drop_folder(p, pointer);
            }
        }
        self.drop_hint = hovering;

        egui::Panel::top("head").show(ui, |ui| self.header(ui));
        egui::Panel::bottom("foot").show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&self.summary).small().color(theme::PARCHMENT.gamma_multiply(0.85)));
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.game_card(ui, if self.drop_hint { pointer } else { None });
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

/// A labelled path field that takes the whole width of the card, so a long path can be read, with the
/// whole of it also on hover.
fn path_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    browse: &str,
    title: &str,
    hot: bool,
) -> (bool, Option<String>, egui::Rect) {
    ui.horizontal(|ui| {
        ui.add_sized([72.0, 18.0], egui::Label::new(label));
        let room = (ui.available_width() - 86.0).max(200.0);
        let resp = ui.add(egui::TextEdit::singleline(value).desired_width(room).font(egui::TextStyle::Monospace));
        let typed = resp.changed();
        if hot {
            ui.painter().rect_stroke(
                resp.rect.expand(2.0),
                6,
                egui::Stroke::new(2.0, theme::GOLD),
                egui::StrokeKind::Outside,
            );
        }
        let rect = resp.rect;
        let full = value.clone();
        resp.on_hover_text(full);
        let picked = if ui.button(browse).clicked() {
            rfd::FileDialog::new().set_title(title).pick_folder().map(|d| d.display().to_string())
        } else {
            None
        };
        (typed, picked, rect)
    })
    .inner
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("oot-mod-merge-drop-test").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_dropped_folder_lands_where_it_belongs() {
        let game = scratch("game");
        std::fs::create_dir_all(game.join("romfs").join("Pack")).unwrap();
        std::fs::write(game.join("romfs").join("Pack").join("ZsDic.pack.zs"), b"x").unwrap();
        std::fs::create_dir_all(game.join("mods")).unwrap();
        let one_mod = scratch("one-mod");
        std::fs::create_dir_all(one_mod.join("romfs").join("Banc")).unwrap();

        // the whole game folder fills both fields
        let mut app = App::default();
        app.mods.clear();
        app.drop_folder(game.clone(), None);
        assert_eq!(app.romfs, game.join("romfs").display().to_string());
        assert_eq!(app.out, game.join("mods").display().to_string());

        // a mod folder joins the list
        app.drop_folder(one_mod.clone(), None);
        assert_eq!(app.mods.len(), 1);
        assert_eq!(app.mods[0].0, "one-mod");
    }
}
