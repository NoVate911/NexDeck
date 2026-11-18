//! Pure-Rust Desktop Manager GUI implemented with `eframe` / `egui`.
//!
//! Designed as an intuitive consumer studio with:
//! - 1-click phone pairing and real vector QR code rendering (`QrMatrix`),
//! - True mouse Drag-and-Drop from the widget catalog onto a smartphone canvas,
//! - Direct mouse dragging to move/swap widgets on the grid and corner-handle resizing,
//! - Instant auto-save and 1-click color theme swatches,
//! - Custom vector icons (`draw_vector_icon`) so no system emoji font is required.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily,
    FontId, Id, LayerId, Layout, Margin, Order, Pos2, Rect, RichText, ScrollArea, Sense, Shape,
    Stroke, StrokeKind, TextStyle, Vec2,
};
use nexdeck_api::panels::{
    MAX_PANEL_COLUMNS, MIN_PANEL_COLUMNS, WidgetIcon, WidgetKind, WidgetSpec,
};
use nexdeck_core::AGENT_VERSION;
use nexdeck_notifications::{NotificationPriority, NotificationSource};

use crate::qr::QrMatrix;
use crate::state::{
    DesktopManagerState, DesktopTab, WidgetPreset, action_label, all_telemetry_bindings,
    all_widget_actions, all_widget_icons, all_widget_kinds, detect_lan_ipv4, icon_label,
    resolve_agent_exe_in_dir, telemetry_label, widget_kind_label,
};

const GRID_CELL_GAP: f32 = 10.0;
const GRID_MIN_ROWS: u16 = 4;
const ANIM_DURATION: f32 = 0.18;

/// Active mouse drag operation inside the Deck Constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DragPayload {
    /// Dragging a ready-made template from the left catalog onto the grid.
    CatalogPreset(WidgetPreset),
    /// Dragging an existing widget across the grid to move or swap it.
    MoveWidget {
        widget_id: String,
        title: String,
        col_span: u16,
        row_span: u16,
        accent_hex: String,
    },
    /// Dragging the bottom-right corner handle of a widget to resize `(col_span, row_span)`.
    ResizeWidget {
        widget_id: String,
        origin_col: u16,
        origin_row: u16,
    },
}

/// Curated 1-click color swatches for widget styling.
struct ColorSwatch {
    name: &'static str,
    bg_hex: &'static str,
    accent_hex: &'static str,
}

const COLOR_SWATCHES: &[ColorSwatch] = &[
    ColorSwatch {
        name: "Океан",
        bg_hex: "#162238",
        accent_hex: "#38BDF8",
    },
    ColorSwatch {
        name: "Индиго",
        bg_hex: "#1D2D50",
        accent_hex: "#60A5FA",
    },
    ColorSwatch {
        name: "Изумруд",
        bg_hex: "#112A26",
        accent_hex: "#10B981",
    },
    ColorSwatch {
        name: "Фиолет",
        bg_hex: "#251838",
        accent_hex: "#A855F7",
    },
    ColorSwatch {
        name: "Бирюза",
        bg_hex: "#132A2E",
        accent_hex: "#2DD4BF",
    },
    ColorSwatch {
        name: "Янтарь",
        bg_hex: "#2E2112",
        accent_hex: "#F59E0B",
    },
    ColorSwatch {
        name: "Рубин",
        bg_hex: "#311820",
        accent_hex: "#F87171",
    },
    ColorSwatch {
        name: "Графит",
        bg_hex: "#1E2636",
        accent_hex: "#94A3B8",
    },
];

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
    show_pairing_modal: bool,
    show_advanced_settings: bool,
    active_drag: Option<DragPayload>,
    selected_empty_cell: Option<(u16, u16)>,
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
    /// Initializes visual theme, optional Windows Cyrillic font fallback, and auto-starts agent if present.
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>, mut state: DesktopManagerState) -> Self {
        configure_fonts(&cc.egui_ctx);
        apply_clean_modern_theme(&cc.egui_ctx, ThemePalette::fluent_dark());

        // Automatically start the background agent if its binary sits next to nexdeck-desktop.exe.
        if !state.agent_process.is_running()
            && std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().and_then(resolve_agent_exe_in_dir))
                .is_some()
        {
            state.connect_phone_one_click();
        }

        let mut app = Self {
            state,
            widget_draft: None,
            panel_meta_draft: None,
            dark_mode: true,
            show_pairing_modal: false,
            show_advanced_settings: false,
            active_drag: None,
            selected_empty_cell: None,
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
                    .is_none_or(|draft| draft.panel_id != panel_id || draft.spec != spec);
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
        if running || self.active_drag.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
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
                    .inner_margin(Margin::symmetric(24, 18)),
            )
            .show(ctx, |ui| {
                self.draw_page_header(ui, running, theme);
                ui.add_space(12.0);

                match self.state.active_tab {
                    DesktopTab::Panels => self.draw_panels_tab(ui, ctx, theme),
                    DesktopTab::AgentSettings => self.draw_connection_tab(ui, ctx, running, theme),
                    DesktopTab::DevicesAndTls => self.draw_devices_tab(ui, ctx, theme),
                }
            });

        if self.show_pairing_modal {
            self.draw_pairing_modal_window(ctx, theme);
        }

        // Clear any finished drag if mouse button is no longer held down.
        if self.active_drag.is_some() && !ctx.input(|i| i.pointer.any_down()) {
            self.active_drag = None;
        }
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
                        .size(17.5)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    RichText::new(format!("Версия {AGENT_VERSION}"))
                        .size(11.5)
                        .color(theme.text_secondary),
                );
            });
        });

        ui.add_space(16.0);

        // 1-Click Hero Button: Connect Phone
        let connect_btn = egui::Button::new(
            RichText::new("Подключить телефон")
                .size(14.5)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(theme.accent)
        .corner_radius(CornerRadius::same(10))
        .min_size(Vec2::new(ui.available_width(), 42.0));

        if ui.add(connect_btn).clicked() {
            self.state.connect_phone_one_click();
            self.show_pairing_modal = true;
        }

        ui.add_space(10.0);

        // Compact Live Agent Status Pill
        fluent_card_compact(ui, theme, |ui| {
            let anim_run = ctx.animate_bool_with_time(
                Id::new("agent_status_dot"),
                agent_running,
                ANIM_DURATION,
            );
            let (dot_color, status_title) = if agent_running {
                (theme.status_ok, "Сервер активен в фоне")
            } else {
                (theme.status_warn, "Сервер остановлен")
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
                            52,
                        ),
                    );
                }
                painter.circle_filled(dot_rect.center(), 4.5, dot_color);
                ui.label(
                    RichText::new(status_title)
                        .size(12.5)
                        .strong()
                        .color(theme.text_primary),
                );
            });

            if let Some((pin, host_port, _)) = self.state.resolved_pairing_qr_payload() {
                ui.label(
                    RichText::new(format!("PIN: {pin} • {host_port}"))
                        .size(11.5)
                        .color(theme.accent),
                );
            } else if let Some(lan_ip) = detect_lan_ipv4() {
                ui.label(
                    RichText::new(format!("Сеть Wi-Fi: {lan_ip}"))
                        .size(11.5)
                        .color(theme.text_secondary),
                );
            }
        });

        ui.add_space(18.0);
        ui.label(
            RichText::new("РАЗДЕЛЫ")
                .size(11.0)
                .strong()
                .color(theme.text_secondary),
        );
        ui.add_space(6.0);

        let nav_items = [
            (
                DesktopTab::Panels,
                NavIconKind::Grid,
                "Конструктор экрана",
                "Перетаскивание виджетов мышью",
            ),
            (
                DesktopTab::AgentSettings,
                NavIconKind::Phone,
                "Подключение и ПК",
                "QR-код, PIN и автозапуск",
            ),
            (
                DesktopTab::DevicesAndTls,
                NavIconKind::Shield,
                "Устройства и доступ",
                "Телефоны и сторонние программы",
            ),
        ];

        for (tab, icon_kind, title, subtitle) in nav_items {
            if draw_sidebar_nav_item(
                ui,
                ctx,
                tab,
                icon_kind,
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
            ui.horizontal(|ui| {
                let theme_label = if self.dark_mode {
                    "Светлая тема"
                } else {
                    "Тёмная тема"
                };
                if secondary_button(ui, theme_label, theme).clicked() {
                    self.dark_mode = !self.dark_mode;
                    apply_clean_modern_theme(ctx, self.palette());
                }
                if secondary_button(ui, "Обновить", theme).clicked() {
                    self.state.refresh_from_disk();
                    self.force_resync_drafts();
                }
            });
        });
    }

    fn draw_page_header(&mut self, ui: &mut egui::Ui, running: bool, theme: ThemePalette) {
        let (title, subtitle) = match self.state.active_tab {
            DesktopTab::Panels => (
                "Конструктор экрана телефона",
                "Перетащите любой виджет мышью из каталога слева прямо на экран смартфона",
            ),
            DesktopTab::AgentSettings => (
                "Подключение телефона и работа в фоне",
                "Отсканируйте QR-код или введите 6-значный PIN-код в мобильном приложении NexDeck",
            ),
            DesktopTab::DevicesAndTls => (
                "Подключённые устройства и приложения",
                "Управление доступом телефонов и внешних плагинов",
            ),
        };

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(
                    RichText::new(title)
                        .size(21.0)
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
                if !running {
                    if primary_button(ui, "Запустить фоновый сервер", theme).clicked()
                    {
                        self.state.connect_phone_one_click();
                    }
                } else if secondary_button(ui, "Показать QR и PIN", theme).clicked() {
                    self.state.connect_phone_one_click();
                    self.show_pairing_modal = true;
                }
            });
        });

        // Friendly dismissible notification bar
        if self.state.error_message.is_some() || self.state.status_message.is_some() {
            ui.add_space(8.0);
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
                    24,
                ))
                .stroke(Stroke::new(1.0_f32, banner_color))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(14, 7))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&text).size(13.0).strong().color(banner_color));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("Закрыть").clicked() {
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

    fn draw_pairing_modal_window(&mut self, ctx: &egui::Context, theme: ThemePalette) {
        let mut open = self.show_pairing_modal;
        egui::Window::new("Подключение телефона к ПК")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(480.0, 440.0))
            .frame(
                egui::Frame::new()
                    .fill(theme.card_bg)
                    .stroke(Stroke::new(1.5_f32, theme.accent))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                self.draw_pairing_card_contents(ui, ctx, theme);
            });
        self.show_pairing_modal = open;
    }

    fn draw_pairing_card_contents(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        theme: ThemePalette,
    ) {
        if let Some((pin, host_port, qr_json)) = self.state.resolved_pairing_qr_payload() {
            ui.columns(2, |cols| {
                // Left: Real Vector QR Code on crisp white card
                cols[0].vertical_centered(|ui| {
                    draw_qr_code_widget(ui, &qr_json, 196.0);
                    ui.add_space(6.0);
                    if secondary_button(ui, "Скопировать код подключения", theme).clicked() {
                        ctx.copy_text(qr_json);
                    }
                });

                // Right: Huge PIN & 3 simple steps
                cols[1].vertical(|ui| {
                    ui.label(
                        RichText::new("ПИН-КОД ДЛЯ ТЕЛЕФОНА")
                            .size(11.5)
                            .strong()
                            .color(theme.text_secondary),
                    );
                    let formatted_pin = if pin.len() == 6 {
                        format!("{} {}", &pin[0..3], &pin[3..6])
                    } else {
                        pin
                    };
                    ui.label(
                        RichText::new(formatted_pin)
                            .size(34.0)
                            .strong()
                            .color(theme.accent),
                    );

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Адрес компьютера в сети:")
                            .size(11.5)
                            .color(theme.text_secondary),
                    );
                    ui.label(
                        RichText::new(&host_port)
                            .size(15.0)
                            .strong()
                            .color(theme.text_primary),
                    );

                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Как подключиться:")
                            .size(13.0)
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        RichText::new("1. Откройте NexDeck на телефоне\n2. Введите адрес и 6 цифр ПИН-кода\n3. Или вставьте скопированный код")
                            .size(12.5)
                            .color(theme.text_secondary),
                    );

                    ui.add_space(10.0);
                    if secondary_button(ui, "Выпустить новый ПИН-код", theme).clicked() {
                        self.state.agent_process.stop();
                        self.state.connect_phone_one_click();
                    }
                });
            });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.label(
                    RichText::new("Генерация кода подключения…")
                        .size(16.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.add_space(8.0);
                if primary_button(ui, "Создать ПИН и QR-код", theme).clicked() {
                    self.state.connect_phone_one_click();
                }
            });
        }
    }

    fn draw_panels_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, theme: ThemePalette) {
        // Top Bar: Screen Profile Switcher
        fluent_card_compact(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Экраны:")
                        .size(13.0)
                        .strong()
                        .color(theme.text_secondary),
                );

                let panel_headers: Vec<(usize, String, usize)> = self
                    .state
                    .deck
                    .panels
                    .iter()
                    .enumerate()
                    .map(|(idx, panel)| (idx, panel.name.clone(), panel.widgets.len()))
                    .collect();

                for (idx, name, widget_count) in panel_headers {
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
                        .hint_text("Название нового экрана…")
                        .desired_width(150.0),
                );
                if primary_button(ui, "+ Новый экран", theme).clicked() {
                    self.state.add_panel();
                    self.force_resync_drafts();
                }

                if self.state.deck.panels.len() > 1
                    && danger_button(ui, "Удалить экран", theme).clicked()
                {
                    self.state.delete_selected_panel();
                    self.force_resync_drafts();
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if secondary_button(ui, "Сбросить по умолчанию", theme).clicked()
                    {
                        self.state.reset_deck_to_starter();
                        self.force_resync_drafts();
                    }
                });
            });
        });

        ui.add_space(12.0);

        // 3-Column Studio Workspace:
        // Left (265px): Drag-and-Drop Catalog of Ready Widgets
        // Center (Fluid): Realistic Smartphone Screen Mockup with Drag-and-Drop & Corner Resize
        // Right (320px): Instant Property & Color Swatch Inspector
        let avail_w = ui.available_width();
        let avail_h = ui.available_height();
        let left_w = 264.0_f32;
        let right_w = 324.0_f32.min(avail_w * 0.34);
        let center_w = (avail_w - left_w - right_w - 24.0).max(280.0);

        ui.horizontal_top(|ui| {
            // LEFT: Ready-Made Widget Catalog (Click or Drag!)
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, avail_h),
                Layout::top_down(Align::Min),
                |ui| {
                    self.draw_preset_catalog(ui, theme);
                },
            );

            ui.add_space(12.0);

            // CENTER: Interactive Smartphone Screen Canvas
            ui.allocate_ui_with_layout(
                Vec2::new(center_w, avail_h),
                Layout::top_down(Align::Min),
                |ui| {
                    ScrollArea::vertical()
                        .id_salt("center_phone_scroll")
                        .show(ui, |ui| {
                            self.draw_grid_canvas(ui, ctx, theme);
                        });
                },
            );

            ui.add_space(12.0);

            // RIGHT: Instant Auto-Saving Inspector
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, avail_h),
                Layout::top_down(Align::Min),
                |ui| {
                    ScrollArea::vertical()
                        .id_salt("right_inspector_scroll")
                        .show(ui, |ui| {
                            self.draw_widget_inspector(ui, theme);
                            ui.add_space(10.0);
                            self.draw_panel_meta_section(ui, theme);
                        });
                },
            );
        });

        // Floating Ghost Preview while dragging with the mouse
        if let Some(drag) = &self.active_drag
            && let Some(pos) = ctx.pointer_interact_pos()
        {
            ctx.set_cursor_icon(CursorIcon::Grabbing);
            let (label, accent) = match drag {
                DragPayload::CatalogPreset(preset) => (preset.title().to_owned(), theme.accent),
                DragPayload::MoveWidget {
                    title, accent_hex, ..
                } => (
                    title.clone(),
                    parse_hex_color(accent_hex).unwrap_or(theme.accent),
                ),
                DragPayload::ResizeWidget { .. } => ("Изменение размера…".to_owned(), theme.accent),
            };

            let ghost_rect =
                Rect::from_center_size(pos + Vec2::new(14.0, 14.0), Vec2::new(150.0, 44.0));
            let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("drag_ghost")));
            painter.rect_filled(
                ghost_rect,
                CornerRadius::same(10),
                Color32::from_rgba_unmultiplied(23, 30, 43, 230),
            );
            painter.rect_stroke(
                ghost_rect,
                CornerRadius::same(10),
                Stroke::new(2.0_f32, accent),
                StrokeKind::Inside,
            );
            painter.text(
                ghost_rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(13.5),
                Color32::WHITE,
            );
        }
    }

    fn draw_preset_catalog(&mut self, ui: &mut egui::Ui, theme: ThemePalette) {
        fluent_card(ui, theme, |ui| {
            ui.label(
                RichText::new("Каталог виджетов")
                    .size(15.5)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.label(
                RichText::new("Перетащите мышью на экран или нажмите для добавления")
                    .size(11.5)
                    .color(theme.text_secondary),
            );
            ui.add_space(8.0);

            ScrollArea::vertical()
                .id_salt("preset_catalog_scroll")
                .show(ui, |ui| {
                    for &preset in WidgetPreset::all() {
                        let desired = Vec2::new(ui.available_width(), 48.0);
                        let (rect, response) =
                            ui.allocate_exact_size(desired, Sense::click_and_drag());

                        let painter = ui.painter_at(rect);
                        let bg = if response.hovered() {
                            theme.accent_soft_bg
                        } else {
                            theme.card_elevated_bg
                        };
                        let border = if response.hovered() {
                            theme.accent
                        } else {
                            theme.subtle_border
                        };
                        painter.rect_filled(rect, CornerRadius::same(10), bg);
                        painter.rect_stroke(
                            rect,
                            CornerRadius::same(10),
                            Stroke::new(1.0_f32, border),
                            StrokeKind::Inside,
                        );

                        // Vector icon box on the left
                        let icon_rect = Rect::from_min_size(
                            rect.left_top() + Vec2::new(8.0, 8.0),
                            Vec2::new(32.0, 32.0),
                        );
                        painter.rect_filled(
                            icon_rect,
                            CornerRadius::same(8),
                            Color32::from_rgba_unmultiplied(
                                theme.accent.r(),
                                theme.accent.g(),
                                theme.accent.b(),
                                34,
                            ),
                        );
                        draw_vector_icon(&painter, icon_rect.center(), preset.icon(), theme.accent);

                        painter.text(
                            rect.left_top() + Vec2::new(48.0, 8.0),
                            Align2::LEFT_TOP,
                            preset.title(),
                            FontId::proportional(13.5),
                            theme.text_primary,
                        );
                        painter.text(
                            rect.left_bottom() + Vec2::new(48.0, -7.0),
                            Align2::LEFT_BOTTOM,
                            preset.subtitle(),
                            FontId::proportional(11.0),
                            theme.text_secondary,
                        );

                        if response.hovered() {
                            ui.ctx().set_cursor_icon(CursorIcon::Grab);
                        }
                        if response.drag_started() {
                            self.active_drag = Some(DragPayload::CatalogPreset(preset));
                        }
                        if response.clicked() {
                            self.state.add_preset_at(preset, self.selected_empty_cell);
                            self.selected_empty_cell = None;
                            self.force_resync_drafts();
                        }

                        ui.add_space(6.0);
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
            let rows = (max_bottom + 1).max(GRID_MIN_ROWS + 1);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Экран «{}»", panel.name))
                        .size(15.5)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(
                            "Перетаскивайте плитки мышью • Тяните правый нижний угол для размера",
                        )
                        .size(11.5)
                        .color(theme.text_secondary),
                    );
                });
            });
            ui.add_space(10.0);

            let avail_width = ui.available_width().max(260.0);
            let bezel_pad = 14.0_f32;
            let inner_w = avail_width - bezel_pad * 2.0;
            let cell_w =
                ((inner_w - GRID_CELL_GAP * f32::from(cols + 1)) / f32::from(cols)).max(52.0);
            let cell_h = (cell_w * 0.78).clamp(68.0, 112.0);
            let top_notch_h = 22.0_f32;
            let grid_h = GRID_CELL_GAP + f32::from(rows) * (cell_h + GRID_CELL_GAP);
            let phone_h = top_notch_h + grid_h + bezel_pad * 2.0;

            let (phone_rect, _) =
                ui.allocate_exact_size(Vec2::new(avail_width, phone_h), Sense::hover());
            let painter = ui.painter_at(phone_rect);

            // Smartphone Outer Bezel
            painter.rect_filled(phone_rect, CornerRadius::same(24), theme.sidebar_bg);
            painter.rect_stroke(
                phone_rect,
                CornerRadius::same(24),
                Stroke::new(2.0_f32, theme.card_border),
                StrokeKind::Inside,
            );

            // Speaker / Dynamic Island Pill at Top of Phone Bezel
            let notch_rect = Rect::from_center_size(
                Pos2::new(phone_rect.center().x, phone_rect.top() + 14.0),
                Vec2::new(76.0, 6.0),
            );
            painter.rect_filled(notch_rect, CornerRadius::same(3), theme.card_border);

            let canvas_rect = Rect::from_min_size(
                Pos2::new(
                    phone_rect.left() + bezel_pad,
                    phone_rect.top() + bezel_pad + top_notch_h,
                ),
                Vec2::new(inner_w, grid_h),
            );
            painter.rect_filled(canvas_rect, CornerRadius::same(16), theme.canvas_bg);

            let pointer_pos = ctx.pointer_interact_pos();
            let hovered_cell: Option<(u16, u16)> = pointer_pos.and_then(|pos| {
                if !canvas_rect.contains(pos) {
                    return None;
                }
                let rel_x = (pos.x - canvas_rect.left() - GRID_CELL_GAP).max(0.0);
                let rel_y = (pos.y - canvas_rect.top() - GRID_CELL_GAP).max(0.0);
                let c = (rel_x / (cell_w + GRID_CELL_GAP)).floor() as u16;
                let r = (rel_y / (cell_h + GRID_CELL_GAP)).floor() as u16;
                Some((c.min(cols.saturating_sub(1)), r.min(rows.saturating_sub(1))))
            });

            // Empty Grid Slot Guides + Click-to-Select Empty Slot
            for row in 0..rows {
                for col in 0..cols {
                    let x = canvas_rect.left()
                        + GRID_CELL_GAP
                        + f32::from(col) * (cell_w + GRID_CELL_GAP);
                    let y = canvas_rect.top()
                        + GRID_CELL_GAP
                        + f32::from(row) * (cell_h + GRID_CELL_GAP);
                    let cell_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h));
                    let is_sel_empty = self.selected_empty_cell == Some((col, row));

                    painter.rect_stroke(
                        cell_rect,
                        CornerRadius::same(10),
                        Stroke::new(
                            if is_sel_empty { 1.8_f32 } else { 1.0_f32 },
                            if is_sel_empty {
                                theme.accent
                            } else {
                                theme.subtle_border
                            },
                        ),
                        StrokeKind::Inside,
                    );

                    // Plus hint on empty selected slot
                    if is_sel_empty {
                        painter.text(
                            cell_rect.center(),
                            Align2::CENTER_CENTER,
                            "+ Выберите виджет слева",
                            FontId::proportional(10.5),
                            theme.accent,
                        );
                    }
                }
            }

            // Draw Live Drop / Resize Target Preview when dragging
            if let (Some(drag), Some((hc, hr))) = (&self.active_drag, hovered_cell) {
                match drag {
                    DragPayload::CatalogPreset(preset) => {
                        let (raw_w, raw_h) = preset.default_spans();
                        let w_span = raw_w.min(cols).max(1);
                        let h_span = raw_h.max(1);
                        let clamped_c = hc.min(cols.saturating_sub(w_span));
                        let valid = self
                            .state
                            .can_drop_widget_at(None, clamped_c, hr, w_span, h_span);
                        draw_drop_preview(
                            &painter,
                            canvas_rect,
                            cell_w,
                            cell_h,
                            clamped_c,
                            hr,
                            w_span,
                            h_span,
                            valid,
                            theme,
                        );
                    }
                    DragPayload::MoveWidget {
                        widget_id,
                        col_span,
                        row_span,
                        ..
                    } => {
                        let clamped_c = hc.min(cols.saturating_sub(*col_span));
                        let valid = self.state.can_drop_widget_at(
                            Some(widget_id),
                            clamped_c,
                            hr,
                            *col_span,
                            *row_span,
                        );
                        draw_drop_preview(
                            &painter,
                            canvas_rect,
                            cell_w,
                            cell_h,
                            clamped_c,
                            hr,
                            *col_span,
                            *row_span,
                            valid,
                            theme,
                        );
                    }
                    DragPayload::ResizeWidget {
                        widget_id,
                        origin_col,
                        origin_row,
                    } => {
                        let new_w = hc.saturating_sub(*origin_col).saturating_add(1).max(1);
                        let new_h = hr.saturating_sub(*origin_row).saturating_add(1).max(1);
                        let valid = self.state.can_drop_widget_at(
                            Some(widget_id),
                            *origin_col,
                            *origin_row,
                            new_w,
                            new_h,
                        );
                        draw_drop_preview(
                            &painter,
                            canvas_rect,
                            cell_w,
                            cell_h,
                            *origin_col,
                            *origin_row,
                            new_w,
                            new_h,
                            valid,
                            theme,
                        );
                    }
                }
            }

            // Render Placed Widgets + Drag & Resize Handles
            let mut clicked_widget_id: Option<String> = None;
            let mut delete_widget_id: Option<String> = None;
            let mut started_drag: Option<DragPayload> = None;

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

                // Corner resize handle rect (bottom-right 20x20)
                let resize_rect = Rect::from_min_size(
                    base_rect.right_bottom() - Vec2::new(20.0, 20.0),
                    Vec2::new(20.0, 20.0),
                );
                // Delete button rect (top-right 20x20 when selected)
                let delete_rect = Rect::from_min_size(
                    Pos2::new(base_rect.right() - 22.0, base_rect.top() + 4.0),
                    Vec2::new(18.0, 18.0),
                );

                let selected = self
                    .state
                    .selected_widget_id
                    .as_deref()
                    .is_some_and(|sel_id| sel_id == widget.id);

                let resize_id = ui.make_persistent_id(("widget_resize", &widget.id));
                let resize_resp = ui.interact(resize_rect, resize_id, Sense::drag());

                let del_id = ui.make_persistent_id(("widget_del", &widget.id));
                let del_resp = if selected {
                    Some(ui.interact(delete_rect, del_id, Sense::click()))
                } else {
                    None
                };

                let tile_id = ui.make_persistent_id(("widget_tile", &widget.id));
                let tile_resp = ui.interact(base_rect, tile_id, Sense::click_and_drag());

                let hover_t = ctx.animate_bool_with_time(
                    tile_id.with("hover"),
                    tile_resp.hovered() || resize_resp.hovered(),
                    0.14,
                );
                let rect = base_rect.expand(hover_t * 1.5);

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
                            2.4_f32
                        } else if tile_resp.hovered() {
                            1.6_f32
                        } else {
                            1.0_f32
                        },
                        if selected { theme.accent } else { accent },
                    ),
                    StrokeKind::Inside,
                );

                // Vector icon on the widget tile
                if let Some(icon) = widget.icon {
                    draw_vector_icon(
                        &painter,
                        rect.left_top() + Vec2::new(18.0, 18.0),
                        icon,
                        accent,
                    );
                }

                let title_offset_x = if widget.icon.is_some() { 32.0 } else { 12.0 };
                painter.text(
                    rect.left_top() + Vec2::new(title_offset_x, 10.0),
                    Align2::LEFT_TOP,
                    &widget.title,
                    FontId::proportional(f32::from(widget.style.font_size_sp).clamp(12.0, 16.5)),
                    fg,
                );

                let subtitle = widget
                    .telemetry
                    .map_or_else(|| action_label(widget.action), telemetry_label);
                painter.text(
                    rect.left_bottom() + Vec2::new(12.0, -10.0),
                    Align2::LEFT_BOTTOM,
                    subtitle,
                    FontId::proportional(11.5),
                    accent,
                );

                // Draw delete button '×' when selected
                if selected {
                    painter.circle_filled(
                        delete_rect.center(),
                        8.0,
                        Color32::from_rgba_unmultiplied(239, 68, 68, 210),
                    );
                    painter.text(
                        delete_rect.center(),
                        Align2::CENTER_CENTER,
                        "×",
                        FontId::proportional(13.0),
                        Color32::WHITE,
                    );
                }

                // Draw bottom-right corner resize grip lines
                let rb = rect.right_bottom() - Vec2::new(5.0, 5.0);
                let grip_color = if resize_resp.hovered() {
                    theme.accent
                } else {
                    Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 120)
                };
                painter.line_segment(
                    [rb - Vec2::new(8.0, 0.0), rb - Vec2::new(0.0, 8.0)],
                    Stroke::new(1.5_f32, grip_color),
                );
                painter.line_segment(
                    [rb - Vec2::new(4.0, 0.0), rb - Vec2::new(0.0, 4.0)],
                    Stroke::new(1.5_f32, grip_color),
                );

                if resize_resp.hovered() {
                    ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
                } else if tile_resp.hovered() {
                    ctx.set_cursor_icon(CursorIcon::Grab);
                }

                if let Some(dr) = del_resp
                    && dr.clicked()
                {
                    delete_widget_id = Some(widget.id.clone());
                } else if resize_resp.drag_started() {
                    started_drag = Some(DragPayload::ResizeWidget {
                        widget_id: widget.id.clone(),
                        origin_col: widget.col,
                        origin_row: widget.row,
                    });
                } else if tile_resp.drag_started() {
                    started_drag = Some(DragPayload::MoveWidget {
                        widget_id: widget.id.clone(),
                        title: widget.title.clone(),
                        col_span: widget.col_span,
                        row_span: widget.row_span,
                        accent_hex: widget.style.accent_hex.clone(),
                    });
                } else if tile_resp.clicked() {
                    clicked_widget_id = Some(widget.id.clone());
                }
            }

            if let Some(drag) = started_drag {
                self.active_drag = Some(drag);
            }

            if let Some(del_id) = delete_widget_id {
                self.state.select_widget(&del_id);
                self.state.delete_selected_widget();
                self.force_resync_drafts();
            } else if let Some(id) = clicked_widget_id {
                self.selected_empty_cell = None;
                self.state.select_widget(&id);
                self.force_resync_drafts();
            } else if ctx.input(|i| i.pointer.primary_clicked())
                && let Some((hc, hr)) = hovered_cell
                && panel.widgets.iter().all(|w| {
                    !(hc >= w.col
                        && hc < w.col + w.col_span
                        && hr >= w.row
                        && hr < w.row + w.row_span)
                })
            {
                self.selected_empty_cell = Some((hc, hr));
            }

            // Handle Drop when Mouse Button is Released over the Grid!
            if ctx.input(|i| i.pointer.any_released())
                && let Some(drag) = self.active_drag.take()
                && let Some((hc, hr)) = hovered_cell
            {
                match drag {
                    DragPayload::CatalogPreset(preset) => {
                        self.state.add_preset_at(preset, Some((hc, hr)));
                        self.force_resync_drafts();
                    }
                    DragPayload::MoveWidget { widget_id, .. } => {
                        self.state.move_widget_to_cell(&widget_id, hc, hr);
                        self.force_resync_drafts();
                    }
                    DragPayload::ResizeWidget {
                        widget_id,
                        origin_col,
                        origin_row,
                    } => {
                        let new_w = hc.saturating_sub(origin_col).saturating_add(1).max(1);
                        let new_h = hr.saturating_sub(origin_row).saturating_add(1).max(1);
                        self.state.resize_widget_to_spans(&widget_id, new_w, new_h);
                        self.force_resync_drafts();
                    }
                }
            }
        });
    }

    fn draw_widget_inspector(&mut self, ui: &mut egui::Ui, theme: ThemePalette) {
        let Some(mut draft) = self.widget_draft.clone() else {
            fluent_card(ui, theme, |ui| {
                ui.label(
                    RichText::new("Настройка виджета")
                        .size(15.5)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Нажмите на любой виджет на экране смартфона, чтобы изменить его название, цвет или действие.",
                    )
                    .color(theme.text_secondary),
                );
            });
            return;
        };

        let original_spec = draft.spec.clone();
        let mut delete_clicked = false;

        fluent_card(ui, theme, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Настройка виджета")
                        .size(15.5)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if danger_button(ui, "Удалить", theme).clicked() {
                        delete_clicked = true;
                    }
                });
            });
            ui.add_space(8.0);

            ui.label(
                RichText::new("Название на экране")
                    .size(12.0)
                    .color(theme.text_secondary),
            );
            ui.add(egui::TextEdit::singleline(&mut draft.spec.title).desired_width(f32::INFINITY));

            ui.add_space(8.0);
            ui.label(
                RichText::new("Готовые цветовые темы (в 1 клик)")
                    .size(12.0)
                    .color(theme.text_secondary),
            );
            ui.horizontal_wrapped(|ui| {
                for swatch in COLOR_SWATCHES {
                    let accent_col = parse_hex_color(swatch.accent_hex).unwrap_or(theme.accent);
                    let is_active = draft
                        .spec
                        .style
                        .accent_hex
                        .eq_ignore_ascii_case(swatch.accent_hex);
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::click());
                    let p = ui.painter_at(r);
                    p.circle_filled(r.center(), 11.0, accent_col);
                    if is_active {
                        p.circle_stroke(r.center(), 12.5, Stroke::new(2.0_f32, Color32::WHITE));
                    }
                    if resp.on_hover_text(swatch.name).clicked() {
                        draft.spec.style.background_hex = swatch.bg_hex.to_owned();
                        draft.spec.style.accent_hex = swatch.accent_hex.to_owned();
                    }
                }
            });

            ui.add_space(8.0);
            egui::Grid::new("inspector_simple_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
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

                    ui.label("Датчик ПК:");
                    let selected_label = draft.spec.telemetry.map_or("Не выбран", telemetry_label);
                    egui::ComboBox::from_id_salt("widget_telemetry_combo")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut draft.spec.telemetry, None, "Не выбран");
                            for &metric in all_telemetry_bindings() {
                                ui.selectable_value(
                                    &mut draft.spec.telemetry,
                                    Some(metric),
                                    telemetry_label(metric),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("При нажатии:");
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

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Размер текста:");
                ui.add(egui::Slider::new(
                    &mut draft.spec.style.font_size_sp,
                    11..=28,
                ));
            });
            ui.horizontal(|ui| {
                ui.label("Скругление углов:");
                ui.add(egui::Slider::new(
                    &mut draft.spec.style.corner_radius,
                    4..=24,
                ));
            });

            ui.collapsing("Точная настройка цветов", |ui| {
                draw_color_row(ui, "Фон:", &mut draft.spec.style.background_hex);
                draw_color_row(ui, "Текст:", &mut draft.spec.style.foreground_hex);
                draw_color_row(ui, "Акцент:", &mut draft.spec.style.accent_hex);
            });
        });

        if delete_clicked {
            self.state.delete_selected_widget();
            self.force_resync_drafts();
        } else if draft.spec != original_spec {
            // Instant auto-save on any change!
            self.state.update_selected_widget(draft.spec.clone());
            self.widget_draft = Some(draft);
        }
    }

    fn draw_panel_meta_section(&mut self, ui: &mut egui::Ui, theme: ThemePalette) {
        let Some(mut draft) = self.panel_meta_draft.clone() else {
            return;
        };
        let original = draft.clone();

        fluent_card(ui, theme, |ui| {
            ui.label(
                RichText::new("Размер сетки экрана")
                    .size(14.5)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Имя экрана:");
                ui.text_edit_singleline(&mut draft.name);
            });
            ui.horizontal(|ui| {
                ui.label("Колонок по ширине:");
                ui.add(
                    egui::Slider::new(&mut draft.columns, MIN_PANEL_COLUMNS..=MAX_PANEL_COLUMNS)
                        .clamping(egui::SliderClamping::Always),
                );
            });
        });

        if draft != original && !draft.name.trim().is_empty() {
            self.state.update_selected_panel_meta(
                draft.name.clone(),
                draft.columns,
                draft.refresh_interval_ms,
            );
            self.panel_meta_draft = Some(draft);
        }
    }

    fn draw_connection_tab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        running: bool,
        theme: ThemePalette,
    ) {
        ScrollArea::vertical()
            .id_salt("connection_tab_scroll")
            .show(ui, |ui| {
                // Hero Card: Visual QR Code & 6-Digit PIN
                fluent_card(ui, theme, |ui| {
                    self.draw_pairing_card_contents(ui, ctx, theme);
                });

                ui.add_space(14.0);

                // Simple Autostart & Background Mode Card
                fluent_card(ui, theme, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("Автозапуск вместе с Windows")
                                    .size(15.5)
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            ui.label(
                                RichText::new(
                                    "NexDeck будет автоматически запускаться в фоне при включении компьютера без лишних окон.",
                                )
                                .size(12.5)
                                .color(theme.text_secondary),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let enabled = self.state.autostart_command.is_some();
                            if !enabled && primary_button(ui, "Включить автозапуск", theme).clicked()
                            {
                                self.state.set_autostart_enabled(true);
                            } else if enabled
                                && secondary_button(ui, "Отключить автозапуск", theme).clicked()
                            {
                                self.state.set_autostart_enabled(false);
                            }
                        });
                    });
                });

                ui.add_space(14.0);

                self.draw_notifications_and_alerts_section(ui, theme);

                ui.add_space(14.0);

                // Collapsible Advanced / Developer section (keeps technical fields out of sight by default)
                fluent_card_compact(ui, theme, |ui| {
                    if ui
                        .selectable_label(
                            self.show_advanced_settings,
                            "Расширенные сетевые настройки и журнал сервера ▾",
                        )
                        .clicked()
                    {
                        self.show_advanced_settings = !self.show_advanced_settings;
                    }

                    if self.show_advanced_settings {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label("Сетевой порт и адрес:");
                            ui.text_edit_singleline(&mut self.state.settings_draft.bind_addr);
                            if primary_button(ui, "Сохранить", theme).clicked() {
                                self.state.save_settings_draft();
                            }
                            if running && danger_button(ui, "Остановить сервер", theme).clicked() {
                                self.state.stop_agent();
                            }
                        });
                        if let Some(note) = self.state.agent_process.last_exit_note() {
                            ui.label(RichText::new(note).size(11.5).color(theme.text_secondary));
                        }
                        let lines = self.state.agent_process.log_lines();
                        if !lines.is_empty() {
                            ui.add_space(6.0);
                            let mut combined = lines.join("\n");
                            ui.add(
                                egui::TextEdit::multiline(&mut combined)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(6),
                            );
                        }
                    }
                });
            });
    }

    fn draw_notifications_and_alerts_section(&mut self, ui: &mut egui::Ui, theme: ThemePalette) {
        fluent_card(ui, theme, |ui| {
            ui.label(
                RichText::new("Уведомления и тревоги состояния ПК")
                    .size(15.5)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.label(
                RichText::new(
                    "Настройки фильтрации уведомлений и автоматических предупреждений о перегреве или нагрузке.",
                )
                .size(12.0)
                .color(theme.text_secondary),
            );
            ui.add_space(8.0);

            ui.checkbox(
                &mut self.state.settings_draft.alerts_enabled,
                "Отправлять на телефон тревоги о высокой нагрузке, температуре и разряде батареи",
            );
            ui.add_space(8.0);

            egui::Grid::new("notif_alerts_grid")
                .num_columns(2)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Минимальный приоритет:");
                    let current_prio = self.state.settings_draft.min_notification_priority;
                    egui::ComboBox::from_id_salt("min_notif_priority_combo")
                        .selected_text(notification_priority_label(current_prio))
                        .show_ui(ui, |ui| {
                            for prio in [
                                NotificationPriority::Low,
                                NotificationPriority::Normal,
                                NotificationPriority::High,
                                NotificationPriority::Critical,
                            ] {
                                ui.selectable_value(
                                    &mut self.state.settings_draft.min_notification_priority,
                                    prio,
                                    notification_priority_label(prio),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("Заглушённые программы (через запятую):");
                    ui.add(
                        egui::TextEdit::singleline(
                            &mut self.state.settings_draft.muted_origins_csv,
                        )
                        .hint_text("Например: ExampleApp, NoisyPlugin"),
                    );
                    ui.end_row();

                    ui.label("Порог нагрузки CPU (%):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.state.settings_draft.alert_cpu_pct)
                            .desired_width(80.0)
                            .hint_text("95"),
                    );
                    ui.end_row();

                    ui.label("Порог памяти RAM (%):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.state.settings_draft.alert_ram_pct)
                            .desired_width(80.0)
                            .hint_text("95"),
                    );
                    ui.end_row();

                    ui.label("Температура CPU (°C):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.state.settings_draft.alert_cpu_temp_c)
                            .desired_width(80.0)
                            .hint_text("90"),
                    );
                    ui.end_row();

                    ui.label("Температура GPU (°C):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.state.settings_draft.alert_gpu_temp_c)
                            .desired_width(80.0)
                            .hint_text("90"),
                    );
                    ui.end_row();

                    ui.label("Низкий заряд батареи (%):");
                    ui.add(
                        egui::TextEdit::singleline(
                            &mut self.state.settings_draft.alert_battery_low_pct,
                        )
                        .desired_width(80.0)
                        .hint_text("15"),
                    );
                    ui.end_row();

                    ui.label("Пауза повтора тревоги (сек):");
                    ui.add(
                        egui::TextEdit::singleline(
                            &mut self.state.settings_draft.alert_cooldown_secs,
                        )
                        .desired_width(80.0)
                        .hint_text("120"),
                    );
                    ui.end_row();
                });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if primary_button(ui, "Сохранить правила", theme).clicked() {
                    self.state.save_settings_draft();
                }
                if secondary_button(ui, "Сбросить изменения", theme).clicked() {
                    self.state.reset_settings_draft();
                }
            });
        });

        ui.add_space(12.0);

        fluent_card(ui, theme, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Буфер отложенных уведомлений ({})",
                        self.state.buffered_notifications.len()
                    ))
                    .size(15.5)
                    .strong()
                    .color(theme.text_primary),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !self.state.buffered_notifications.is_empty()
                        && danger_button(ui, "Очистить буфер", theme).clicked()
                    {
                        self.state.clear_notifications_buffer();
                    }
                    if secondary_button(ui, "Обновить", theme).clicked() {
                        self.state.reload_notifications_buffer();
                    }
                });
            });
            ui.label(
                RichText::new(
                    "События сохраняются на диск при отключении телефона или сне ПК и доставляются сразу после подключения.",
                )
                .size(12.0)
                .color(theme.text_secondary),
            );
            ui.add_space(8.0);

            if self.state.buffered_notifications.is_empty() {
                ui.label(
                    RichText::new("Очередь отложенных уведомлений пуста.")
                        .color(theme.text_secondary),
                );
            } else {
                for event in self.state.buffered_notifications.iter().take(8) {
                    egui::Frame::new()
                        .fill(theme.card_elevated_bg)
                        .stroke(Stroke::new(1.0_f32, theme.subtle_border))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(notification_source_label(&event.source))
                                        .size(11.5)
                                        .strong()
                                        .color(theme.accent),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "• {} • #{}",
                                        notification_priority_label(event.priority),
                                        event.id
                                    ))
                                    .size(11.5)
                                    .color(theme.text_secondary),
                                );
                            });
                            ui.label(
                                RichText::new(&event.title)
                                    .size(13.5)
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            if !event.body.trim().is_empty() {
                                ui.label(
                                    RichText::new(&event.body)
                                        .size(12.0)
                                        .color(theme.text_secondary),
                                );
                            }
                        });
                    ui.add_space(6.0);
                }
            }
        });
    }

    fn draw_devices_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, theme: ThemePalette) {
        ScrollArea::vertical()
            .id_salt("devices_tab_scroll")
            .show(ui, |ui| {
                fluent_card(ui, theme, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Подключённые телефоны ({})",
                                self.state.devices.len()
                            ))
                            .size(15.5)
                            .strong()
                            .color(theme.text_primary),
                        );
                        if secondary_button(ui, "Обновить список", theme).clicked() {
                            self.state.refresh_from_disk();
                        }
                    });
                    ui.add_space(6.0);

                    if self.state.devices.is_empty() {
                        ui.label(
                            RichText::new(
                                "Пока нет привязанных телефонов. Нажмите «Подключить телефон» слева.",
                            )
                            .color(theme.text_secondary),
                        );
                    } else {
                        let mut revoke_target: Option<String> = None;
                        for record in &self.state.devices {
                            egui::Frame::new()
                                .fill(theme.card_elevated_bg)
                                .stroke(Stroke::new(1.0_f32, theme.subtle_border))
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(Margin::symmetric(14, 10))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(&record.device_name)
                                                .size(14.5)
                                                .strong()
                                                .color(theme.text_primary),
                                        );
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if danger_button(ui, "Отключить телефон", theme)
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
                    "Внешние программы и плагины ({})",
                    self.state.api_clients.len()
                ))
                .size(15.5)
                .strong()
                .color(theme.text_primary),
            );
            ui.label(
                RichText::new(
                    "Позволяет сторонним скриптам и приложениям присылать уведомления и запросы подтверждения на ваш телефон.",
                )
                .size(12.0)
                .color(theme.text_secondary),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("ID программы:");
                ui.text_edit_singleline(&mut self.state.new_api_client_id);
                ui.label("Название:");
                ui.text_edit_singleline(&mut self.state.new_api_client_name);
                if primary_button(ui, "+ Подключить программу", theme).clicked()
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
                                "Ключ доступа для '{}' (скопируйте сейчас):",
                                issued.client_id
                            ))
                            .strong()
                            .color(theme.accent),
                        );
                        ui.monospace(&issued.access_secret);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "Скопировать ключ", theme).clicked()
                            {
                                ctx.copy_text(issued.access_secret.clone());
                            }
                            if secondary_button(ui, "Готово", theme).clicked() {
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
                                ui.label(
                                    RichText::new(&client.application_name)
                                        .color(status_color)
                                        .strong(),
                                );
                                if client.revoked {
                                    if secondary_button(ui, "Разрешить", theme).clicked() {
                                        toggle_revoke = Some((client.client_id.clone(), false));
                                    }
                                } else if secondary_button(ui, "Заблокировать", theme).clicked()
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

#[derive(Clone, Copy)]
enum NavIconKind {
    Grid,
    Phone,
    Shield,
}

#[allow(clippy::too_many_arguments)]
fn draw_drop_preview(
    painter: &egui::Painter,
    canvas_rect: Rect,
    cell_w: f32,
    cell_h: f32,
    col: u16,
    row: u16,
    col_span: u16,
    row_span: u16,
    valid: bool,
    theme: ThemePalette,
) {
    let x = canvas_rect.left() + GRID_CELL_GAP + f32::from(col) * (cell_w + GRID_CELL_GAP);
    let y = canvas_rect.top() + GRID_CELL_GAP + f32::from(row) * (cell_h + GRID_CELL_GAP);
    let w = f32::from(col_span) * cell_w + f32::from(col_span.saturating_sub(1)) * GRID_CELL_GAP;
    let h = f32::from(row_span) * cell_h + f32::from(row_span.saturating_sub(1)) * GRID_CELL_GAP;
    let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    let color = if valid {
        theme.accent
    } else {
        theme.status_err
    };
    painter.rect_filled(
        rect,
        CornerRadius::same(12),
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 45),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(2.2_f32, color),
        StrokeKind::Inside,
    );
}

fn draw_qr_code_widget(ui: &mut egui::Ui, payload: &str, box_size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(box_size), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(14), Color32::WHITE);

    let Some(qr) = QrMatrix::encode_bytes(payload.as_bytes()) else {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "QR",
            FontId::proportional(18.0),
            Color32::BLACK,
        );
        return;
    };

    let quiet = 3_usize;
    let total_modules = qr.size() + quiet * 2;
    let mod_px = rect.width() / (total_modules as f32);
    let dark = Color32::from_rgb(15, 23, 42);

    for y in 0..qr.size() {
        for x in 0..qr.size() {
            if qr.get(x, y) {
                let min_x = rect.left() + ((x + quiet) as f32) * mod_px;
                let min_y = rect.top() + ((y + quiet) as f32) * mod_px;
                let m_rect = Rect::from_min_size(
                    Pos2::new(min_x, min_y),
                    Vec2::splat((mod_px + 0.35).ceil()),
                );
                painter.rect_filled(m_rect, CornerRadius::ZERO, dark);
            }
        }
    }
}

/// Draws crisp anti-aliased geometric vector icons so no emoji font is required.
fn draw_vector_icon(painter: &egui::Painter, center: Pos2, icon: WidgetIcon, color: Color32) {
    let stroke = Stroke::new(1.8_f32, color);
    match icon {
        WidgetIcon::Cpu => {
            let r = Rect::from_center_size(center, Vec2::splat(11.0));
            painter.rect_stroke(r, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.rect_filled(
                Rect::from_center_size(center, Vec2::splat(4.0)),
                CornerRadius::same(1),
                color,
            );
        }
        WidgetIcon::Memory => {
            let r = Rect::from_center_size(center, Vec2::new(14.0, 9.0));
            painter.rect_stroke(r, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.line_segment(
                [center - Vec2::new(3.0, 2.0), center - Vec2::new(3.0, -2.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(3.0, -2.0), center + Vec2::new(3.0, 2.0)],
                stroke,
            );
        }
        WidgetIcon::Gpu => {
            let r = Rect::from_center_size(center, Vec2::new(14.0, 10.0));
            painter.rect_stroke(r, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.circle_stroke(center, 2.8, stroke);
        }
        WidgetIcon::Network => {
            painter.circle_filled(center + Vec2::new(0.0, 4.0), 1.8, color);
            painter.circle_stroke(center + Vec2::new(0.0, 1.0), 4.5, stroke);
        }
        WidgetIcon::Battery => {
            let r = Rect::from_center_size(center, Vec2::new(13.0, 8.0));
            painter.rect_stroke(r, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.rect_filled(
                Rect::from_min_size(r.left_top() + Vec2::new(2.0, 2.0), Vec2::new(6.0, 4.0)),
                CornerRadius::same(1),
                color,
            );
        }
        WidgetIcon::PlayPause => {
            let pts = vec![
                center + Vec2::new(-4.0, -5.0),
                center + Vec2::new(5.0, 0.0),
                center + Vec2::new(-4.0, 5.0),
            ];
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        WidgetIcon::NextTrack => {
            painter.line_segment(
                [center + Vec2::new(-4.0, -4.5), center + Vec2::new(2.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(2.0, 0.0), center + Vec2::new(-4.0, 4.5)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(4.0, -4.5), center + Vec2::new(4.0, 4.5)],
                stroke,
            );
        }
        WidgetIcon::PreviousTrack => {
            painter.line_segment(
                [center + Vec2::new(4.0, -4.5), center + Vec2::new(-2.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(-2.0, 0.0), center + Vec2::new(4.0, 4.5)],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-4.0, -4.5),
                    center + Vec2::new(-4.0, 4.5),
                ],
                stroke,
            );
        }
        WidgetIcon::VolumeUp => {
            painter.line_segment(
                [center + Vec2::new(-5.0, 0.0), center + Vec2::new(5.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(0.0, -5.0), center + Vec2::new(0.0, 5.0)],
                stroke,
            );
        }
        WidgetIcon::VolumeDown => {
            painter.line_segment(
                [center + Vec2::new(-5.0, 0.0), center + Vec2::new(5.0, 0.0)],
                stroke,
            );
        }
        WidgetIcon::VolumeMute => {
            painter.line_segment(
                [center + Vec2::new(-4.5, -4.5), center + Vec2::new(4.5, 4.5)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(4.5, -4.5), center + Vec2::new(-4.5, 4.5)],
                stroke,
            );
        }
        _ => {
            let r = Rect::from_center_size(center, Vec2::splat(10.0));
            painter.rect_stroke(r, CornerRadius::same(3), stroke, StrokeKind::Inside);
        }
    }
}

fn fluent_card(ui: &mut egui::Ui, theme: ThemePalette, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme.card_bg)
        .stroke(Stroke::new(1.0_f32, theme.card_border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

fn fluent_card_compact(
    ui: &mut egui::Ui,
    theme: ThemePalette,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(theme.card_bg)
        .stroke(Stroke::new(1.0_f32, theme.card_border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(14, 10))
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

#[allow(clippy::too_many_arguments)]
fn draw_sidebar_nav_item(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    tab: DesktopTab,
    icon_kind: NavIconKind,
    selected: bool,
    title: &str,
    subtitle: &str,
    theme: ThemePalette,
) -> bool {
    let desired = Vec2::new(ui.available_width(), 52.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let id = Id::new(("sidebar_nav", tab as u8));
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

    let icon_color = if selected {
        theme.accent
    } else {
        theme.text_secondary
    };
    let ic_center = Pos2::new(rect.left() + 20.0, rect.center().y);
    let stroke = Stroke::new(1.7_f32, icon_color);
    match icon_kind {
        NavIconKind::Grid => {
            for dx in [-4.0_f32, 3.0] {
                for dy in [-4.0_f32, 3.0] {
                    painter.rect_stroke(
                        Rect::from_center_size(ic_center + Vec2::new(dx, dy), Vec2::splat(5.0)),
                        CornerRadius::same(1),
                        stroke,
                        StrokeKind::Inside,
                    );
                }
            }
        }
        NavIconKind::Phone => {
            painter.rect_stroke(
                Rect::from_center_size(ic_center, Vec2::new(9.0, 14.0)),
                CornerRadius::same(2),
                stroke,
                StrokeKind::Inside,
            );
        }
        NavIconKind::Shield => {
            painter.circle_stroke(ic_center, 6.5, stroke);
            painter.circle_filled(ic_center, 2.2, icon_color);
        }
    }

    let title_color = if selected {
        theme.accent
    } else {
        theme.text_primary
    };
    painter.text(
        rect.left_top() + Vec2::new(36.0, 9.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(13.8),
        title_color,
    );
    painter.text(
        rect.left_bottom() + Vec2::new(36.0, -8.0),
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
    let mut fonts = FontDefinitions::default();
    if let Ok(bytes) = fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts
            .font_data
            .insert("segoeui".to_owned(), Arc::new(FontData::from_owned(bytes)));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoeui".to_owned());
    }
    if let Ok(sym_bytes) = fs::read(r"C:\Windows\Fonts\seguisym.ttf") {
        fonts.font_data.insert(
            "seguisym".to_owned(),
            Arc::new(FontData::from_owned(sym_bytes)),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push("seguisym".to_owned());
    }
    ctx.set_fonts(fonts);
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

fn notification_priority_label(priority: NotificationPriority) -> &'static str {
    match priority {
        NotificationPriority::Low => "Низкий (все)",
        NotificationPriority::Normal => "Обычный",
        NotificationPriority::High => "Высокий (High+)",
        NotificationPriority::Critical => "Критический",
    }
}

fn notification_source_label(source: &NotificationSource) -> String {
    match source {
        NotificationSource::WindowsSystem => "Система Windows".to_owned(),
        NotificationSource::HardwareAlert => "Датчики ПК".to_owned(),
        NotificationSource::Application(name) | NotificationSource::Plugin(name) => name.clone(),
    }
}

/// Parses `#RRGGBB` or `#AARRGGBB` hex colors used by [`nexdeck_api::panels::WidgetStyle`] into [`Color32`].
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
