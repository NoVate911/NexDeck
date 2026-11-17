//! Pure-Rust Desktop Manager GUI implemented with `eframe` / `egui`.
//!
//! Styled in a Clean Modern (Windows 11 Fluent / macOS) design language with
//! matte layered surfaces, subtle 1px borders, generous spacing, a left
//! navigation sidebar, and smooth micro-animations.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Layout, Margin, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextStyle, Vec2,
};
use nexdeck_api::ApiPermission;
use nexdeck_api::panels::{
    MAX_PANEL_COLUMNS, MAX_REFRESH_INTERVAL_MS, MIN_PANEL_COLUMNS, MIN_REFRESH_INTERVAL_MS,
    WidgetKind, WidgetSpec, WidgetStyle,
};
use nexdeck_core::AGENT_VERSION;
use nexdeck_transport::PROTOCOL_VERSION;

use crate::state::{
    DesktopManagerState, DesktopTab, action_label, all_telemetry_bindings, all_widget_actions,
    all_widget_icons, all_widget_kinds, icon_label, telemetry_label, widget_kind_label,
};

const GRID_CELL_GAP: f32 = 10.0;
const GRID_MIN_ROWS: u16 = 4;
const ANIM_DURATION: f32 = 0.18;

/// Clean Modern theme palette (Fluent Dark & Clean Light).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThemePalette {
    is_dark: bool,
    canvas_bg: Color32,
    sidebar_bg: Color32,
    card_bg: Color32,
    card_elevated_bg: Color32,
    card_border: Color32,
    subtle_border: Color32,
    text_primary: Color32,
    text_secondary: Color32,
    accent: Color32,
    accent_soft_bg: Color32,
    status_ok: Color32,
    status_warn: Color32,
    status_err: Color32,
}

impl ThemePalette {
    const fn fluent_dark() -> Self {
        Self {
            is_dark: true,
            canvas_bg: Color32::from_rgb(13, 17, 23),
            sidebar_bg: Color32::from_rgb(18, 24, 34),
            card_bg: Color32::from_rgb(23, 30, 43),
            card_elevated_bg: Color32::from_rgb(30, 39, 56),
            card_border: Color32::from_rgb(42, 54, 74),
            subtle_border: Color32::from_rgb(34, 44, 62),
            text_primary: Color32::from_rgb(241, 245, 249),
            text_secondary: Color32::from_rgb(148, 163, 184),
            accent: Color32::from_rgb(59, 130, 246),
            accent_soft_bg: Color32::from_rgba_premultiplied(15, 33, 62, 180),
            status_ok: Color32::from_rgb(16, 185, 129),
            status_warn: Color32::from_rgb(245, 158, 11),
            status_err: Color32::from_rgb(239, 68, 68),
        }
    }

    const fn clean_light() -> Self {
        Self {
            is_dark: false,
            canvas_bg: Color32::from_rgb(244, 246, 250),
            sidebar_bg: Color32::from_rgb(235, 239, 245),
            card_bg: Color32::from_rgb(255, 255, 255),
            card_elevated_bg: Color32::from_rgb(248, 250, 252),
            card_border: Color32::from_rgb(218, 226, 236),
            subtle_border: Color32::from_rgb(229, 235, 243),
            text_primary: Color32::from_rgb(15, 23, 42),
            text_secondary: Color32::from_rgb(100, 116, 139),
            accent: Color32::from_rgb(37, 99, 235),
            accent_soft_bg: Color32::from_rgba_premultiplied(219, 234, 254, 220),
            status_ok: Color32::from_rgb(5, 150, 105),
            status_warn: Color32::from_rgb(217, 119, 6),
            status_err: Color32::from_rgb(220, 38, 38),
        }
    }
}

/// Top-level `eframe::App` state wrapping [`DesktopManagerState`].
pub struct NexDeckDesktopApp {
    state: DesktopManagerState,
    widget_draft: Option<WidgetDraft>,
    panel_meta_draft: Option<PanelMetaDraft>,
    dark_mode: bool,
    show_json_drawer: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PanelMetaDraft {
    panel_id: String,
    name: String,
    columns: u16,
    refresh_interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WidgetDraft {
    panel_id: String,
    spec: WidgetSpec,
}

impl NexDeckDesktopApp {
    /// Initializes visual theme, optional Windows Cyrillic font fallback, and initial drafts.
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>, state: DesktopManagerState) -> Self {
        configure_fonts(&cc.egui_ctx);
        let dark_mode = true;
        apply_clean_modern_theme(&cc.egui_ctx, ThemePalette::fluent_dark());

        let mut app = Self {
            state,
            widget_draft: None,
            panel_meta_draft: None,
            dark_mode,
            show_json_drawer: false,
        };
        app.sync_drafts_from_selection();
        app
    }

    const fn palette(&self) -> ThemePalette {
        if self.dark_mode {
            ThemePalette::fluent_dark()
        } else {
            ThemePalette::clean_light()
        }
    }

    fn sync_drafts_from_selection(&mut self) {
        if let Some(panel) = self.state.selected_panel() {
            let needs_panel_sync = self
                .panel_meta_draft
                .as_ref()
                .is_none_or(|draft| draft.panel_id != panel.id);
            if needs_panel_sync {
                self.panel_meta_draft = Some(PanelMetaDraft {
                    panel_id: panel.id.clone(),
                    name: panel.name.clone(),
                    columns: panel.columns,
                    refresh_interval_ms: panel.refresh_interval_ms,
                });
            }
        } else {
            self.panel_meta_draft = None;
        }

        let current_widget = self.state.selected_widget();
        let current_panel_id = self.state.selected_panel().map(|p| p.id.as_str());
        match (current_panel_id, current_widget) {
            (Some(panel_id), Some(spec)) => {
                let needs_sync = self
                    .widget_draft
                    .as_ref()
                    .is_none_or(|draft| draft.panel_id != panel_id || draft.spec.id != spec.id);
                if needs_sync {
                    self.widget_draft = Some(WidgetDraft {
                        panel_id: panel_id.to_owned(),
                        spec,
                    });
                }
            }
            _ => {
                self.widget_draft = None;
            }
        }
    }

    fn force_resync_drafts(&mut self) {
        self.panel_meta_draft = None;
        self.widget_draft = None;
        self.sync_drafts_from_selection();
    }
}

impl eframe::App for NexDeckDesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let running = self.state.agent_process.is_running();
        if running {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        self.sync_drafts_from_selection();
        let theme = self.palette();

        egui::SidePanel::left("nexdeck_sidebar")
            .resizable(false)
            .exact_width(256.0)
            .frame(
                egui::Frame::new()
                    .fill(theme.sidebar_bg)
                    .stroke(Stroke::new(1.0_f32, theme.card_border))
                    .inner_margin(Margin::symmetric(16, 18)),
            )
            .show(ctx, |ui| {
                self.draw_sidebar(ui, ctx, running, theme);
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme.canvas_bg)
                    .inner_margin(Margin::symmetric(24, 20)),
            )
            .show(ctx, |ui| {
                self.draw_page_header(ui, running, theme);
                ui.add_space(14.0);

                // Smooth micro-animation when switching tabs.
                let tab_idx = match self.state.active_tab {
                    DesktopTab::Panels => 0.0_f32,
                    DesktopTab::AgentSettings => 1.0_f32,
                    DesktopTab::DevicesAndTls => 2.0_f32,
                };
                let _animated_tab =
                    ctx.animate_value_with_time(egui::Id::new("active_tab_anim"), tab_idx, 0.20);

                match self.state.active_tab {
                    DesktopTab::Panels => self.draw_panels_tab(ui, ctx, theme),
                    DesktopTab::AgentSettings => self.draw_agent_tab(ui, ctx, running, theme),
                    DesktopTab::DevicesAndTls => self.draw_devices_tab(ui, ctx, theme),
                }
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.agent_process.stop();
    }
}

impl NexDeckDesktopApp {
    fn draw_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        agent_running: bool,
        theme: ThemePalette,
    ) {
        // Brand Header
        ui.horizontal(|ui| {
            let (badge_rect, _) = ui.allocate_exact_size(Vec2::new(38.0, 38.0), Sense::hover());
            let painter = ui.painter_at(badge_rect);
            painter.rect_filled(badge_rect, CornerRadius::same(10), theme.accent);
            painter.text(
                badge_rect.center(),
                Align2::CENTER_CENTER,
                "ND",
                FontId::proportional(16.0),
                Color32::WHITE,
            );

            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(
                    RichText::new("NexDeck Studio")
                        .size(17.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    RichText::new(format!("v{AGENT_VERSION} • Протокол v{PROTOCOL_VERSION}"))
                        .size(11.5)
                        .color(theme.text_secondary),
                );
            });
        });

        ui.add_space(18.0);

        // Live Agent Status Card in Sidebar
        fluent_card(ui, theme, |ui| {
            let anim_run = ctx.animate_bool_with_time(
                egui::Id::new("agent_status_dot"),
                agent_running,
                ANIM_DURATION,
            );
            let (dot_color, status_title, status_sub) = if agent_running {
                if self.state.agent_process.is_pair_mode() {
                    (
                        theme.accent,
                        "Режим сопряжения",
                        "Ожидание подключения телефона по PIN/QR",
                    )
                } else {
                    (
                        theme.status_ok,
                        "Агент активен",
                        "Локальный WSS и Universal API работают",
                    )
                }
            } else {
                (
                    theme.status_warn,
                    "Агент не запущен",
                    "Нажмите кнопку ниже для старта",
                )
            };

            ui.horizontal(|ui| {
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                let painter = ui.painter_at(dot_rect);
                if anim_run > 0.05 {
                    painter.circle_filled(
                        dot_rect.center(),
                        6.5 * anim_run,
                        Color32::from_rgba_unmultiplied(
                            dot_color.r(),
                            dot_color.g(),
                            dot_color.b(),
                            48,
                        ),
                    );
                }
                painter.circle_filled(dot_rect.center(), 4.5, dot_color);
                ui.label(
                    RichText::new(status_title)
                        .size(13.5)
                        .strong()
                        .color(theme.text_primary),
                );
            });
            ui.label(
                RichText::new(status_sub)
                    .size(11.5)
                    .color(theme.text_secondary),
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if !agent_running {
                    if primary_button(ui, "▶ Старт", theme).clicked() {
                        self.state.start_agent(false);
                    }
                    if secondary_button(ui, "🔗 С PIN", theme).clicked() {
                        self.state.start_agent(true);
                        self.state.active_tab = DesktopTab::AgentSettings;
                    }
                } else if danger_button(ui, "■ Остановить", theme).clicked() {
                    self.state.stop_agent();
                }
            });
        });

        ui.add_space(18.0);
        ui.label(
            RichText::new("НАВИГАЦИЯ")
                .size(11.0)
                .strong()
                .color(theme.text_secondary),
        );
        ui.add_space(6.0);

        let nav_items = [
            (
                DesktopTab::Panels,
                "⊞  Конструктор деки",
                "Визуальные панели и сетка",
            ),
            (
                DesktopTab::AgentSettings,
                "⚡  Агент и подключение",
                "Сопряжение, журнал и автозапуск",
            ),
            (
                DesktopTab::DevicesAndTls,
                "🛡  Безопасность и API",
                "Устройства, TLS и токены",
            ),
        ];

        for (tab, title, subtitle) in nav_items {
            if draw_sidebar_nav_item(
                ui,
                ctx,
                tab,
                self.state.active_tab == tab,
                title,
                subtitle,
                theme,
            ) {
                self.state.active_tab = tab;
            }
            ui.add_space(4.0);
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.label(
                RichText::new(format!(
                    "Хранилище: {}",
                    self.state.paths.root_dir().display()
                ))
                .size(11.0)
                .color(theme.text_secondary),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let theme_label = if self.dark_mode {
                    "☀ Светлая тема"
                } else {
                    "🌙 Тёмная тема"
                };
                if secondary_button(ui, theme_label, theme).clicked() {
                    self.dark_mode = !self.dark_mode;
                    apply_clean_modern_theme(ctx, self.palette());
                }
                if secondary_button(ui, "↻ Обновить", theme).clicked() {
                    self.state.refresh_from_disk();
                    self.force_resync_drafts();
                }
            });
        });
    }

    fn draw_page_header(&mut self, ui: &mut egui::Ui, running: bool, theme: ThemePalette) {
        let (title, subtitle) = match self.state.active_tab {
            DesktopTab::Panels => (
                "Конструктор интерактивных панелей",
                "Настраивайте раскладку виджетов, показатели телеметрии и горячие клавиши для мобильного клиента",
            ),
            DesktopTab::AgentSettings => (
                "Управление фоновым агентом и сопряжением",
                "Запуск локального WSS-сервера, выпуск PIN/QR-кода для телефона и настройка автозапуска Windows",
            ),
            DesktopTab::DevicesAndTls => (
                "Центр безопасности, устройств и Universal API",
                "Управление доверенными телефонами, SHA-256 отпечатком сертификата и токенами сторонних приложений",
            ),
        };

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.label(
                    RichText::new(title)
                        .size(22.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    RichText::new(subtitle)
                        .size(13.0)
                        .color(theme.text_secondary),
                );
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (pill_bg, pill_fg, pill_text) = if running {
                    (
                        Color32::from_rgba_unmultiplied(
                            theme.status_ok.r(),
                            theme.status_ok.g(),
                            theme.status_ok.b(),
                            32,
                        ),
                        theme.status_ok,
                        format!("● WSS {}", self.state.settings_draft.bind_addr),
                    )
                } else {
                    (
                        theme.card_elevated_bg,
                        theme.text_secondary,
                        format!("○ WSS {}", self.state.settings_draft.bind_addr),
                    )
                };
                egui::Frame::new()
                    .fill(pill_bg)
                    .stroke(Stroke::new(1.0_f32, theme.card_border))
                    .corner_radius(CornerRadius::same(20))
                    .inner_margin(Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.label(RichText::new(pill_text).size(12.5).strong().color(pill_fg));
                    });
            });
        });

        // Inline toast banner for status or error feedback
        if self.state.error_message.is_some() || self.state.status_message.is_some() {
            ui.add_space(10.0);
            let is_err = self.state.error_message.is_some();
            let banner_color = if is_err {
                theme.status_err
            } else {
                theme.status_ok
            };
            let text = self
                .state
                .error_message
                .as_deref()
                .or(self.state.status_message.as_deref())
                .unwrap_or_default()
                .to_owned();

            let mut dismiss = false;
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(
                    banner_color.r(),
                    banner_color.g(),
                    banner_color.b(),
                    26,
                ))
                .stroke(Stroke::new(1.0_f32, banner_color))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&text).size(13.0).strong().color(banner_color));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                dismiss = true;
                            }
                        });
                    });
                });
            if dismiss {
                self.state.error_message = None;
                self.state.status_message = None;
            }
        }
    }

    fn draw_panels_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, theme: ThemePalette) {
        // Top Panel Switcher & Actions Bar
        fluent_card(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Профили панелей:")
                        .size(13.0)
                        .strong()
                        .color(theme.text_secondary),
                );

                let panel_headers: Vec<(usize, String, String, usize)> = self
                    .state
                    .deck
                    .panels
                    .iter()
                    .enumerate()
                    .map(|(idx, panel)| {
                        (
                            idx,
                            panel.id.clone(),
                            panel.name.clone(),
                            panel.widgets.len(),
                        )
                    })
                    .collect();

                for (idx, _id, name, widget_count) in panel_headers {
                    let selected = idx == self.state.selected_panel_idx;
                    let pill_label = format!("{name} ({widget_count})");
                    let btn =
                        egui::Button::new(RichText::new(pill_label).size(13.0).strong().color(
                            if selected {
                                Color32::WHITE
                            } else {
                                theme.text_primary
                            },
                        ))
                        .fill(if selected {
                            theme.accent
                        } else {
                            theme.card_elevated_bg
                        })
                        .corner_radius(CornerRadius::same(8));

                    if ui.add(btn).clicked() && !selected {
                        self.state.select_panel(idx);
                        self.force_resync_drafts();
                    }
                }

                ui.separator();
                ui.add(
                    egui::TextEdit::singleline(&mut self.state.new_panel_name)
                        .hint_text("Имя новой панели…")
                        .desired_width(140.0),
                );
                if primary_button(ui, "+ Создать панель", theme).clicked() {
                    self.state.add_panel();
                    self.force_resync_drafts();
                }

                let can_delete = self.state.deck.panels.len() > 1;
                if can_delete && danger_button(ui, "Удалить панель", theme).clicked() {
                    self.state.delete_selected_panel();
                    self.force_resync_drafts();
                }

                if secondary_button(ui, "{ } JSON Импорт/Экспорт", theme).clicked() {
                    self.show_json_drawer = !self.show_json_drawer;
                }
            });
        });

        ui.add_space(12.0);

        // Two-column spacious layout: Left (Deck Canvas + Palette), Right (Inspector + Panel Meta)
        let avail_w = ui.available_width();
        let right_w = 350.0_f32.min(avail_w * 0.38);
        let left_w = (avail_w - right_w - 16.0).max(320.0);

        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| {
                    ScrollArea::vertical()
                        .id_salt("panels_canvas_scroll")
                        .show(ui, |ui| {
                            self.draw_widget_palette(ui, theme);
                            ui.add_space(12.0);
                            self.draw_grid_canvas(ui, ctx, theme);
                            if self.show_json_drawer {
                                ui.add_space(12.0);
                                self.draw_json_section(ui, theme);
                            }
                        });
                },
            );

            ui.add_space(12.0);

            ui.allocate_ui_with_layout(
                Vec2::new(right_w, ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| {
                    ScrollArea::vertical()
                        .id_salt("panels_inspector_scroll")
                        .show(ui, |ui| {
                            self.draw_widget_inspector(ui, theme);
                            ui.add_space(12.0);
                            self.draw_panel_meta_section(ui, theme);
                        });
                },
            );
        });
    }

    fn draw_widget_palette(&mut self, ui: &mut egui::Ui, theme: ThemePalette) {
        fluent_card(ui, theme, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Палитра виджетов")
                        .size(14.5)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    RichText::new("— нажмите, чтобы разместить в первой свободной ячейке")
                        .size(12.0)
                        .color(theme.text_secondary),
                );
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for &kind in all_widget_kinds() {
                    if secondary_button(ui, &format!("+ {}", widget_kind_label(kind)), theme)
                        .clicked()
                    {
                        self.state.add_widget(kind);
                        self.force_resync_drafts();
                    }
                }
            });
        });
    }

    fn draw_grid_canvas(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, theme: ThemePalette) {
        let Some(panel) = self.state.selected_panel().cloned() else {
            return;
        };

        fluent_card(ui, theme, |ui| {
            let cols = panel.columns.max(MIN_PANEL_COLUMNS);
            let max_bottom = panel
                .widgets
                .iter()
                .map(|w| w.row.saturating_add(w.row_span))
                .max()
                .unwrap_or(GRID_MIN_ROWS);
            let rows = max_bottom.max(GRID_MIN_ROWS);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Холст панели «{}»", panel.name))
                        .size(15.5)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Сетка {cols}×{rows} • Обновление {} мс",
                            panel.refresh_interval_ms
                        ))
                        .size(12.0)
                        .color(theme.text_secondary),
                    );
                });
            });
            ui.add_space(8.0);

            let avail_width = ui.available_width().max(280.0);
            let cell_w =
                ((avail_width - GRID_CELL_GAP * f32::from(cols + 1)) / f32::from(cols)).max(54.0);
            let cell_h = (cell_w * 0.72).clamp(64.0, 108.0);
            let canvas_h = GRID_CELL_GAP + f32::from(rows) * (cell_h + GRID_CELL_GAP);

            let (canvas_rect, _) =
                ui.allocate_exact_size(Vec2::new(avail_width, canvas_h), Sense::hover());
            let painter = ui.painter_at(canvas_rect);

            painter.rect_filled(canvas_rect, CornerRadius::same(12), theme.canvas_bg);
            painter.rect_stroke(
                canvas_rect,
                CornerRadius::same(12),
                Stroke::new(1.0_f32, theme.subtle_border),
                StrokeKind::Inside,
            );

            // Empty slot guides
            for row in 0..rows {
                for col in 0..cols {
                    let x = canvas_rect.left()
                        + GRID_CELL_GAP
                        + f32::from(col) * (cell_w + GRID_CELL_GAP);
                    let y = canvas_rect.top()
                        + GRID_CELL_GAP
                        + f32::from(row) * (cell_h + GRID_CELL_GAP);
                    let cell_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h));
                    painter.rect_stroke(
                        cell_rect,
                        CornerRadius::same(8),
                        Stroke::new(1.0_f32, theme.subtle_border),
                        StrokeKind::Inside,
                    );
                }
            }

            let mut clicked_widget_id: Option<String> = None;
            for widget in &panel.widgets {
                let x = canvas_rect.left()
                    + GRID_CELL_GAP
                    + f32::from(widget.col) * (cell_w + GRID_CELL_GAP);
                let y = canvas_rect.top()
                    + GRID_CELL_GAP
                    + f32::from(widget.row) * (cell_h + GRID_CELL_GAP);
                let w = f32::from(widget.col_span) * cell_w
                    + f32::from(widget.col_span.saturating_sub(1)) * GRID_CELL_GAP;
                let h = f32::from(widget.row_span) * cell_h
                    + f32::from(widget.row_span.saturating_sub(1)) * GRID_CELL_GAP;
                let base_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));

                let id = ui.make_persistent_id(("widget_tile", &widget.id));
                let response = ui.interact(base_rect, id, Sense::click());
                let selected = self
                    .state
                    .selected_widget_id
                    .as_deref()
                    .is_some_and(|sel_id| sel_id == widget.id);

                let hover_t =
                    ctx.animate_bool_with_time(id.with("hover"), response.hovered(), 0.14);
                let select_t = ctx.animate_bool_with_time(id.with("sel"), selected, 0.16);
                let expand = hover_t * 1.5 + select_t * 1.0;
                let rect = base_rect.expand(expand);

                let bg =
                    parse_hex_color(&widget.style.background_hex).unwrap_or(theme.card_elevated_bg);
                let fg =
                    parse_hex_color(&widget.style.foreground_hex).unwrap_or(theme.text_primary);
                let accent = parse_hex_color(&widget.style.accent_hex).unwrap_or(theme.accent);
                let radius = CornerRadius::same(widget.style.corner_radius.min(18));

                painter.rect_filled(rect, radius, bg);
                painter.rect_stroke(
                    rect,
                    radius,
                    Stroke::new(
                        if selected {
                            2.2_f32
                        } else if response.hovered() {
                            1.6_f32
                        } else {
                            1.0_f32
                        },
                        if selected { theme.accent } else { accent },
                    ),
                    StrokeKind::Inside,
                );

                // Top type pill badge
                painter.text(
                    rect.right_top() + Vec2::new(-10.0, 8.0),
                    Align2::RIGHT_TOP,
                    widget_kind_label(widget.kind),
                    FontId::proportional(10.5),
                    Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 165),
                );

                painter.text(
                    rect.left_top() + Vec2::new(10.0, 10.0),
                    Align2::LEFT_TOP,
                    &widget.title,
                    FontId::proportional(f32::from(widget.style.font_size_sp).clamp(12.0, 17.0)),
                    fg,
                );

                let subtitle = widget
                    .telemetry
                    .map_or_else(|| action_label(widget.action), telemetry_label);
                painter.text(
                    rect.left_bottom() + Vec2::new(10.0, -10.0),
                    Align2::LEFT_BOTTOM,
                    subtitle,
                    FontId::proportional(11.5),
                    accent,
                );

                if response.clicked() {
                    clicked_widget_id = Some(widget.id.clone());
                }
            }

            if let Some(id) = clicked_widget_id {
                self.state.select_widget(&id);
                self.force_resync_drafts();
            }
        });
    }

    fn draw_widget_inspector(&mut self, ui: &mut egui::Ui, theme: ThemePalette) {
        let Some(mut draft) = self.widget_draft.clone() else {
            fluent_card(ui, theme, |ui| {
                ui.label(
                    RichText::new("Инспектор виджета")
                        .size(15.5)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Выберите любой элемент на холсте слева или добавьте новый из палитры.",
                    )
                    .color(theme.text_secondary),
                );
            });
            return;
        };

        let mut apply_edit = false;
        let mut move_delta: Option<(i32, i32)> = None;
        let mut resize_delta: Option<(i32, i32)> = None;
        let mut delete_clicked = false;

        fluent_card(ui, theme, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Свойства виджета")
                        .size(15.5)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("#{}", draft.spec.id))
                            .size(11.5)
                            .color(theme.text_secondary),
                    );
                });
            });
            ui.add_space(8.0);

            egui::Grid::new("inspector_main_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Заголовок:");
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.spec.title)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();

                    ui.label("Тип:");
                    egui::ComboBox::from_id_salt("widget_kind_combo")
                        .selected_text(widget_kind_label(draft.spec.kind))
                        .show_ui(ui, |ui| {
                            for &kind in all_widget_kinds() {
                                ui.selectable_value(
                                    &mut draft.spec.kind,
                                    kind,
                                    widget_kind_label(kind),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("Телеметрия:");
                    let selected_label = draft.spec.telemetry.map_or("Нет", telemetry_label);
                    egui::ComboBox::from_id_salt("widget_telemetry_combo")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut draft.spec.telemetry, None, "Нет");
                            for &metric in all_telemetry_bindings() {
                                ui.selectable_value(
                                    &mut draft.spec.telemetry,
                                    Some(metric),
                                    telemetry_label(metric),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("Действие:");
                    egui::ComboBox::from_id_salt("widget_action_combo")
                        .selected_text(action_label(draft.spec.action))
                        .show_ui(ui, |ui| {
                            for &action in all_widget_actions() {
                                ui.selectable_value(
                                    &mut draft.spec.action,
                                    action,
                                    action_label(action),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("Иконка:");
                    let current_icon = draft.spec.icon.map_or("Нет", icon_label);
                    egui::ComboBox::from_id_salt("widget_icon_combo")
                        .selected_text(current_icon)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut draft.spec.icon, None, "Нет");
                            for &icon in all_widget_icons() {
                                ui.selectable_value(
                                    &mut draft.spec.icon,
                                    Some(icon),
                                    icon_label(icon),
                                );
                            }
                        });
                    ui.end_row();
                });

            if matches!(draft.spec.kind, WidgetKind::Text) {
                ui.add_space(6.0);
                let mut text_val = draft.spec.text.clone().unwrap_or_default();
                ui.label("Текст заметки:");
                ui.text_edit_multiline(&mut text_val);
                draft.spec.text = if text_val.is_empty() {
                    None
                } else {
                    Some(text_val)
                };
            }

            if matches!(draft.spec.kind, WidgetKind::Image) {
                ui.add_space(6.0);
                let mut key = draft.spec.asset_key.clone().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label("Ключ ресурса:");
                    ui.text_edit_singleline(&mut key);
                });
                draft.spec.asset_key = if key.trim().is_empty() {
                    None
                } else {
                    Some(key)
                };
            }

            if matches!(draft.spec.kind, WidgetKind::Custom) {
                ui.add_space(6.0);
                let mut comp = draft.spec.custom_component.clone().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label("ID плагина:");
                    ui.text_edit_singleline(&mut comp);
                });
                draft.spec.custom_component = if comp.trim().is_empty() {
                    None
                } else {
                    Some(comp)
                };
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            ui.label(
                RichText::new(format!(
                    "Геометрия: колонка {}, строка {} • размер {}×{}",
                    draft.spec.col, draft.spec.row, draft.spec.col_span, draft.spec.row_span
                ))
                .size(12.5)
                .strong()
                .color(theme.text_secondary),
            );
            ui.horizontal(|ui| {
                ui.label("Позиция:");
                if ui.button("◀").clicked() {
                    move_delta = Some((-1, 0));
                }
                if ui.button("▶").clicked() {
                    move_delta = Some((1, 0));
                }
                if ui.button("▲").clicked() {
                    move_delta = Some((0, -1));
                }
                if ui.button("▼").clicked() {
                    move_delta = Some((0, 1));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Ширина:");
                if ui.button(" − ").clicked() {
                    resize_delta = Some((-1, 0));
                }
                if ui.button(" + ").clicked() {
                    resize_delta = Some((1, 0));
                }
                ui.label("Высота:");
                if ui.button("−").clicked() {
                    resize_delta = Some((0, -1));
                }
                if ui.button("+").clicked() {
                    resize_delta = Some((0, 1));
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            ui.label(
                RichText::new("Цвета и стиль плитки")
                    .size(13.0)
                    .strong()
                    .color(theme.text_primary),
            );
            draw_color_row(ui, "Фон:", &mut draft.spec.style.background_hex);
            draw_color_row(ui, "Текст:", &mut draft.spec.style.foreground_hex);
            draw_color_row(ui, "Акцент:", &mut draft.spec.style.accent_hex);

            ui.horizontal(|ui| {
                ui.label("Шрифт (sp):");
                ui.add(egui::Slider::new(
                    &mut draft.spec.style.font_size_sp,
                    10..=48,
                ));
            });
            ui.horizontal(|ui| {
                ui.label("Скругление:");
                ui.add(egui::Slider::new(
                    &mut draft.spec.style.corner_radius,
                    0..=32,
                ));
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary_button(ui, "Применить изменения", theme).clicked() {
                    apply_edit = true;
                }
                if secondary_button(ui, "Сбросить стиль", theme).clicked() {
                    draft.spec.style = WidgetStyle::default();
                }
            });
            ui.add_space(4.0);
            if danger_button(ui, "Удалить виджет с панели", theme).clicked() {
                delete_clicked = true;
            }
        });

        self.widget_draft = Some(draft.clone());

        if let Some((dx, dy)) = move_delta {
            self.state.move_selected_widget(dx, dy);
            self.force_resync_drafts();
        } else if let Some((dw, dh)) = resize_delta {
            self.state.resize_selected_widget(dw, dh);
            self.force_resync_drafts();
        } else if delete_clicked {
            self.state.delete_selected_widget();
            self.force_resync_drafts();
        } else if apply_edit {
            self.state.update_selected_widget(draft.spec);
            self.force_resync_drafts();
        }
    }

    fn draw_panel_meta_section(&mut self, ui: &mut egui::Ui, theme: ThemePalette) {
        let Some(mut draft) = self.panel_meta_draft.clone() else {
            return;
        };
        let mut save_clicked = false;
        let mut reset_starter = false;

        fluent_card(ui, theme, |ui| {
            ui.label(
                RichText::new("Параметры текущей панели")
                    .size(14.5)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.add_space(6.0);
            egui::Grid::new("panel_meta_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Название:");
                    ui.text_edit_singleline(&mut draft.name);
                    ui.end_row();

                    ui.label("Колонок:");
                    ui.add(
                        egui::Slider::new(
                            &mut draft.columns,
                            MIN_PANEL_COLUMNS..=MAX_PANEL_COLUMNS,
                        )
                        .clamping(egui::SliderClamping::Always),
                    );
                    ui.end_row();

                    ui.label("Интервал (мс):");
                    ui.add(
                        egui::DragValue::new(&mut draft.refresh_interval_ms)
                            .speed(50)
                            .range(MIN_REFRESH_INTERVAL_MS..=MAX_REFRESH_INTERVAL_MS),
                    );
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                save_clicked = primary_button(ui, "Сохранить панель", theme).clicked();
                reset_starter = secondary_button(ui, "Сбросить Deck", theme).clicked();
            });
        });

        self.panel_meta_draft = Some(draft.clone());
        if save_clicked {
            self.state.update_selected_panel_meta(
                draft.name,
                draft.columns,
                draft.refresh_interval_ms,
            );
            self.panel_meta_draft = None;
            self.sync_drafts_from_selection();
        } else if reset_starter {
            self.state.reset_deck_to_starter();
            self.force_resync_drafts();
        }
    }

    fn draw_json_section(&mut self, ui: &mut egui::Ui, theme: ThemePalette) {
        fluent_card(ui, theme, |ui| {
            ui.label(
                RichText::new("Прямой JSON-редактор (panels.json)")
                    .size(14.5)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if secondary_button(ui, "Выгрузить всю деку", theme).clicked() {
                    self.state.export_deck_to_buffer();
                }
                if secondary_button(ui, "Выгрузить текущую панель", theme).clicked()
                {
                    self.state.export_selected_panel_to_buffer();
                }
                if primary_button(ui, "Импортировать деку", theme).clicked() {
                    self.state.import_deck_from_buffer();
                    self.force_resync_drafts();
                }
                if primary_button(ui, "Импортировать панель", theme).clicked() {
                    self.state.import_panel_from_buffer();
                    self.force_resync_drafts();
                }
            });
            ui.add_space(6.0);
            ScrollArea::vertical()
                .id_salt("panel_json_editor")
                .max_height(180.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.state.json_buffer)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .desired_rows(7),
                    );
                });
        });
    }

    fn draw_agent_tab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        running: bool,
        theme: ThemePalette,
    ) {
        // KPI Summary Row
        ui.columns(3, |cols| {
            fluent_card(&mut cols[0], theme, |ui| {
                ui.label(
                    RichText::new("Состояние nexdeck-agent")
                        .size(12.0)
                        .color(theme.text_secondary),
                );
                let (txt, col) = if running {
                    if self.state.agent_process.is_pair_mode() {
                        ("Ожидание сопряжения (PIN)", theme.accent)
                    } else {
                        ("Активен в фоне (без окна)", theme.status_ok)
                    }
                } else {
                    ("Остановлен", theme.status_warn)
                };
                ui.label(RichText::new(txt).size(16.0).strong().color(col));
            });

            fluent_card(&mut cols[1], theme, |ui| {
                ui.label(
                    RichText::new("Сетевой шлюз (WSS + Universal API)")
                        .size(12.0)
                        .color(theme.text_secondary),
                );
                ui.label(
                    RichText::new(&self.state.settings_draft.bind_addr)
                        .size(16.0)
                        .strong()
                        .color(theme.text_primary),
                );
            });

            fluent_card(&mut cols[2], theme, |ui| {
                ui.label(
                    RichText::new("Автозапуск с Windows 11")
                        .size(12.0)
                        .color(theme.text_secondary),
                );
                let (txt, col) = if self.state.autostart_command.is_some() {
                    ("Включён (HKCU\\Run)", theme.status_ok)
                } else {
                    ("Отключён", theme.text_secondary)
                };
                ui.label(RichText::new(txt).size(16.0).strong().color(col));
            });
        });

        ui.add_space(14.0);

        ui.columns(2, |columns| {
            ScrollArea::vertical()
                .id_salt("agent_left_col")
                .show(&mut columns[0], |ui| {
                    fluent_card(ui, theme, |ui| {
                        ui.label(
                            RichText::new("Мастер подключения телефона")
                                .size(15.5)
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.label(
                            RichText::new(
                                "Агент запускается в фоновом режиме без открытия чёрного окна командной строки.",
                            )
                            .size(12.5)
                            .color(theme.text_secondary),
                        );
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.label("Время жизни PIN-кода (сек):");
                            ui.add(
                                egui::DragValue::new(&mut self.state.pair_ttl_secs)
                                    .speed(10)
                                    .range(30..=1800),
                            );
                        });
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            if !running {
                                if primary_button(ui, "🔗 Запустить с PIN и QR", theme).clicked() {
                                    self.state.start_agent(true);
                                }
                                if secondary_button(ui, "▶ Обычный запуск", theme).clicked() {
                                    self.state.start_agent(false);
                                }
                            } else if danger_button(ui, "■ Остановить агент", theme).clicked() {
                                self.state.stop_agent();
                            }
                        });
                        if let Some(note) = self.state.agent_process.last_exit_note() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(note).size(12.0).color(theme.text_secondary));
                        }

                        let (pin_opt, pairing_json_opt) =
                            self.state.agent_process.latest_pairing_info();
                        if let Some(pin) = pin_opt {
                            ui.add_space(10.0);
                            egui::Frame::new()
                                .fill(theme.accent_soft_bg)
                                .stroke(Stroke::new(1.0_f32, theme.accent))
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(Margin::symmetric(16, 12))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new("Одноразовый PIN-код сопряжения")
                                            .size(12.0)
                                            .color(theme.text_secondary),
                                    );
                                    ui.label(
                                        RichText::new(pin)
                                            .size(28.0)
                                            .strong()
                                            .color(theme.accent),
                                    );
                                });
                        }
                        if let Some(json) = pairing_json_opt {
                            ui.add_space(8.0);
                            if primary_button(ui, "Скопировать JSON для мобильного клиента", theme)
                                .clicked()
                            {
                                ctx.copy_text(json.clone());
                            }
                            let mut preview = json;
                            ui.add(
                                egui::TextEdit::multiline(&mut preview)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(5),
                            );
                        }
                    });

                    ui.add_space(12.0);
                    fluent_card(ui, theme, |ui| {
                        ui.label(
                            RichText::new("Автозапуск при входе в систему (без UAC)")
                                .size(15.5)
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(4.0);
                        if let Some(cmd) = &self.state.autostart_command {
                            ui.monospace(cmd);
                        }
                        if let Some(note) = &self.state.autostart_note {
                            ui.label(RichText::new(note).size(12.0).color(theme.text_secondary));
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let enabled = self.state.autostart_command.is_some();
                            if !enabled
                                && primary_button(ui, "Включить автозапуск", theme).clicked()
                            {
                                self.state.set_autostart_enabled(true);
                            }
                            if enabled
                                && danger_button(ui, "Отключить автозапуск", theme).clicked()
                            {
                                self.state.set_autostart_enabled(false);
                            }
                        });
                    });
                });

            ScrollArea::vertical()
                .id_salt("agent_right_col")
                .show(&mut columns[1], |ui| {
                    fluent_card(ui, theme, |ui| {
                        ui.label(
                            RichText::new("Параметры сервера (config.json)")
                                .size(15.5)
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(8.0);
                        egui::Grid::new("config_settings_grid")
                            .num_columns(2)
                            .spacing([12.0, 8.0])
                            .show(ui, |ui| {
                                ui.label("Адрес и порт WSS:");
                                ui.text_edit_singleline(&mut self.state.settings_draft.bind_addr);
                                ui.end_row();

                                ui.label("SAN хосты (TLS):");
                                ui.text_edit_singleline(
                                    &mut self.state.settings_draft.san_hosts_csv,
                                );
                                ui.end_row();

                                ui.label("Опрос телеметрии (мс):");
                                ui.text_edit_singleline(
                                    &mut self.state.settings_draft.poll_interval_ms,
                                );
                                ui.end_row();

                                ui.label("Буфер уведомлений:");
                                ui.text_edit_singleline(
                                    &mut self.state.settings_draft.notification_queue_capacity,
                                );
                                ui.end_row();
                            });

                        ui.add_space(8.0);
                        ui.checkbox(
                            &mut self.state.settings_draft.enable_plugins,
                            "Разрешить изолированный WASM-движок плагинов",
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "Сохранить настройки", theme).clicked() {
                                self.state.save_settings_draft();
                            }
                            if secondary_button(ui, "Сбросить", theme).clicked() {
                                self.state.reset_settings_draft();
                            }
                        });
                    });

                    ui.add_space(12.0);
                    fluent_card(ui, theme, |ui| {
                        ui.label(
                            RichText::new("Журнал событий агента")
                                .size(15.5)
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(6.0);
                        let lines = self.state.agent_process.log_lines();
                        if lines.is_empty() {
                            ui.label(
                                RichText::new("Журнал пуст — запустите агент слева.")
                                    .color(theme.text_secondary),
                            );
                        } else {
                            let mut combined = lines.join("\n");
                            ui.add(
                                egui::TextEdit::multiline(&mut combined)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(12),
                            );
                        }
                    });
                });
        });
    }

    fn draw_devices_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, theme: ThemePalette) {
        ScrollArea::vertical()
            .id_salt("devices_tab_scroll")
            .show(ui, |ui| {
                fluent_card(ui, theme, |ui| {
                    ui.label(
                        RichText::new("Криптографический отпечаток хоста (TLS SHA-256 Pinning)")
                            .size(15.5)
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.add_space(4.0);
                    ui.monospace(&self.state.tls_fingerprint_sha256);
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if primary_button(ui, "Скопировать отпечаток", theme).clicked()
                        {
                            ctx.copy_text(self.state.tls_fingerprint_sha256.clone());
                        }
                        if danger_button(ui, "Перевыпустить TLS-сертификат", theme).clicked()
                        {
                            self.state.rotate_tls_identity();
                        }
                    });
                });

                ui.add_space(12.0);
                fluent_card(ui, theme, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Доверенные мобильные устройства ({})",
                                self.state.devices.len()
                            ))
                            .size(15.5)
                            .strong()
                            .color(theme.text_primary),
                        );
                        if secondary_button(ui, "↻ Обновить", theme).clicked() {
                            self.state.refresh_from_disk();
                        }
                    });
                    ui.add_space(6.0);

                    if self.state.devices.is_empty() {
                        ui.label(
                            RichText::new("Сопряжённых телефонов пока нет.")
                                .color(theme.text_secondary),
                        );
                    } else {
                        let mut revoke_target: Option<String> = None;
                        for record in &self.state.devices {
                            egui::Frame::new()
                                .fill(theme.card_elevated_bg)
                                .stroke(Stroke::new(1.0_f32, theme.subtle_border))
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(12, 8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} ({})",
                                                    record.device_name, record.device_id
                                                ))
                                                .strong()
                                                .color(theme.text_primary),
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "SHA-256 токена: {}…",
                                                    &record.token_hash_sha256
                                                        [..record.token_hash_sha256.len().min(16)]
                                                ))
                                                .size(11.5)
                                                .color(theme.text_secondary),
                                            );
                                        });
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if danger_button(ui, "Отозвать доступ", theme)
                                                    .clicked()
                                                {
                                                    revoke_target = Some(record.device_id.clone());
                                                }
                                            },
                                        );
                                    });
                                });
                            ui.add_space(6.0);
                        }
                        if let Some(id) = revoke_target {
                            self.state.revoke_device(&id);
                        }
                    }
                });

                ui.add_space(12.0);
                self.draw_api_clients_section(ui, ctx, theme);
            });
    }

    fn draw_api_clients_section(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        theme: ThemePalette,
    ) {
        fluent_card(ui, theme, |ui| {
            ui.label(
                RichText::new(format!(
                    "Сторонние приложения Universal API ({}, clients.json)",
                    self.state.api_clients.len()
                ))
                .size(15.5)
                .strong()
                .color(theme.text_primary),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Client ID:");
                ui.text_edit_singleline(&mut self.state.new_api_client_id);
                ui.label("Приложение:");
                ui.text_edit_singleline(&mut self.state.new_api_client_name);
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Разрешения:");
                for (idx, perm) in ApiPermission::ALL.iter().copied().enumerate() {
                    ui.checkbox(
                        &mut self.state.new_api_client_permissions[idx],
                        perm.as_str(),
                    );
                }
                if primary_button(ui, "+ Выпустить токен клиента", theme).clicked()
                {
                    self.state.register_api_client();
                }
            });

            if let Some(issued) = self.state.latest_issued_api_client.clone() {
                ui.add_space(8.0);
                egui::Frame::new()
                    .fill(theme.accent_soft_bg)
                    .stroke(Stroke::new(1.0_f32, theme.accent))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "Одноразовый токен для '{}' (сохраните сейчас):",
                                issued.client_id
                            ))
                            .strong()
                            .color(theme.accent),
                        );
                        ui.monospace(&issued.access_secret);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "Скопировать токен", theme).clicked()
                            {
                                ctx.copy_text(issued.access_secret.clone());
                            }
                            if secondary_button(ui, "Скрыть", theme).clicked() {
                                self.state.latest_issued_api_client = None;
                            }
                        });
                    });
            }

            if !self.state.api_clients.is_empty() {
                ui.add_space(10.0);
                let mut toggle_revoke: Option<(String, bool)> = None;
                let mut delete_target: Option<String> = None;

                for client in &self.state.api_clients {
                    egui::Frame::new()
                        .fill(theme.card_elevated_bg)
                        .stroke(Stroke::new(1.0_f32, theme.subtle_border))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let status_color = if client.revoked {
                                    theme.status_err
                                } else {
                                    theme.status_ok
                                };
                                let perms_text = client
                                    .permissions
                                    .iter()
                                    .map(|p| p.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                ui.label(
                                    RichText::new(format!(
                                        "{} [{}] ({})",
                                        client.application_name,
                                        client.client_id,
                                        if client.revoked {
                                            "ОТОЗВАН"
                                        } else {
                                            "АКТИВЕН"
                                        }
                                    ))
                                    .color(status_color)
                                    .strong(),
                                );
                                ui.label(
                                    RichText::new(format!("Права: [{perms_text}]"))
                                        .size(12.0)
                                        .color(theme.text_secondary),
                                );
                                if client.revoked {
                                    if secondary_button(ui, "Восстановить", theme).clicked()
                                    {
                                        toggle_revoke = Some((client.client_id.clone(), false));
                                    }
                                } else if secondary_button(ui, "Отозвать", theme).clicked()
                                {
                                    toggle_revoke = Some((client.client_id.clone(), true));
                                }
                                if danger_button(ui, "Удалить", theme).clicked() {
                                    delete_target = Some(client.client_id.clone());
                                }
                            });
                        });
                    ui.add_space(6.0);
                }

                if let Some((id, revoked)) = toggle_revoke {
                    self.state.set_api_client_revoked(&id, revoked);
                }
                if let Some(id) = delete_target {
                    self.state.delete_api_client(&id);
                }
            }
        });
    }
}

fn fluent_card(ui: &mut egui::Ui, theme: ThemePalette, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme.card_bg)
        .stroke(Stroke::new(1.0_f32, theme.card_border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

fn primary_button(ui: &mut egui::Ui, label: &str, theme: ThemePalette) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
            .fill(theme.accent)
            .corner_radius(CornerRadius::same(8)),
    )
}

fn secondary_button(ui: &mut egui::Ui, label: &str, theme: ThemePalette) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(theme.text_primary))
            .fill(theme.card_elevated_bg)
            .stroke(Stroke::new(1.0_f32, theme.card_border))
            .corner_radius(CornerRadius::same(8)),
    )
}

fn danger_button(ui: &mut egui::Ui, label: &str, theme: ThemePalette) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).strong().color(theme.status_err))
            .fill(Color32::from_rgba_unmultiplied(
                theme.status_err.r(),
                theme.status_err.g(),
                theme.status_err.b(),
                26,
            ))
            .stroke(Stroke::new(1.0_f32, theme.status_err))
            .corner_radius(CornerRadius::same(8)),
    )
}

fn draw_sidebar_nav_item(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    tab: DesktopTab,
    selected: bool,
    title: &str,
    subtitle: &str,
    theme: ThemePalette,
) -> bool {
    let desired = Vec2::new(ui.available_width(), 52.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let id = egui::Id::new(("sidebar_nav", tab as u8));
    let sel_t = ctx.animate_bool_with_time(id.with("sel"), selected, ANIM_DURATION);
    let hov_t = ctx.animate_bool_with_time(id.with("hov"), response.hovered(), 0.12);

    let painter = ui.painter_at(rect);
    if sel_t > 0.01 || hov_t > 0.01 {
        let fill = if selected {
            theme.accent_soft_bg
        } else {
            theme.card_elevated_bg
        };
        painter.rect_filled(rect, CornerRadius::same(10), fill);
    }

    if sel_t > 0.02 {
        let bar_h = 24.0 * sel_t;
        let bar_rect = Rect::from_center_size(
            Pos2::new(rect.left() + 3.0, rect.center().y),
            Vec2::new(3.5, bar_h),
        );
        painter.rect_filled(bar_rect, CornerRadius::same(2), theme.accent);
    }

    let title_color = if selected {
        theme.accent
    } else {
        theme.text_primary
    };
    painter.text(
        rect.left_top() + Vec2::new(14.0, 9.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(14.0),
        title_color,
    );
    painter.text(
        rect.left_bottom() + Vec2::new(14.0, -8.0),
        Align2::LEFT_BOTTOM,
        subtitle,
        FontId::proportional(11.0),
        theme.text_secondary,
    );

    response.clicked()
}

fn draw_color_row(ui: &mut egui::Ui, label: &str, hex_value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        if let Some(mut rgba) = parse_hex_rgba(hex_value)
            && ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed()
        {
            *hex_value = format_hex_rgba(rgba);
        }
        ui.text_edit_singleline(hex_value);
    });
}

fn configure_fonts(ctx: &egui::Context) {
    let windows_font = fs::read(r"C:\Windows\Fonts\segoeui.ttf");
    if let Ok(bytes) = windows_font {
        let mut fonts = FontDefinitions::default();
        fonts
            .font_data
            .insert("segoeui".to_owned(), Arc::new(FontData::from_owned(bytes)));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoeui".to_owned());
        ctx.set_fonts(fonts);
    }
}

fn apply_clean_modern_theme(ctx: &egui::Context, theme: ThemePalette) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 9.0);
    style.spacing.button_padding = Vec2::new(13.0, 7.0);
    style.spacing.window_margin = Margin::same(16);

    let mut text_styles = BTreeMap::new();
    text_styles.insert(TextStyle::Heading, FontId::proportional(18.0));
    text_styles.insert(TextStyle::Body, FontId::proportional(13.5));
    text_styles.insert(TextStyle::Button, FontId::proportional(13.5));
    text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
    text_styles.insert(TextStyle::Monospace, FontId::monospace(12.5));
    style.text_styles = text_styles;

    let mut visuals = if theme.is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = theme.canvas_bg;
    visuals.window_fill = theme.card_bg;
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.selection.bg_fill = theme.accent;
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);

    visuals.widgets.noninteractive.bg_fill = theme.card_bg;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, theme.text_primary);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, theme.card_border);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);

    visuals.widgets.inactive.bg_fill = theme.card_elevated_bg;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, theme.text_primary);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, theme.card_border);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);

    visuals.widgets.hovered.bg_fill = theme.accent_soft_bg;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, theme.text_primary);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, theme.accent);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);

    visuals.widgets.active.bg_fill = theme.accent;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);

    style.visuals = visuals;
    ctx.set_style(style);
}

/// Parses `#RRGGBB` or `#AARRGGBB` hex colors into unmultiplied `[r, g, b, a]` bytes.
#[must_use]
pub fn parse_hex_rgba(raw: &str) -> Option<[u8; 4]> {
    let hex = raw.strip_prefix('#')?;
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([r, g, b, u8::MAX])
        }
        8 => {
            let a = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let r = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let g = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let b = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some([r, g, b, a])
        }
        _ => None,
    }
}

/// Parses `#RRGGBB` or `#AARRGGBB` hex colors used by [`WidgetStyle`] into [`Color32`].
#[must_use]
pub fn parse_hex_color(raw: &str) -> Option<Color32> {
    let [r, g, b, a] = parse_hex_rgba(raw)?;
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// Formats unmultiplied `[r, g, b, a]` bytes back into `#RRGGBB` or `#AARRGGBB`.
#[must_use]
pub fn format_hex_rgba(rgba: [u8; 4]) -> String {
    let [r, g, b, a] = rgba;
    if a == u8::MAX {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{a:02X}{r:02X}{g:02X}{b:02X}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_color_roundtrip_handles_rgb_and_argb() {
        let rgb = parse_hex_rgba("#38BDF8").expect("parse 6-digit hex");
        assert_eq!(rgb, [0x38, 0xBD, 0xF8, 0xFF]);
        assert_eq!(format_hex_rgba(rgb), "#38BDF8");

        let argb = parse_hex_rgba("#801E293B").expect("parse 8-digit hex");
        assert_eq!(argb, [0x1E, 0x29, 0x3B, 0x80]);
        assert_eq!(format_hex_rgba(argb), "#801E293B");

        assert!(parse_hex_rgba("38BDF8").is_none());
        assert!(parse_hex_rgba("#12345").is_none());
        assert!(parse_hex_color("#38BDF8").is_some());
    }
}
