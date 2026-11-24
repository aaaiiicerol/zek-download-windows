//! Interface gráfica (egui) — mesmo visual e mesmas funções da versão Linux.
use crate::downloader::{
    extract_urls, JobView, Mode, Options, State, AUDIO_FORMATS, BROWSERS, QUALITIES,
};
use crate::engine::{Engine, UreqHttp};
use crate::manager::{EngineStatus, Kind, Manager};
use crate::paths;
use crate::settings::Settings;
use crate::{APP_NAME, VERSION};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontId, Layout, Margin, Rect, RichText, Rounding,
    Sense, Stroke,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ paleta
const BG: Color32 = Color32::from_rgb(14, 17, 23);
const HEADER_BG: Color32 = Color32::from_rgb(11, 14, 19);
const CARD: Color32 = Color32::from_rgb(22, 27, 36);
const BORDER: Color32 = Color32::from_rgb(35, 42, 54);
const BORDER2: Color32 = Color32::from_rgb(42, 51, 66);
const BTN: Color32 = Color32::from_rgb(29, 36, 48);
const BTN_HOVER: Color32 = Color32::from_rgb(38, 48, 64);
const TEXT: Color32 = Color32::from_rgb(232, 236, 243);
const DIM: Color32 = Color32::from_rgb(139, 149, 167);
const DIM2: Color32 = Color32::from_rgb(154, 165, 184);
const ACCENT: Color32 = Color32::from_rgb(99, 102, 241);
const GREEN: Color32 = Color32::from_rgb(52, 211, 153);
const GREEN_DIM: Color32 = Color32::from_rgb(34, 110, 86);
const RED: Color32 = Color32::from_rgb(248, 113, 113);
const RED_DIM: Color32 = Color32::from_rgb(120, 52, 58);
const YELLOW: Color32 = Color32::from_rgb(251, 191, 36);
const WHITE: Color32 = Color32::WHITE;

fn quality_label(q: &str) -> &'static str {
    match q {
        "best" => "Melhor disponível",
        "2160" => "4K (2160p)",
        "1440" => "2K (1440p)",
        "1080" => "Full HD (1080p)",
        "720" => "HD (720p)",
        "480" => "480p",
        "360" => "360p",
        _ => "Full HD (1080p)",
    }
}

fn audio_label(f: &str) -> &'static str {
    match f {
        "mp3" => "MP3 — funciona em tudo",
        "m4a" => "M4A (AAC) — ótima qualidade",
        "opus" => "Opus — arquivo menor",
        "flac" => "FLAC — sem perdas (arquivo grande)",
        _ => "MP3 — funciona em tudo",
    }
}

fn browser_label(b: &str) -> &'static str {
    match b {
        "" => "Nenhum (padrão)",
        "firefox" => "Firefox",
        "chrome" => "Google Chrome",
        "chromium" => "Chromium",
        "brave" => "Brave",
        "edge" => "Microsoft Edge",
        "opera" => "Opera",
        "vivaldi" => "Vivaldi",
        _ => "Nenhum (padrão)",
    }
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", head)
    }
}

fn read_clipboard() -> Option<String> {
    arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok())
}

// ------------------------------------------------------------------- tema

fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = BG;
    v.faint_bg_color = CARD;
    v.window_stroke = Stroke::new(1.0, BORDER2);
    v.window_rounding = Rounding::same(14.0);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.bg_fill = BTN;
    v.widgets.inactive.weak_bg_fill = BTN;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER2);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.rounding = Rounding::same(10.0);
    v.widgets.hovered.bg_fill = BTN_HOVER;
    v.widgets.hovered.weak_bg_fill = BTN_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.hovered.rounding = Rounding::same(10.0);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.active.rounding = Rounding::same(10.0);
    v.widgets.open.bg_fill = BTN_HOVER;
    v.widgets.open.weak_bg_fill = BTN_HOVER;
    v.widgets.open.rounding = Rounding::same(10.0);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, WHITE);
    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = vec2(10.0, 10.0);
    style.spacing.button_padding = vec2(14.0, 8.0);
    style.spacing.interact_size.y = 36.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.5));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.5));
    ctx.set_style(style);
}

// -------------------------------------------------------- widgets pequenos

fn primary_button(text: &str) -> impl egui::Widget {
    egui::Button::new(RichText::new(text).strong().color(WHITE))
        .fill(ACCENT)
        .rounding(10.0)
        .min_size(vec2(96.0, 36.0))
}

fn secondary_button(text: &str) -> impl egui::Widget {
    egui::Button::new(RichText::new(text).color(TEXT))
        .fill(BTN)
        .stroke(Stroke::new(1.0, BORDER2))
        .rounding(10.0)
}

fn h1(text: &str) -> RichText {
    RichText::new(text).strong().size(17.0).color(WHITE)
}

fn h2(text: &str) -> RichText {
    RichText::new(text).strong().size(14.5).color(TEXT)
}

fn paint_logo(ui: &egui::Ui, rect: Rect) {
    let p = ui.painter();
    p.rect_filled(rect, 9.0, ACCENT);
    let at = |x: f32, y: f32| pos2(rect.left() + rect.width() * x, rect.top() + rect.height() * y);
    let s = Stroke::new(3.2, WHITE);
    p.line_segment([at(0.30, 0.30), at(0.70, 0.30)], s);
    p.line_segment([at(0.70, 0.30), at(0.30, 0.64)], s);
    p.line_segment([at(0.30, 0.64), at(0.70, 0.64)], s);
    p.line_segment([at(0.28, 0.78), at(0.72, 0.78)], Stroke::new(2.4, WHITE));
}

fn paint_badge(ui: &egui::Ui, rect: Rect, mode: Mode) {
    let p = ui.painter();
    p.rect_filled(rect, 12.0, Color32::from_rgb(35, 42, 58));
    let c = rect.center();
    let col = Color32::from_rgb(154, 167, 255);
    match mode {
        Mode::Video => {
            let pts = vec![
                pos2(c.x - 6.0, c.y - 9.0),
                pos2(c.x - 6.0, c.y + 9.0),
                pos2(c.x + 9.0, c.y),
            ];
            p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
        }
        Mode::Audio => {
            for (i, h) in [8.0f32, 16.0, 11.0].iter().enumerate() {
                let x = c.x - 9.0 + i as f32 * 8.0;
                let r = Rect::from_min_max(pos2(x, c.y + 8.0 - h), pos2(x + 5.0, c.y + 8.0));
                p.rect_filled(r, 2.0, col);
            }
        }
    }
}

fn progress_bar(ui: &mut egui::Ui, frac: f32, color: Color32, indeterminate: bool, time: f64) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, BG);
    if indeterminate {
        let seg = rect.width() * 0.3;
        let t = ((time * 0.9).fract()) as f32;
        let x0 = rect.left() + (rect.width() + seg) * t - seg;
        let left = x0.max(rect.left());
        let right = (x0 + seg).min(rect.right());
        if right > left {
            let r = Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom()));
            p.rect_filled(r, 4.0, color);
        }
    } else {
        let f = frac.clamp(0.0, 1.0);
        if f > 0.0 {
            let r = Rect::from_min_size(rect.min, vec2((rect.width() * f).max(8.0), 8.0));
            p.rect_filled(r, 4.0, color);
        }
    }
}

fn banner(ui: &mut egui::Ui, st: &EngineStatus) {
    let (fill, stroke) = match st.kind {
        Kind::Err => (Color32::from_rgb(58, 28, 32), RED),
        Kind::Ok => (Color32::from_rgb(20, 52, 44), GREEN),
        Kind::Busy => (Color32::from_rgb(28, 32, 64), ACCENT),
    };
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .rounding(12.0)
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if st.kind == Kind::Busy {
                    ui.spinner();
                }
                ui.label(RichText::new(&st.text).strong().color(WHITE));
            });
        });
}

fn step_card(ui: &mut egui::Ui, num: &str, title: &str, text: &str) {
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(14.0)
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 17.0, ACCENT);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                num,
                FontId::proportional(16.0),
                WHITE,
            );
            ui.label(h2(title));
            ui.label(RichText::new(text).color(DIM2));
        });
}

// ------------------------------------------------------------------ ações

enum Action {
    Download,
    Paste,
    PickFolder,
    OpenFolder,
    Cancel(u64),
    Retry(u64),
    OpenFile(PathBuf),
    Reveal(PathBuf),
    Details(u64),
    ClearFinished,
    OpenSettings,
    UpdateEngine,
    About,
    Quit,
}

struct Draft {
    folder: String,
    cookies: String,
    auto_update: bool,
    channel: String,
}

pub struct App {
    mgr: Arc<Manager>,
    settings: Arc<Mutex<Settings>>,
    url: String,
    mode: Mode,
    quality: String,
    audio_format: String,
    playlist: bool,
    folder: String,
    status: String,
    show_settings: bool,
    show_about: bool,
    details: Option<u64>,
    confirm_exit: bool,
    allow_close: bool,
    draft: Draft,
    was_focused: bool,
    last_clip: String,
    prev_states: HashMap<u64, State>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        apply_theme(&cc.egui_ctx);
        let loaded = Settings::load();
        let (mode, quality, audio_format, playlist, folder, channel) = (
            loaded.mode,
            loaded.quality.clone(),
            loaded.audio_format.clone(),
            loaded.playlist,
            loaded.folder.clone(),
            loaded.channel.clone(),
        );
        let settings = Arc::new(Mutex::new(loaded));
        let engine = Arc::new(Engine::new(&paths::data_dir(), &channel, Box::new(UreqHttp::new())));
        let ctx = cc.egui_ctx.clone();
        let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        let mgr = Manager::new(Arc::clone(&settings), engine, repaint);
        mgr.start();
        App {
            mgr,
            settings,
            url: String::new(),
            mode,
            quality,
            audio_format,
            playlist,
            folder: folder.clone(),
            status: "Pronto. Cole um link acima para começar.".to_string(),
            show_settings: false,
            show_about: false,
            details: None,
            confirm_exit: false,
            allow_close: false,
            draft: Draft {
                folder,
                cookies: String::new(),
                auto_update: true,
                channel,
            },
            was_focused: false,
            last_clip: String::new(),
            prev_states: HashMap::new(),
        }
    }

    fn save_prefs(&self) {
        let mut s = self.settings.lock().unwrap();
        s.mode = self.mode;
        s.quality = self.quality.clone();
        s.audio_format = self.audio_format.clone();
        s.playlist = self.playlist;
        s.folder = self.folder.clone();
        s.save();
    }

    fn hint(&self) -> String {
        let mut t = match self.mode {
            Mode::Video => {
                let q = if self.quality == "best" {
                    "na melhor qualidade disponível".to_string()
                } else {
                    format!("em até {}", quality_label(&self.quality))
                };
                format!("Vídeo {}, com som. Salvo em MP4 quando o site permite (ou MKV).", q)
            }
            Mode::Audio => format!(
                "Somente o áudio, em {}, na melhor qualidade. Inclui capa e dados da música quando o site oferece.",
                self.audio_format.to_uppercase()
            ),
        };
        if self.playlist {
            t.push_str(" Se o link for de uma playlist, todos os itens entram na fila (numerados, em uma pasta).");
        } else {
            t.push_str(" Se o link for de uma playlist, só o item indicado será baixado.");
        }
        t
    }

    fn find_job(&self, id: u64) -> Option<Arc<crate::downloader::Job>> {
        self.mgr.jobs.lock().unwrap().iter().find(|j| j.id == id).cloned()
    }

    // ---------------------------------------------------------- comportamento
    fn start_download(&mut self) {
        let (urls, errors) = extract_urls(&self.url);
        if urls.is_empty() {
            let msg = errors.first().cloned().unwrap_or_else(|| "Cole um link para começar.".into());
            self.status = format!("Atenção: {}", msg);
            return;
        }
        let cookies = self.settings.lock().unwrap().cookies_browser.clone();
        let opts = Options {
            mode: self.mode,
            quality: self.quality.clone(),
            audio_format: self.audio_format.clone(),
            playlist: self.playlist,
            folder: PathBuf::from(&self.folder),
            cookies_browser: cookies,
        };
        if std::fs::create_dir_all(&opts.folder).is_err() {
            self.status = "Atenção: não consegui criar/usar a pasta de destino. Escolha outra pasta.".into();
            return;
        }
        for u in &urls {
            self.mgr.add(u.clone(), opts.clone());
        }
        self.url.clear();
        let mut msg = format!("{} link(s) adicionado(s) à fila.", urls.len());
        if !errors.is_empty() {
            msg.push_str(&format!(" {} item(ns) inválido(s) ignorado(s).", errors.len()));
        }
        self.status = msg;
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        let requested = ctx.input(|i| i.viewport().close_requested());
        if requested && !self.allow_close && self.mgr.has_active() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_exit = true;
        }
    }

    fn check_focus(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if focused && !self.was_focused && self.url.is_empty() {
            if let Some(t) = read_clipboard() {
                let t = t.trim().to_string();
                let lower = t.to_lowercase();
                if (lower.starts_with("http://") || lower.starts_with("https://"))
                    && t != self.last_clip
                    && !extract_urls(&t).0.is_empty()
                {
                    self.url = t.clone();
                    self.last_clip = t;
                    self.status =
                        "Detectei um link na área de transferência e já colei para você. Clique em Baixar.".into();
                }
            }
        }
        self.was_focused = focused;
    }

    fn track_transitions(&mut self, ctx: &egui::Context, views: &[JobView]) {
        let unfocused = ctx.input(|i| i.viewport().focused) == Some(false);
        for v in views {
            let prev = self.prev_states.insert(v.id, v.state);
            if prev == Some(v.state) {
                continue;
            }
            let name = if v.title.is_empty() { v.url.clone() } else { v.title.clone() };
            match v.state {
                State::Done => {
                    self.status = format!("Concluído: {}", shorten(&name, 80));
                    if unfocused {
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                            egui::UserAttentionType::Informational,
                        ));
                    }
                }
                State::Error => {
                    self.status = format!("Falhou: {} — veja a mensagem no item.", shorten(&name, 60));
                    if unfocused {
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                            egui::UserAttentionType::Informational,
                        ));
                    }
                }
                State::Cancelled => self.status = "Download cancelado.".into(),
                State::Downloading if prev.is_none() || prev == Some(State::Queued) || prev == Some(State::Preparing) => {
                    self.status = "Baixando… acompanhe o progresso abaixo.".into();
                }
                _ => {}
            }
        }
    }

    fn apply_actions(&mut self, ctx: &egui::Context, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Download => self.start_download(),
                Action::Paste => match read_clipboard() {
                    Some(t) if !t.trim().is_empty() => {
                        self.url = t.trim().to_string();
                        self.status = "Link colado. Escolha o formato e clique em Baixar.".into();
                    }
                    _ => self.status = "A área de transferência está vazia. Copie um link primeiro.".into(),
                },
                Action::PickFolder => {
                    let start = PathBuf::from(&self.folder);
                    let mut dlg = rfd::FileDialog::new().set_title("Escolha a pasta de downloads");
                    if start.is_dir() {
                        dlg = dlg.set_directory(&start);
                    }
                    if let Some(p) = dlg.pick_folder() {
                        self.folder = p.to_string_lossy().to_string();
                        self.save_prefs();
                        self.status = format!("Os próximos downloads serão salvos em: {}", self.folder);
                    }
                }
                Action::OpenFolder => {
                    let p = PathBuf::from(&self.folder);
                    let _ = std::fs::create_dir_all(&p);
                    paths::open_path(&p);
                }
                Action::Cancel(id) => {
                    if let Some(j) = self.find_job(id) {
                        self.mgr.cancel(&j);
                    }
                }
                Action::Retry(id) => {
                    if let Some(j) = self.find_job(id) {
                        self.mgr.retry(&j);
                    }
                }
                Action::OpenFile(p) => {
                    if p.is_file() {
                        paths::open_path(&p);
                    }
                }
                Action::Reveal(p) => {
                    if p.is_file() {
                        paths::reveal_in_folder(&p);
                    } else if let Some(parent) = p.parent() {
                        paths::open_path(parent);
                    }
                }
                Action::Details(id) => self.details = Some(id),
                Action::ClearFinished => {
                    self.mgr.clear_finished();
                    self.prev_states.clear();
                }
                Action::OpenSettings => {
                    let s = self.settings.lock().unwrap();
                    self.draft = Draft {
                        folder: self.folder.clone(),
                        cookies: s.cookies_browser.clone(),
                        auto_update: s.auto_update,
                        channel: s.channel.clone(),
                    };
                    drop(s);
                    self.show_settings = true;
                }
                Action::UpdateEngine => self.mgr.update_now(),
                Action::About => self.show_about = true,
                Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    // ---------------------------------------------------------------- telas
    fn header_ui(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            paint_logo(ui, rect);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(APP_NAME).strong().size(17.0).color(WHITE));
                ui.label(RichText::new("Vídeos e músicas, de qualquer site").size(11.5).color(DIM));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("Menu", |ui| {
                    if ui.button("Configurações").clicked() {
                        actions.push(Action::OpenSettings);
                        ui.close_menu();
                    }
                    if ui.button("Verificar atualização do motor").clicked() {
                        actions.push(Action::UpdateEngine);
                        ui.close_menu();
                    }
                    if ui.button("Sobre o Zek Download").clicked() {
                        actions.push(Action::About);
                        ui.close_menu();
                    }
                    if ui.button("Sair").clicked() {
                        actions.push(Action::Quit);
                        ui.close_menu();
                    }
                });
            });
        });
    }

    fn footer_ui(&mut self, ui: &mut egui::Ui, st: &EngineStatus) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(shorten(&self.status, 95)).size(12.0).color(DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (text, color) = match st.kind {
                    Kind::Busy => ("yt-dlp: atualizando…".to_string(), YELLOW),
                    Kind::Ok => (
                        match self.mgr.engine.cached_version() {
                            Some(v) => format!("yt-dlp {}", v),
                            None => "yt-dlp pronto".to_string(),
                        },
                        GREEN,
                    ),
                    Kind::Err => ("yt-dlp: sem atualização".to_string(), RED),
                };
                ui.label(RichText::new(text).size(12.0).color(DIM));
                let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.5, color);
            });
        });
    }

    fn card_ui(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::Frame::none()
            .fill(CARD)
            .stroke(Stroke::new(1.0, BORDER))
            .rounding(16.0)
            .inner_margin(Margin::same(20.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(h1("Novo download"));
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 230.0).max(160.0);
                    let edit = egui::TextEdit::singleline(&mut self.url)
                        .hint_text("Cole aqui o link do vídeo ou da música (YouTube, SoundCloud, Vimeo, X…)")
                        .desired_width(w);
                    let resp = ui.add(edit);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        actions.push(Action::Download);
                    }
                    if ui.add(secondary_button("Colar")).clicked() {
                        actions.push(Action::Paste);
                    }
                    if ui.add(primary_button("Baixar")).clicked() {
                        actions.push(Action::Download);
                    }
                });
                let before = (self.mode, self.quality.clone(), self.audio_format.clone(), self.playlist);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (m, label) in [(Mode::Video, "Vídeo"), (Mode::Audio, "Áudio")] {
                        let selected = self.mode == m;
                        let (fill, color) = if selected { (ACCENT, WHITE) } else { (BG, DIM) };
                        let r = if m == Mode::Video {
                            Rounding { nw: 10.0, sw: 10.0, ne: 0.0, se: 0.0 }
                        } else {
                            Rounding { nw: 0.0, sw: 0.0, ne: 10.0, se: 10.0 }
                        };
                        let b = egui::Button::new(RichText::new(label).strong().color(color))
                            .fill(fill)
                            .stroke(Stroke::new(1.0, if selected { ACCENT } else { BORDER2 }))
                            .rounding(r)
                            .min_size(vec2(92.0, 36.0));
                        if ui.add(b).clicked() {
                            self.mode = m;
                        }
                    }
                    ui.spacing_mut().item_spacing.x = 14.0;
                    ui.add_space(14.0);
                    match self.mode {
                        Mode::Video => {
                            egui::ComboBox::new("q_combo", "")
                                .selected_text(quality_label(&self.quality))
                                .width(190.0)
                                .show_ui(ui, |ui| {
                                    for q in QUALITIES.iter() {
                                        ui.selectable_value(&mut self.quality, q.to_string(), quality_label(q));
                                    }
                                });
                        }
                        Mode::Audio => {
                            egui::ComboBox::new("a_combo", "")
                                .selected_text(audio_label(&self.audio_format))
                                .width(250.0)
                                .show_ui(ui, |ui| {
                                    for f in AUDIO_FORMATS.iter() {
                                        ui.selectable_value(&mut self.audio_format, f.to_string(), audio_label(f));
                                    }
                                });
                        }
                    }
                    ui.checkbox(&mut self.playlist, "Baixar a playlist inteira");
                });
                if before != (self.mode, self.quality.clone(), self.audio_format.clone(), self.playlist) {
                    self.save_prefs();
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Salvar em").color(DIM));
                    ui.label(RichText::new(shorten(&self.folder, 64)).color(TEXT))
                        .on_hover_text(self.folder.clone());
                    if ui.add(secondary_button("Alterar pasta…")).clicked() {
                        actions.push(Action::PickFolder);
                    }
                    if ui.add(secondary_button("Abrir pasta")).clicked() {
                        actions.push(Action::OpenFolder);
                    }
                });
                ui.label(RichText::new(self.hint()).size(12.0).color(DIM));
            });
    }

    fn onboarding_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(h1("Como funciona"));
        ui.columns(3, |cols| {
            step_card(&mut cols[0], "1", "Cole o link", "Copie o endereço do vídeo ou da música em qualquer site e cole no campo acima.");
            step_card(&mut cols[1], "2", "Escolha o formato", "Vídeo na qualidade que quiser, ou só o áudio em MP3, M4A, Opus ou FLAC.");
            step_card(&mut cols[2], "3", "Clique em Baixar", "O progresso aparece aqui. Quando acabar, abra o arquivo ou a pasta com um clique.");
        });
        ui.label(
            RichText::new(
                "Dica: cole vários links de uma vez (separados por espaço ou linha) e acompanhe tudo na fila. \
                 O Zek Download atualiza sozinho o motor de download (yt-dlp), então continua funcionando quando os sites mudam.",
            )
            .size(12.0)
            .color(DIM),
        );
    }

    fn list_ui(&mut self, ui: &mut egui::Ui, views: &[JobView], actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.label(h1("Downloads"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(secondary_button("Limpar finalizados")).clicked() {
                    actions.push(Action::ClearFinished);
                }
            });
        });
        let time = ui.input(|i| i.time);
        for v in views.iter().rev() {
            job_row(ui, v, actions, time);
        }
    }

    fn main_ui(&mut self, ui: &mut egui::Ui, views: &[JobView], st: &EngineStatus, actions: &mut Vec<Action>) {
        let show_banner = st.kind == Kind::Busy
            || st.banner_until.map(|t| Instant::now() < t).unwrap_or(false);
        if show_banner {
            banner(ui, st);
        }
        self.card_ui(ui, actions);
        if views.is_empty() {
            self.onboarding_ui(ui);
        } else {
            self.list_ui(ui, views, actions);
        }
    }

    // ------------------------------------------------------------- janelas
    fn windows(&mut self, ctx: &egui::Context, views: &[JobView]) {
        self.settings_window(ctx);

        if self.show_about {
            let mut open = true;
            let mut close = false;
            egui::Window::new("Sobre o Zek Download")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.label(h1(&format!("{} {}", APP_NAME, VERSION)));
                    ui.label("Baixe vídeos e músicas de vários sites.");
                    ui.label(
                        RichText::new(
                            "Usa o yt-dlp, que o Zek Download mantém atualizado sozinho (com verificação SHA-256).",
                        )
                        .color(DIM),
                    );
                    ui.label(RichText::new("Licença MIT — github.com/aaaiiicerol").color(DIM));
                    if paths::is_portable() {
                        ui.label(RichText::new("Modo portátil: tudo fica na pasta “data” ao lado do programa.").color(DIM));
                    }
                    if ui.add(primary_button("Fechar")).clicked() {
                        close = true;
                    }
                });
            if !open || close {
                self.show_about = false;
            }
        }

        if let Some(id) = self.details {
            let mut open = true;
            let mut close = false;
            if let Some(v) = views.iter().find(|v| v.id == id) {
                let body = if v.log.trim().is_empty() {
                    "(sem detalhes adicionais)".to_string()
                } else {
                    v.log.clone()
                };
                let full = format!("Link: {}\n\n{}", v.url, body);
                let mut shown = full.clone();
                egui::Window::new("Detalhes técnicos")
                    .open(&mut open)
                    .collapsible(false)
                    .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.set_width(560.0);
                        ui.label(RichText::new("Mensagem do yt-dlp (útil para pedir ajuda ou reportar um problema):").color(DIM));
                        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut shown)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(12),
                            );
                        });
                        ui.horizontal(|ui| {
                            if ui.add(secondary_button("Copiar")).clicked() {
                                let text = full.clone();
                                ui.output_mut(|o| o.copied_text = text);
                            }
                            if ui.add(primary_button("Fechar")).clicked() {
                                close = true;
                            }
                        });
                    });
            } else {
                close = true;
            }
            if !open || close {
                self.details = None;
            }
        }

        if self.confirm_exit {
            let mut stay = false;
            let mut leave = false;
            egui::Window::new("Há downloads em andamento")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.label("Se você sair agora, eles serão cancelados. Depois, ao tentar de novo, o download continua de onde parou.");
                    ui.horizontal(|ui| {
                        if ui.add(primary_button("Continuar baixando")).clicked() {
                            stay = true;
                        }
                        if ui.add(secondary_button("Sair e cancelar")).clicked() {
                            leave = true;
                        }
                    });
                });
            if stay {
                self.confirm_exit = false;
            }
            if leave {
                self.mgr.cancel_all();
                self.allow_close = true;
                self.confirm_exit = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("Configurações")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(470.0);
                ui.label(h2("Pasta de downloads"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.draft.folder).desired_width(320.0));
                    if ui.add(secondary_button("Escolher…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.draft.folder = p.to_string_lossy().to_string();
                        }
                    }
                });
                ui.label(RichText::new("Onde os vídeos e músicas serão salvos.").size(12.0).color(DIM));

                ui.label(h2("Usar login do navegador"));
                egui::ComboBox::new("cookies_combo", "")
                    .selected_text(browser_label(&self.draft.cookies))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for b in BROWSERS.iter() {
                            ui.selectable_value(&mut self.draft.cookies, b.to_string(), browser_label(b));
                        }
                    });
                ui.label(
                    RichText::new("Só se o site pedir login ou confirmação de idade. Os cookies são lidos apenas durante o download e não são guardados.")
                        .size(12.0)
                        .color(DIM),
                );

                ui.checkbox(&mut self.draft.auto_update, "Atualizar o motor sozinho");
                ui.label(
                    RichText::new("Confere a cada poucas horas se há versão nova do yt-dlp. Recomendado: é o que mantém o app funcionando quando os sites mudam.")
                        .size(12.0)
                        .color(DIM),
                );

                ui.label(h2("Canal de atualização"));
                egui::ComboBox::new("channel_combo", "")
                    .selected_text(if self.draft.channel == "nightly" {
                        "Nightly — correções chegam primeiro"
                    } else {
                        "Estável (recomendado)"
                    })
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.draft.channel, "stable".to_string(), "Estável (recomendado)");
                        ui.selectable_value(&mut self.draft.channel, "nightly".to_string(), "Nightly — correções chegam primeiro");
                    });
                ui.label(
                    RichText::new("O canal Nightly recebe as correções dias antes, mas pode ter mais imprevistos.")
                        .size(12.0)
                        .color(DIM),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(primary_button("Salvar")).clicked() {
                        save = true;
                    }
                    if ui.add(secondary_button("Cancelar")).clicked() {
                        cancel = true;
                    }
                });
            });
        if save {
            {
                let mut s = self.settings.lock().unwrap();
                s.folder = self.draft.folder.clone();
                s.cookies_browser = self.draft.cookies.clone();
                s.auto_update = self.draft.auto_update;
                s.channel = self.draft.channel.clone();
                s.sanitize();
                s.save();
                self.folder = s.folder.clone();
            }
            self.mgr.engine.set_channel(&self.draft.channel);
            self.status = "Configurações salvas.".into();
            self.show_settings = false;
        } else if cancel || !open {
            self.show_settings = false;
        }
    }
}

fn job_row(ui: &mut egui::Ui, v: &JobView, actions: &mut Vec<Action>, time: f64) {
    let (border, bar_color, text_color) = match v.state {
        State::Done => (GREEN_DIM, GREEN, GREEN),
        State::Error => (RED_DIM, RED, RED),
        _ => (BORDER, ACCENT, DIM2),
    };
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0, border))
        .rounding(14.0)
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(46.0, 46.0), Sense::hover());
                paint_badge(ui, rect, v.mode);
                let left_w = (ui.available_width() - 240.0).max(220.0);
                ui.vertical(|ui| {
                    ui.set_width(left_w);
                    let title = if v.title.is_empty() { v.url.clone() } else { v.title.clone() };
                    ui.label(RichText::new(shorten(&title, 80)).strong().size(15.0).color(WHITE))
                        .on_hover_text(title.clone());
                    let indeterminate = matches!(v.state, State::Preparing | State::Processing);
                    let frac = match v.state {
                        State::Queued => 0.0,
                        State::Done => 1.0,
                        _ => (v.percent / 100.0) as f32,
                    };
                    progress_bar(ui, frac, bar_color, indeterminate, time);
                    ui.label(RichText::new(v.status.clone()).size(12.5).color(text_color));
                });
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| match v.state {
                        State::Done => {
                            let file = PathBuf::from(&v.filepath);
                            if ui.add(secondary_button("Abrir")).clicked() {
                                actions.push(Action::OpenFile(file.clone()));
                            }
                            if ui.add(secondary_button("Pasta")).clicked() {
                                if v.filepath.is_empty() {
                                    actions.push(Action::Reveal(v.folder.join("x")));
                                } else {
                                    actions.push(Action::Reveal(file));
                                }
                            }
                        }
                        State::Error => {
                            if ui.add(secondary_button("Tentar de novo")).clicked() {
                                actions.push(Action::Retry(v.id));
                            }
                            if ui.add(secondary_button("Detalhes")).clicked() {
                                actions.push(Action::Details(v.id));
                            }
                        }
                        State::Cancelled => {
                            if ui.add(secondary_button("Tentar de novo")).clicked() {
                                actions.push(Action::Retry(v.id));
                            }
                        }
                        _ => {
                            if ui.add(secondary_button("Cancelar")).clicked() {
                                actions.push(Action::Cancel(v.id));
                            }
                        }
                    });
                });
            });
        });
    ui.add_space(2.0);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_close(ctx);
        self.check_focus(ctx);

        let jobs: Vec<Arc<crate::downloader::Job>> = self.mgr.jobs.lock().unwrap().clone();
        let views: Vec<JobView> = jobs.iter().map(|j| j.view()).collect();
        self.track_transitions(ctx, &views);
        let st = self.mgr.status_snapshot();
        let mut actions: Vec<Action> = Vec::new();

        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(HEADER_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(Margin::symmetric(18.0, 10.0)),
            )
            .show(ctx, |ui| self.header_ui(ui, &mut actions));

        egui::TopBottomPanel::bottom("footer")
            .frame(
                egui::Frame::none()
                    .fill(HEADER_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(Margin::symmetric(18.0, 8.0)),
            )
            .show(ctx, |ui| self.footer_ui(ui, &st));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(Margin::same(22.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.main_ui(ui, &views, &st, &mut actions);
                    });
            });

        self.windows(ctx, &views);
        self.apply_actions(ctx, actions);

        let busy = views.iter().any(|v| v.state.is_active())
            || st.kind == Kind::Busy
            || st.banner_until.map(|t| Instant::now() < t).unwrap_or(false);
        if busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
