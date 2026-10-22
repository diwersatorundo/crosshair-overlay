use crate::autostart;
use crate::logging;
use crate::shared::{Shared, Snapshot};
use crosshair_core::config::{self, Config, Rgba};
use crosshair_core::geometry::{build_scene, Shape as CrosshairShape};
use crosshair_core::sharecode;
use eframe::egui::{
    self, color_picker, vec2, Align, Align2, Button, Color32, ComboBox, CornerRadius, DragValue,
    FontId, Frame, Layout, Margin, Panel, RichText, ScrollArea, Sense, Stroke, StrokeKind,
    TextEdit, TextStyle, Ui, ViewportCommand,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

const SIDEBAR_WIDTH: f32 = 236.0;

const BG: Color32 = Color32::from_rgb(10, 10, 10);
const SIDEBAR: Color32 = Color32::from_rgb(10, 10, 10);
const SURFACE: Color32 = Color32::from_rgb(23, 23, 23);
const HOVER: Color32 = Color32::from_rgb(32, 32, 32);
const BORDER: Color32 = Color32::from_rgb(45, 45, 45);
const BORDER_HOVER: Color32 = Color32::from_rgb(78, 78, 78);
const TEXT: Color32 = Color32::from_rgb(237, 237, 237);
const MUTED: Color32 = Color32::from_rgb(160, 160, 160);
const DIM: Color32 = Color32::from_rgb(110, 110, 110);
const ACCENT: Color32 = Color32::from_rgb(240, 240, 240);
const ACCENT_DARK: Color32 = Color32::from_rgb(38, 38, 38);
const ERROR: Color32 = Color32::from_rgb(235, 235, 235);
const PREVIEW_BG: Color32 = Color32::from_rgb(17, 17, 17);

const STYLE_NAMES: [&str; 10] = [
    "Крест (динамический)",
    "Круг (динамический)",
    "Крест (legacy)",
    "Круг (статический)",
    "Крест (статический)",
    "Крест (shot feedback)",
    "Только точка",
    "Диагональ (динамическая)",
    "Квадрат",
    "Диагональ (статическая)",
];

const STYLE_DESCRIPTIONS: [&str; 10] = [
    "Классический крест с динамическим разбросом. В оверлее отображается статично.",
    "Круг с динамическим радиусом. В оверлее отображается статично.",
    "Крест старого формата CS.",
    "Статичный круг.",
    "Статичный крест — базовая форма CS2.",
    "Крест, который расширяется при выстреле. В оверлее отображается статично.",
    "Только центральная точка. Размер точки задаётся толщиной.",
    "Четыре диагональных луча.",
    "Квадратная рамка.",
    "Статичный вариант диагонального прицела.",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Settings,
    About,
}

impl Tab {
    fn title(self) -> &'static str {
        match self {
            Tab::Settings => "Настройки",
            Tab::About => "О программе",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Tab::Settings => "Прицел, поведение оверлея и share code",
            Tab::About => "Версия, ограничения и файлы",
        }
    }
}

struct Notice {
    text: String,
    error: bool,
    until: Instant,
}

impl Notice {
    fn new(text: impl Into<String>, error: bool, ttl: Duration) -> Self {
        Self {
            text: text.into(),
            error,
            until: Instant::now() + ttl,
        }
    }
}

pub struct CrosshairApp {
    shared: Arc<Shared>,
    cfg: Config,
    seen_version: u64,
    tab: Tab,
    import_text: String,
    notice: Option<Notice>,
    dirty_since: Option<Instant>,
    minimize_requested: bool,
    autostart: bool,
    autostart_error: Option<String>,
    tray: Option<TrayIcon>,
    quitting: bool,
}

#[derive(Clone, Copy)]
enum TrayAction {
    Show,
    Quit,
}

static TRAY_ACTIONS: Mutex<Vec<TrayAction>> = Mutex::new(Vec::new());

fn push_tray_action(ctx: &egui::Context, action: TrayAction) {
    let mut pending = TRAY_ACTIONS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    pending.push(action);
    drop(pending);
    ctx.request_repaint();
}

fn create_tray(ctx: &egui::Context) -> Option<TrayIcon> {
    let result = (|| -> Result<TrayIcon, String> {
        let menu = Menu::new();
        let show = MenuItem::with_id("show", "Открыть настройки", true, None);
        let exit = MenuItem::with_id("exit", "Выход", true, None);
        let separator = PredefinedMenuItem::separator();
        menu.append_items(&[&show, &separator, &exit])
            .map_err(|error| error.to_string())?;

        let show_id = show.id().clone();
        let exit_id = exit.id().clone();
        let wake_ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == show_id {
                push_tray_action(&wake_ctx, TrayAction::Show);
            } else if event.id == exit_id {
                push_tray_action(&wake_ctx, TrayAction::Quit);
            }
        }));

        let wake_ctx = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
            {
                push_tray_action(&wake_ctx, TrayAction::Show);
            }
        }));

        let icon_data = app_icon();
        let icon = tray_icon::Icon::from_rgba(icon_data.rgba, icon_data.width, icon_data.height)
            .map_err(|error| error.to_string())?;
        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Crosshair Overlay")
            .with_icon(icon)
            .build()
            .map_err(|error| error.to_string())
    })();

    match result {
        Ok(tray) => Some(tray),
        Err(error) => {
            logging::log("WARN", format!("Не удалось создать значок в трее: {error}"));
            None
        }
    }
}

pub fn app_icon() -> egui::IconData {
    let size = 64usize;
    let mut rgba = vec![0u8; size * size * 4];
    let center = (size as f32 - 1.0) * 0.5;
    let arm = |dx: f32, dy: f32, half_width: f32, gap: f32, length: f32| {
        (dx <= half_width && dy >= gap && dy <= length)
            || (dy <= half_width && dx >= gap && dx <= length)
    };
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - center).abs();
            let dy = (y as f32 - center).abs();
            let dot_outer = dx * dx + dy * dy <= 8.0 * 8.0;
            let dot_inner = dx * dx + dy * dy <= 5.5 * 5.5;
            let outer = arm(dx, dy, 4.2, 5.0, 26.0) || dot_outer;
            let inner = arm(dx, dy, 2.2, 7.5, 24.0) || dot_inner;
            let index = (y * size + x) * 4;
            if inner {
                rgba[index] = 232;
                rgba[index + 1] = 154;
                rgba[index + 2] = 65;
                rgba[index + 3] = 255;
            } else if outer {
                rgba[index] = 9;
                rgba[index + 1] = 10;
                rgba[index + 2] = 12;
                rgba[index + 3] = 255;
            }
        }
    }
    egui::IconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}

impl CrosshairApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shared: Arc<Shared>,
        config: Config,
        load_warning: Option<String>,
    ) -> Self {
        configure_theme(&cc.egui_ctx);
        let seen_version = shared.snapshot().version;
        let notice = load_warning.map(|warning| {
            logging::log("WARN", &warning);
            Notice::new(warning, true, Duration::from_secs(12))
        });
        let (autostart, autostart_error) = match autostart::is_enabled() {
            Ok(enabled) => (enabled, None),
            Err(error) => {
                logging::log("ERROR", format!("Автозапуск: {error}"));
                (false, Some(error))
            }
        };
        let tray = create_tray(&cc.egui_ctx);
        Self {
            shared,
            cfg: config,
            seen_version,
            tab: Tab::Settings,
            import_text: String::new(),
            notice,
            dirty_since: None,
            minimize_requested: false,
            autostart,
            autostart_error,
            tray,
            quitting: false,
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        let actions = {
            let mut pending = TRAY_ACTIONS
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            std::mem::take(&mut *pending)
        };
        for action in actions {
            match action {
                TrayAction::Show => self.show_window(ctx),
                TrayAction::Quit => self.quit(ctx),
            }
        }
    }

    fn show_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        self.force_save();
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.quitting || self.tray.is_none() {
            self.force_save();
            return;
        }
        self.force_save();
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        logging::log("INFO", "Окно свёрнуто в трей");
    }

    fn sync(&mut self) -> Snapshot {
        let snapshot = self.shared.snapshot();
        if snapshot.version != self.seen_version {
            if self.dirty_since.is_some() {
                self.cfg.enabled = snapshot.config.enabled;
                self.seen_version = self.shared.set_config(self.cfg.clone());
            } else {
                self.cfg = snapshot.config.clone();
                self.seen_version = snapshot.version;
            }
        }
        snapshot
    }

    fn commit_if_changed(&mut self, before: &Config) {
        if self.cfg != *before {
            self.cfg = self.cfg.clone().sanitized();
            self.seen_version = self.shared.set_config(self.cfg.clone());
            self.dirty_since = Some(Instant::now());
        }
    }

    fn flush_save(&mut self) {
        let Some(since) = self.dirty_since else {
            return;
        };
        if since.elapsed() < Duration::from_millis(350) {
            return;
        }
        self.dirty_since = None;
        if let Err(err) = self.shared.save(&self.cfg) {
            self.set_notice(err, true, Duration::from_secs(8));
        }
    }

    fn force_save(&mut self) {
        self.seen_version = self.shared.set_config(self.cfg.clone());
        self.dirty_since = None;
        if let Err(err) = self.shared.save(&self.cfg) {
            logging::log("ERROR", &err);
        }
    }

    fn set_notice(&mut self, text: impl Into<String>, error: bool, ttl: Duration) {
        self.notice = Some(Notice::new(text, error, ttl));
    }

    fn sidebar(&mut self, ui: &mut Ui, snapshot: &Snapshot) {
        ui.set_width(ui.available_width());
        ui.label(
            RichText::new("Crosshair Overlay")
                .size(14.5)
                .strong()
                .color(TEXT),
        );
        ui.add_space(3.0);
        ui.label(
            RichText::new(format!("Версия {}", env!("CARGO_PKG_VERSION")))
                .size(11.5)
                .color(DIM),
        );
        ui.label(RichText::new("Вайб-код проект").size(11.5).color(DIM));
        ui.add_space(24.0);

        for tab in [Tab::Settings, Tab::About] {
            if nav_item(ui, self.tab == tab, tab.title()).clicked() {
                self.tab = tab;
            }
            ui.add_space(2.0);
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.label(
                RichText::new("Закрытие окна сворачивает в трей")
                    .size(10.5)
                    .color(DIM),
            );
            ui.add_space(8.0);
            if secondary_button_sized(ui, "Выход", 32.0).clicked() {
                self.quit(ui.ctx());
            }
            ui.add_space(8.0);
            if primary_button_sized(ui, "Свернуть в игру", 36.0).clicked() {
                self.minimize_requested = true;
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new("Настройки сохраняются автоматически")
                    .size(10.5)
                    .color(DIM),
            );
            ui.add_space(16.0);
            Frame::new().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                status_plain(ui, snapshot, self.cfg.enabled);
            });
            ui.add_space(12.0);
            ui.separator();
        });
    }

    fn header(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(self.tab.title())
                        .size(25.0)
                        .strong()
                        .color(TEXT),
                );
                ui.add_space(2.0);
                ui.label(RichText::new(self.tab.subtitle()).size(12.5).color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.checkbox(
                    &mut self.cfg.enabled,
                    RichText::new("Прицел включён").size(12.5).color(MUTED),
                )
                .on_hover_text("Переключить прицел. Горячая клавиша: Ctrl+Alt+X");
            });
        });
    }

    fn main_content(&mut self, ui: &mut Ui, snapshot: &Snapshot, ctx: &egui::Context) {
        self.header(ui);
        ui.add_space(12.0);

        if let Some(error) = &snapshot.hotkey_error {
            plain_notice(
                ui,
                format!("Ctrl+Alt+X недоступна: {error}. Переключение из настроек работает."),
                true,
            );
        }
        if let Some(error) = &snapshot.overlay_error {
            plain_notice(
                ui,
                format!("Ошибка оверлея: {error}. Поток повторит запуск автоматически."),
                true,
            );
        }
        if self
            .notice
            .as_ref()
            .is_some_and(|notice| Instant::now() >= notice.until)
        {
            self.notice = None;
        }
        if let Some(notice) = &self.notice {
            plain_notice(ui, notice.text.clone(), notice.error);
        }

        if self.tab == Tab::Settings {
            let preview_height = (ui.available_height() * 0.27).clamp(150.0, 200.0);
            self.preview(ui, preview_height);
            ui.add_space(5.0);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Превью в масштабе ×2; в игре размер зависит от высоты активного окна.",
                    )
                    .size(11.0)
                    .color(DIM),
                )
                .wrap(),
            );
            ui.add_space(8.0);
        }

        ScrollArea::vertical()
            .id_salt(("settings_scroll", self.tab as u8))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match self.tab {
                    Tab::Settings => self.settings_page(ui, ctx),
                    Tab::About => self.about_page(ui),
                }
                ui.add_space(16.0);
            });
    }

    fn preview(&self, ui: &mut Ui, height: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(5), PREVIEW_BG);

        let center = rect.center();
        let available = (rect.height() * 0.5 - 30.0).max(20.0);
        let mut zoom = 2.0f32;
        let mut scene = build_scene(&self.cfg, self.cfg.screen_height as f32, zoom);
        if scene.extent > available {
            zoom = (zoom * available / scene.extent).clamp(0.02, 12.0);
            scene = build_scene(&self.cfg, self.cfg.screen_height as f32, zoom);
        }

        for shape in &scene.shapes {
            match shape {
                CrosshairShape::Fill { polygons, color } => {
                    let color = to_color32(*color);
                    for polygon in polygons {
                        let points: Vec<egui::Pos2> = polygon
                            .iter()
                            .map(|point| center + vec2(point.x, point.y))
                            .collect();
                        if points.len() >= 3 {
                            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
                        }
                    }
                }
                CrosshairShape::Ring {
                    center: shape_center,
                    radius,
                    width,
                    color,
                } => {
                    let ring_center = center + vec2(shape_center.x, shape_center.y);
                    painter.circle_stroke(
                        ring_center,
                        *radius,
                        Stroke::new(width.max(1.0), to_color32(*color)),
                    );
                }
            }
        }

        painter.text(
            egui::pos2(rect.left() + 12.0, rect.top() + 10.0),
            Align2::LEFT_TOP,
            "ПРЕВЬЮ",
            FontId::proportional(11.0),
            Color32::from_rgba_unmultiplied(255, 255, 255, 95),
        );
        painter.text(
            egui::pos2(rect.right() - 12.0, rect.top() + 10.0),
            Align2::RIGHT_TOP,
            format!("×{zoom:.2}"),
            FontId::monospace(10.0),
            Color32::from_rgba_unmultiplied(255, 255, 255, 105),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
    }

    fn settings_page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.share_controls(ui, ctx);
        section_divider(ui);
        section_heading(ui, "ПРИЦЕЛ");

        field_label(
            ui,
            "Стиль",
            Some("Стили CS2. Динамические варианты в оверлее отображаются статично."),
        );
        ComboBox::from_id_salt("style_combo")
            .width(ui.available_width())
            .selected_text(STYLE_NAMES[self.cfg.style as usize])
            .show_ui(ui, |ui| {
                for style in 0..=9u8 {
                    ui.selectable_value(&mut self.cfg.style, style, STYLE_NAMES[style as usize]);
                }
            });
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(
                RichText::new(STYLE_DESCRIPTIONS[self.cfg.style as usize])
                    .size(11.0)
                    .color(DIM),
            )
            .wrap(),
        );
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut self.cfg.center_dot,
                RichText::new("Точка в центре").size(12.5).color(TEXT),
            )
            .on_hover_text("Добавляет точку в центр прицела.");
            ui.add_space(16.0);
            ui.checkbox(
                &mut self.cfg.t_style,
                RichText::new("T-образный прицел").size(12.5).color(TEXT),
            )
            .on_hover_text("Убирает верхний луч у креста.");
        });
        ui.add_space(8.0);

        field_label(
            ui,
            "Обводка",
            Some("0 — без обводки, 1 — полная, 2 — половина."),
        );
        ComboBox::from_id_salt("outline_combo")
            .width(ui.available_width())
            .selected_text(match self.cfg.outline {
                0 => "Без обводки",
                1 => "Полная обводка",
                _ => "Половина обводки",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.cfg.outline, 0, "Без обводки");
                ui.selectable_value(&mut self.cfg.outline, 1, "Полная обводка");
                ui.selectable_value(&mut self.cfg.outline, 2, "Половина обводки");
            });
        ui.add_space(12.0);

        ui.columns(2, |columns| {
            field_label(
                &mut columns[0],
                "Длина луча",
                Some("Длина каждого луча в пикселях при опорной высоте экрана."),
            );
            columns[0].add_sized(
                [columns[0].available_width(), 24.0],
                egui::Slider::new(&mut self.cfg.length, 0..=64).suffix(" px"),
            );
            field_label(
                &mut columns[1],
                "Толщина",
                Some("Толщина лучей и центральной точки."),
            );
            columns[1].add_sized(
                [columns[1].available_width(), 24.0],
                egui::Slider::new(&mut self.cfg.thickness, 1..=20).suffix(" px"),
            );
        });
        ui.add_space(10.0);

        field_label(
            ui,
            "Зазор",
            Some(
                "Расстояние от центра до начала лучей. Полный диапазон задаётся через share code.",
            ),
        );
        ui.add_sized(
            [ui.available_width(), 24.0],
            egui::Slider::new(&mut self.cfg.gap, -30..=30).suffix(" px"),
        );
        ui.add_space(10.0);

        ui.columns(2, |columns| {
            color_field(&mut columns[0], "Цвет прицела", &mut self.cfg.color);
            color_field(&mut columns[1], "Цвет обводки", &mut self.cfg.outline_color);
        });
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(
                RichText::new(
                    "Динамический разброс, отдача и shot feedback не анимируются: оверлей не читает память игры.",
                )
                .size(11.0)
                .color(DIM),
            )
            .wrap(),
        );

        section_divider(ui);
        section_heading(ui, "ОВЕРЛЕЙ");
        ui.checkbox(
            &mut self.cfg.scale_with_screen,
            RichText::new("Масштабировать под экран")
                .size(12.5)
                .color(TEXT),
        )
        .on_hover_text("Размер прицела подстраивается под высоту активного окна.");
        ui.add_space(8.0);
        field_label(
            ui,
            "Опорная высота",
            Some("Высота, для которой заданы пиксельные размеры прицела."),
        );
        ui.add_sized(
            [ui.available_width(), 24.0],
            egui::Slider::new(&mut self.cfg.screen_height, 240..=4320).suffix(" px"),
        );
        ui.add_space(10.0);

        ui.columns(2, |columns| {
            field_label(
                &mut columns[0],
                "Смещение по X",
                Some("Сдвиг от центра активного окна."),
            );
            columns[0].add(
                DragValue::new(&mut self.cfg.offset_x)
                    .range(-2000..=2000)
                    .speed(1.0)
                    .suffix(" px"),
            );
            field_label(
                &mut columns[1],
                "Смещение по Y",
                Some("Сдвиг от центра активного окна."),
            );
            columns[1].add(
                DragValue::new(&mut self.cfg.offset_y)
                    .range(-2000..=2000)
                    .speed(1.0)
                    .suffix(" px"),
            );
        });
        ui.add_space(6.0);
        if secondary_button(ui, "Сбросить смещение").clicked() {
            self.cfg.offset_x = 0;
            self.cfg.offset_y = 0;
        }
        ui.add_space(10.0);

        let mut autostart = self.autostart;
        if ui
            .checkbox(
                &mut autostart,
                RichText::new("Запускать при входе в Windows")
                    .size(12.5)
                    .color(TEXT),
            )
            .on_hover_text("Добавляет приложение в автозапуск текущего пользователя.")
            .changed()
        {
            match autostart::set_enabled(autostart) {
                Ok(()) => {
                    self.autostart = autostart;
                    self.autostart_error = None;
                    self.set_notice(
                        if autostart {
                            "Автозапуск включён."
                        } else {
                            "Автозапуск выключен."
                        },
                        false,
                        Duration::from_secs(3),
                    );
                }
                Err(error) => {
                    logging::log("ERROR", format!("Автозапуск: {error}"));
                    self.autostart_error = Some(error);
                }
            }
        }
        if let Some(error) = &self.autostart_error {
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(format!("Автозапуск: {error}"))
                        .size(11.0)
                        .color(ERROR),
                )
                .wrap(),
            );
        }
    }

    fn share_controls(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        section_heading(ui, "SHARE CODE");
        let mut code = sharecode::encode_cs2(&self.cfg);
        field_label(ui, "Код текущего прицела", None);
        ui.add(
            TextEdit::multiline(&mut code)
                .font(TextStyle::Monospace)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .interactive(false),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if primary_button(ui, "Скопировать").clicked() {
                ctx.copy_text(code.clone());
                self.set_notice(
                    "Код прицела скопирован в буфер обмена.",
                    false,
                    Duration::from_secs(4),
                );
            }
            ui.add(
                egui::Label::new(
                    RichText::new("Вставьте в CS2: Настройки → Прицел → Импорт.")
                        .size(11.0)
                        .color(DIM),
                )
                .wrap(),
            );
        });
        ui.add_space(10.0);

        field_label(ui, "Импорт", None);
        let response = ui.add(
            TextEdit::singleline(&mut self.import_text)
                .font(TextStyle::Monospace)
                .hint_text("CS… или CSGO-…")
                .desired_width(f32::INFINITY),
        );
        let enter = response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if primary_button(ui, "Импортировать").clicked() || enter {
                self.import_share_code();
            }
            if !self.import_text.is_empty() && secondary_button(ui, "Очистить").clicked() {
                self.import_text.clear();
            }
            ui.add(
                egui::Label::new(
                    RichText::new("Поддерживаются CS2 v1 и legacy CS:GO/CS2 коды.")
                        .size(11.0)
                        .color(DIM),
                )
                .wrap(),
            );
        });
        ui.add_space(8.0);

        egui::CollapsingHeader::new(
            RichText::new("Динамические параметры кода")
                .size(12.0)
                .color(MUTED),
        )
        .default_open(false)
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Оверлей не анимирует эти значения, но сохраняет их в share code.",
                    )
                    .size(11.0)
                    .color(DIM),
                )
                .wrap(),
            );
            ui.add_space(6.0);
            ui.checkbox(
                &mut self.cfg.follow_recoil,
                RichText::new("Следование за отдачей")
                    .size(12.5)
                    .color(TEXT),
            );
            ui.add_space(6.0);
            ui.columns(2, |columns| {
                field_label(&mut columns[0], "Spread limit", None);
                columns[0].add_sized(
                    [columns[0].available_width(), 24.0],
                    egui::Slider::new(&mut self.cfg.dynamic_spread_limit, 0..=255),
                );
                field_label(&mut columns[1], "Split distance", None);
                columns[1].add_sized(
                    [columns[1].available_width(), 24.0],
                    egui::Slider::new(&mut self.cfg.split_distance, 0..=127),
                );
            });
            ui.add_space(6.0);
            ui.columns(2, |columns| {
                field_label(&mut columns[0], "Inner split alpha", None);
                columns[0].add_sized(
                    [columns[0].available_width(), 24.0],
                    egui::Slider::new(&mut self.cfg.inner_split_alpha, 0.0..=1.0).fixed_decimals(2),
                );
                field_label(&mut columns[1], "Outer split alpha", None);
                columns[1].add_sized(
                    [columns[1].available_width(), 24.0],
                    egui::Slider::new(&mut self.cfg.outer_split_alpha, 0.3..=1.0).fixed_decimals(2),
                );
            });
            ui.add_space(6.0);
            ui.columns(2, |columns| {
                field_label(&mut columns[0], "Split size ratio", None);
                columns[0].add_sized(
                    [columns[0].available_width(), 24.0],
                    egui::Slider::new(&mut self.cfg.split_size_ratio, 0.0..=1.0).fixed_decimals(2),
                );
                field_label(&mut columns[1], "Scope dot scale", None);
                columns[1].add_sized(
                    [columns[1].available_width(), 24.0],
                    egui::Slider::new(&mut self.cfg.scope_dot_scale, 0.1..=2.0).fixed_decimals(2),
                );
            });
            ui.add_space(6.0);
            ui.checkbox(
                &mut self.cfg.scope_dot_color_match,
                RichText::new("Scope dot использует цвет прицела")
                    .size(12.5)
                    .color(TEXT),
            );
        });
    }

    fn import_share_code(&mut self) {
        let code = self.import_text.trim().to_string();
        if code.is_empty() {
            self.set_notice(
                "Вставьте share code для импорта.",
                true,
                Duration::from_secs(4),
            );
            return;
        }
        match sharecode::decode(&code) {
            Ok(decoded) => {
                let behavior = (
                    self.cfg.enabled,
                    self.cfg.offset_x,
                    self.cfg.offset_y,
                    self.cfg.scale_with_screen,
                );
                let format = decoded.format;
                let mut imported = decoded.config;
                imported.enabled = behavior.0;
                imported.offset_x = behavior.1;
                imported.offset_y = behavior.2;
                imported.scale_with_screen = behavior.3;
                self.cfg = imported.sanitized();
                self.import_text.clear();
                self.set_notice(
                    format!(
                        "Код импортирован ({}). Позиция и параметры оверлея сохранены.",
                        format.label_ru()
                    ),
                    false,
                    Duration::from_secs(5),
                );
            }
            Err(error) => {
                self.set_notice(
                    format!("Не удалось импортировать код: {}", error.user_message_ru()),
                    true,
                    Duration::from_secs(6),
                );
            }
        }
    }

    fn about_page(&mut self, ui: &mut Ui) {
        section_heading(ui, "CROSSHAIR OVERLAY");
        ui.label(
            RichText::new(format!(
                "Версия {} · Rust + egui + Direct2D",
                env!("CARGO_PKG_VERSION")
            ))
            .size(11.5)
            .color(DIM),
        );
        ui.add_space(6.0);
        ui.add(
            egui::Label::new(
                RichText::new(
                    "Внешний статичный прицел поверх активного окна. Приложение не внедряется в процесс игры, не читает и не изменяет память.",
                )
                .size(12.5)
                .color(MUTED),
            )
            .wrap(),
        );

        section_divider(ui);
        section_heading(ui, "ВАЙБ-КОД ПРОЕКТ");
        ui.add(
            egui::Label::new(
                RichText::new(
                    "Проект написан в формате вайб-кодинга: большая часть кода и документации создана с помощью ИИ и затем проверена вручную.",
                )
                .size(12.5)
                .color(MUTED),
            )
            .wrap(),
        );

        section_divider(ui);
        section_heading(ui, "ИСПОЛЬЗОВАНИЕ");
        for text in [
            "Запустите игру в оконном или безрамочном режиме.",
            "Сверните настройки или переключитесь в игру.",
            "Ctrl+Alt+X включает и выключает прицел.",
            "Закрытие окна сворачивает приложение в трей.",
        ] {
            bullet(ui, text);
        }

        section_divider(ui);
        section_heading(ui, "ОГРАНИЧЕНИЯ");
        for text in [
            "Эксклюзивный полноэкранный режим не поддерживается.",
            "Динамический разброс, отдача и shot feedback отображаются статично.",
            "Прицел скрывается, когда активно окно настроек.",
            "Положение привязывается к центру клиентской области активного окна.",
        ] {
            bullet(ui, text);
        }
        ui.add_space(4.0);
        ui.add(
            egui::Label::new(
                RichText::new(
                    "Сторонние оверлеи могут нарушать правила отдельных игр и античит-систем. Используйте приложение на свой риск.",
                )
                .size(11.5)
                .color(MUTED),
            )
            .wrap(),
        );

        section_divider(ui);
        section_heading(ui, "ФАЙЛЫ");
        field_label(ui, "Конфигурация", None);
        ui.add(
            egui::Label::new(
                RichText::new(config::config_path().display().to_string())
                    .size(11.0)
                    .monospace()
                    .color(MUTED),
            )
            .wrap(),
        );
        ui.add_space(4.0);
        field_label(ui, "Журнал", None);
        ui.add(
            egui::Label::new(
                RichText::new(logging::log_path().display().to_string())
                    .size(11.0)
                    .monospace()
                    .color(MUTED),
            )
            .wrap(),
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if secondary_button(ui, "Открыть папку").clicked() {
                open_folder(&config::config_dir());
            }
            if secondary_button(ui, "Открыть журнал").clicked() {
                let _ = std::process::Command::new("notepad")
                    .arg(logging::log_path())
                    .spawn();
            }
        });
    }
}

impl eframe::App for CrosshairApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_tray(ctx);
        self.sync();
        self.flush_save();
        self.handle_close(ctx);
        if self.dirty_since.is_some() || self.notice.is_some() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let snapshot = self.sync();
        let before = self.cfg.clone();
        let ctx = ui.ctx().clone();

        Panel::left("sidebar")
            .exact_size(SIDEBAR_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(SIDEBAR).inner_margin(Margin::same(18)))
            .show(ui, |ui| {
                self.sidebar(ui, &snapshot);
            });

        egui::CentralPanel::default_margins()
            .frame(Frame::new().fill(BG).inner_margin(Margin::same(22)))
            .show(ui, |ui| {
                self.main_content(ui, &snapshot, &ctx);
            });

        self.commit_if_changed(&before);
        self.flush_save();
        if self.minimize_requested {
            self.minimize_requested = false;
            self.force_save();
            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        }
        if self.dirty_since.is_some() || self.notice.is_some() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }
}

impl Drop for CrosshairApp {
    fn drop(&mut self) {
        self.force_save();
    }
}

fn configure_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(16, 16, 16);
    visuals.faint_bg_color = SURFACE;
    visuals.code_bg_color = Color32::from_rgb(18, 18, 18);
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = ACCENT;
    visuals.error_fg_color = ERROR;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(5);
    visuals.menu_corner_radius = CornerRadius::same(5);
    visuals.selection.bg_fill = ACCENT_DARK;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.weak_bg_fill = SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(5);
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(5);
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_HOVER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.2, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(5);
    visuals.widgets.active.bg_fill = ACCENT_DARK;
    visuals.widgets.active.weak_bg_fill = ACCENT_DARK;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.2, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(5);
    visuals.widgets.open.bg_fill = HOVER;
    visuals.widgets.open.weak_bg_fill = HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER_HOVER);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.open.corner_radius = CornerRadius::same(5);

    ctx.set_visuals_of(egui::Theme::Dark, visuals.clone());
    ctx.set_visuals_of(egui::Theme::Light, visuals);

    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.spacing.item_spacing = vec2(10.0, 10.0);
            style.spacing.button_padding = vec2(13.0, 7.0);
            style.spacing.interact_size.y = 30.0;
            style.spacing.slider_width = 190.0;
            style.spacing.combo_width = 190.0;
            style.spacing.window_margin = Margin::same(0);
            style.spacing.menu_margin = Margin::same(6);
            style.animation_time = 0.1;
            style
                .text_styles
                .insert(TextStyle::Heading, FontId::proportional(24.0));
            style
                .text_styles
                .insert(TextStyle::Body, FontId::proportional(13.5));
            style
                .text_styles
                .insert(TextStyle::Button, FontId::proportional(13.5));
            style
                .text_styles
                .insert(TextStyle::Small, FontId::proportional(11.0));
            style
                .text_styles
                .insert(TextStyle::Monospace, FontId::monospace(12.5));
        });
    }
}

fn nav_item(ui: &mut Ui, selected: bool, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let hovered = response.hovered();
    let fill = if selected {
        SURFACE
    } else if hovered {
        HOVER
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
    ui.painter().text(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        if selected || hovered { TEXT } else { MUTED },
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn status_plain(ui: &mut Ui, snapshot: &Snapshot, enabled: bool) {
    ui.label(RichText::new("СТАТУС").size(10.0).strong().color(DIM));
    ui.add_space(3.0);
    let (label, color) = if snapshot.overlay_error.is_some() {
        ("Ошибка оверлея", ERROR)
    } else if !snapshot.overlay_running {
        ("Запуск оверлея…", MUTED)
    } else if enabled {
        ("Оверлей работает", TEXT)
    } else {
        ("Прицел выключен", MUTED)
    };
    ui.label(RichText::new(label).size(12.5).strong().color(color));
    ui.add_space(2.0);
    ui.label(RichText::new("Ctrl+Alt+X — вкл/выкл").size(10.5).color(DIM));
}

fn section_heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(10.5).strong().color(DIM));
    ui.add_space(7.0);
}

fn section_divider(ui: &mut Ui) {
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(12.0);
}

fn field_label(ui: &mut Ui, label: &str, hint: Option<&str>) {
    let response = ui.label(RichText::new(label).size(12.0).color(MUTED));
    if let Some(hint) = hint {
        response.on_hover_text(hint);
    }
    ui.add_space(2.0);
}

fn plain_notice(ui: &mut Ui, text: impl Into<String>, error: bool) {
    let text = RichText::new(text.into())
        .size(12.0)
        .color(if error { ERROR } else { MUTED });
    ui.add(egui::Label::new(if error { text.strong() } else { text }).wrap());
    ui.add_space(6.0);
}

fn bullet(ui: &mut Ui, text: &str) {
    ui.horizontal_top(|ui| {
        ui.label(RichText::new("—").size(11.5).color(DIM));
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(MUTED)).wrap());
    });
    ui.add_space(3.0);
}

fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        Button::new(RichText::new(text).size(12.5).color(BG))
            .fill(ACCENT)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(5)),
    )
}

fn primary_button_sized(ui: &mut Ui, text: &str, height: f32) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), height],
        Button::new(RichText::new(text).size(13.5).color(BG))
            .fill(ACCENT)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(5)),
    )
}

fn secondary_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        Button::new(RichText::new(text).size(12.5).color(MUTED))
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(5)),
    )
}

fn secondary_button_sized(ui: &mut Ui, text: &str, height: f32) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), height],
        Button::new(RichText::new(text).size(13.0).color(MUTED))
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(5)),
    )
}

fn color_field(ui: &mut Ui, label: &str, value: &mut Rgba) {
    field_label(ui, label, None);
    ui.horizontal(|ui| {
        let mut color = to_color32(*value);
        if color_picker::color_edit_button_srgba(ui, &mut color, color_picker::Alpha::OnlyBlend)
            .changed()
        {
            *value = from_color32(color);
        }
        ui.label(
            RichText::new(hex_color(*value))
                .size(11.5)
                .monospace()
                .color(MUTED),
        );
    });
}

fn to_color32(color: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

fn from_color32(color: Color32) -> Rgba {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Rgba::new(r, g, b, a)
}

fn hex_color(color: Rgba) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color.r, color.g, color.b, color.a
    )
}

#[cfg(windows)]
fn open_folder(path: &std::path::Path) {
    if let Err(error) = std::process::Command::new("explorer").arg(path).spawn() {
        logging::log("ERROR", format!("Не удалось открыть папку: {error}"));
    }
}
