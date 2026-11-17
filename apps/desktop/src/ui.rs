//! Pure-Rust Desktop Manager GUI implemented with `eframe` / `egui`.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect,
    RichText, ScrollArea, Sense, Stroke, StrokeKind, Vec2,
};
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

const BG_CANVAS: Color32 = Color32::from_rgb(15, 23, 42);
const BG_PANEL: Color32 = Color32::from_rgb(30, 41, 59);
const BORDER_MUTED: Color32 = Color32::from_rgb(51, 65, 85);
const ACCENT_CYAN: Color32 = Color32::from_rgb(56, 189, 248);
const STATUS_OK: Color32 = Color32::from_rgb(34, 197, 94);
const STATUS_WARN: Color32 = Color32::from_rgb(245, 158, 11);
const STATUS_ERR: Color32 = Color32::from_rgb(239, 68, 68);
const GRID_CELL_GAP: f32 = 8.0;
const GRID_MIN_ROWS: u16 = 4;

/// Top-level `eframe::App` state wrapping [`DesktopManagerState`].
pub struct NexDeckDesktopApp {
    state: DesktopManagerState,
    widget_draft: Option<WidgetDraft>,
    panel_meta_draft: Option<PanelMetaDraft>,
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
        configure_visuals(&cc.egui_ctx);

        let mut app = Self {
            state,
            widget_draft: None,
            panel_meta_draft: None,
        };
        app.sync_drafts_from_selection();
        app
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

        egui::TopBottomPanel::top("nexdeck_header").show(ctx, |ui| {
            self.draw_header(ui, running);
        });

        egui::TopBottomPanel::bottom("nexdeck_status").show(ctx, |ui| {
            self.draw_status_bar(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.state.active_tab {
            DesktopTab::Panels => self.draw_panels_tab(ui),
            DesktopTab::AgentSettings => self.draw_agent_tab(ui, ctx, running),
            DesktopTab::DevicesAndTls => self.draw_devices_tab(ui, ctx),
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.agent_process.stop();
    }
}

impl NexDeckDesktopApp {
    fn draw_header(&mut self, ui: &mut egui::Ui, agent_running: bool) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("NexDeck Desktop")
                    .size(18.0)
                    .strong()
                    .color(ACCENT_CYAN),
            );
            ui.label(
                RichText::new(format!("v{AGENT_VERSION} • Протокол v{PROTOCOL_VERSION}"))
                    .small()
                    .weak(),
            );
            ui.separator();

            ui.selectable_value(
                &mut self.state.active_tab,
                DesktopTab::Panels,
                "Панели и виджеты",
            );
            ui.selectable_value(
                &mut self.state.active_tab,
                DesktopTab::AgentSettings,
                "Агент и настройки",
            );
            ui.selectable_value(
                &mut self.state.active_tab,
                DesktopTab::DevicesAndTls,
                "Устройства и TLS",
            );

            ui.separator();
            let (badge_color, badge_text) = if agent_running {
                if self.state.agent_process.is_pair_mode() {
                    (ACCENT_CYAN, "Агент: сопряжение (PIN)")
                } else {
                    (STATUS_OK, "Агент: запущен")
                }
            } else {
                (STATUS_WARN, "Агент: остановлен (внешний или выкл.)")
            };
            ui.label(RichText::new(badge_text).color(badge_color).strong());

            if ui.button("Обновить с диска").clicked() {
                self.state.refresh_from_disk();
                self.force_resync_drafts();
            }
        });
        ui.add_space(4.0);
    }

    fn draw_status_bar(&self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Каталог: {}",
                    self.state.paths.root_dir().display()
                ))
                .small()
                .weak(),
            );
            if let Some(err) = &self.state.error_message {
                ui.separator();
                ui.label(RichText::new(err).color(STATUS_ERR).strong());
            } else if let Some(msg) = &self.state.status_message {
                ui.separator();
                ui.label(RichText::new(msg).color(STATUS_OK));
            }
        });
        ui.add_space(2.0);
    }

    fn draw_panels_tab(&mut self, ui: &mut egui::Ui) {
        ui.columns(3, |columns| {
            // Column 1: Panels list, add/delete, panel metadata & JSON import/export.
            ScrollArea::vertical()
                .id_salt("panels_left_col")
                .show(&mut columns[0], |ui| {
                    self.draw_panel_list_section(ui);
                    ui.add_space(10.0);
                    self.draw_panel_meta_section(ui);
                    ui.add_space(10.0);
                    self.draw_json_section(ui);
                });

            // Column 2: Interactive grid canvas + quick add widget palette.
            ScrollArea::vertical()
                .id_salt("panels_center_col")
                .show(&mut columns[1], |ui| {
                    self.draw_widget_palette(ui);
                    ui.add_space(8.0);
                    self.draw_grid_canvas(ui);
                });

            // Column 3: Selected widget inspector & movement/resize controls.
            ScrollArea::vertical()
                .id_salt("panels_right_col")
                .show(&mut columns[2], |ui| {
                    self.draw_widget_inspector(ui);
                });
        });
    }

    fn draw_panel_list_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.heading("Панели (panels.json)");
            ui.add_space(4.0);

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

            for (idx, id, name, widget_count) in panel_headers {
                let selected = idx == self.state.selected_panel_idx;
                let label = format!("{name} ({widget_count}) [{id}]");
                if ui.selectable_label(selected, label).clicked() && !selected {
                    self.state.select_panel(idx);
                    self.force_resync_drafts();
                }
            }

            ui.separator();
            ui.label(RichText::new("Создать панель").strong());
            ui.horizontal(|ui| {
                ui.label("Название:");
                ui.text_edit_singleline(&mut self.state.new_panel_name);
            });
            ui.horizontal(|ui| {
                ui.label("Колонок:");
                ui.add(
                    egui::Slider::new(
                        &mut self.state.new_panel_columns,
                        MIN_PANEL_COLUMNS..=MAX_PANEL_COLUMNS,
                    )
                    .clamping(egui::SliderClamping::Always),
                );
            });
            ui.horizontal(|ui| {
                if ui.button("+ Добавить панель").clicked() {
                    self.state.add_panel();
                    self.force_resync_drafts();
                }
                let can_delete = self.state.deck.panels.len() > 1;
                if ui
                    .add_enabled(can_delete, egui::Button::new("Удалить панель"))
                    .clicked()
                {
                    self.state.delete_selected_panel();
                    self.force_resync_drafts();
                }
            });
            if ui.button("Сбросить Deck к стартовой").clicked() {
                self.state.reset_deck_to_starter();
                self.force_resync_drafts();
            }
        });
    }

    fn draw_panel_meta_section(&mut self, ui: &mut egui::Ui) {
        let Some(mut draft) = self.panel_meta_draft.clone() else {
            return;
        };
        let mut save_clicked = false;

        ui.group(|ui| {
            ui.heading("Параметры панели");
            ui.label(
                RichText::new(format!("ID: {}", draft.panel_id))
                    .small()
                    .weak(),
            );
            ui.horizontal(|ui| {
                ui.label("Имя:");
                ui.text_edit_singleline(&mut draft.name);
            });
            ui.horizontal(|ui| {
                ui.label("Колонки:");
                ui.add(
                    egui::Slider::new(&mut draft.columns, MIN_PANEL_COLUMNS..=MAX_PANEL_COLUMNS)
                        .clamping(egui::SliderClamping::Always),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Опрос (мс):");
                ui.add(
                    egui::DragValue::new(&mut draft.refresh_interval_ms)
                        .speed(50)
                        .range(MIN_REFRESH_INTERVAL_MS..=MAX_REFRESH_INTERVAL_MS),
                );
            });
            save_clicked = ui.button("Сохранить параметры панели").clicked();
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
        }
    }

    fn draw_json_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.heading("Экспорт / Импорт JSON");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Экспорт Deck").clicked() {
                    self.state.export_deck_to_buffer();
                }
                if ui.button("Экспорт панели").clicked() {
                    self.state.export_selected_panel_to_buffer();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Импорт Deck").clicked() {
                    self.state.import_deck_from_buffer();
                    self.force_resync_drafts();
                }
                if ui.button("Импорт панели").clicked() {
                    self.state.import_panel_from_buffer();
                    self.force_resync_drafts();
                }
            });
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

    fn draw_widget_palette(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("Добавить виджет на сетку").strong());
            ui.horizontal_wrapped(|ui| {
                for &kind in all_widget_kinds() {
                    if ui
                        .button(format!("+ {}", widget_kind_label(kind)))
                        .clicked()
                    {
                        self.state.add_widget(kind);
                        self.force_resync_drafts();
                    }
                }
            });
        });
    }

    fn draw_grid_canvas(&mut self, ui: &mut egui::Ui) {
        let Some(panel) = self.state.selected_panel().cloned() else {
            return;
        };

        let cols = panel.columns.max(MIN_PANEL_COLUMNS);
        let max_bottom = panel
            .widgets
            .iter()
            .map(|w| w.row.saturating_add(w.row_span))
            .max()
            .unwrap_or(GRID_MIN_ROWS);
        let rows = max_bottom.max(GRID_MIN_ROWS);

        let avail_width = ui.available_width().max(260.0);
        let cell_w =
            ((avail_width - GRID_CELL_GAP * f32::from(cols + 1)) / f32::from(cols)).max(46.0);
        let cell_h = (cell_w * 0.78).clamp(52.0, 96.0);
        let canvas_h = GRID_CELL_GAP + f32::from(rows) * (cell_h + GRID_CELL_GAP);

        ui.label(
            RichText::new(format!(
                "Предпросмотр сетки: {} ({cols}×{rows}, обновление {} мс)",
                panel.name, panel.refresh_interval_ms
            ))
            .strong(),
        );

        let (canvas_rect, _) =
            ui.allocate_exact_size(Vec2::new(avail_width, canvas_h), Sense::hover());
        let painter = ui.painter_at(canvas_rect);

        painter.rect_filled(canvas_rect, CornerRadius::same(10), BG_CANVAS);
        painter.rect_stroke(
            canvas_rect,
            CornerRadius::same(10),
            Stroke::new(1.0_f32, BORDER_MUTED),
            StrokeKind::Inside,
        );

        // Background cell guides so free grid slots remain clear during editing.
        for row in 0..rows {
            for col in 0..cols {
                let x =
                    canvas_rect.left() + GRID_CELL_GAP + f32::from(col) * (cell_w + GRID_CELL_GAP);
                let y =
                    canvas_rect.top() + GRID_CELL_GAP + f32::from(row) * (cell_h + GRID_CELL_GAP);
                let cell_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h));
                painter.rect_stroke(
                    cell_rect,
                    CornerRadius::same(6),
                    Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 68)),
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
            let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));

            let selected = self
                .state
                .selected_widget_id
                .as_deref()
                .is_some_and(|id| id == widget.id);

            let bg = parse_hex_color(&widget.style.background_hex).unwrap_or(BG_PANEL);
            let fg = parse_hex_color(&widget.style.foreground_hex).unwrap_or(Color32::WHITE);
            let accent = parse_hex_color(&widget.style.accent_hex).unwrap_or(ACCENT_CYAN);
            let radius = CornerRadius::same(widget.style.corner_radius.min(18));

            painter.rect_filled(rect, radius, bg);
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(
                    if selected { 2.5_f32 } else { 1.2_f32 },
                    if selected { ACCENT_CYAN } else { accent },
                ),
                StrokeKind::Inside,
            );

            painter.text(
                rect.left_top() + Vec2::new(8.0, 8.0),
                Align2::LEFT_TOP,
                &widget.title,
                FontId::proportional(f32::from(widget.style.font_size_sp).clamp(11.0, 16.0)),
                fg,
            );
            let subtitle = widget.telemetry.map_or_else(
                || widget_kind_label(widget.kind).to_owned(),
                |metric| telemetry_label(metric).to_owned(),
            );
            painter.text(
                rect.left_bottom() + Vec2::new(8.0, -8.0),
                Align2::LEFT_BOTTOM,
                subtitle,
                FontId::proportional(11.0),
                accent,
            );

            let response = ui.interact(
                rect,
                ui.make_persistent_id(("widget", &widget.id)),
                Sense::click(),
            );
            if response.clicked() {
                clicked_widget_id = Some(widget.id.clone());
            }
        }

        if let Some(id) = clicked_widget_id {
            self.state.select_widget(&id);
            self.force_resync_drafts();
        }
    }

    fn draw_widget_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(mut draft) = self.widget_draft.clone() else {
            ui.group(|ui| {
                ui.heading("Инспектор виджета");
                ui.label("Выберите виджет на сетке или добавьте новый сверху.");
            });
            return;
        };

        let mut apply_edit = false;
        let mut move_delta: Option<(i32, i32)> = None;
        let mut resize_delta: Option<(i32, i32)> = None;
        let mut delete_clicked = false;

        ui.group(|ui| {
            ui.heading("Инспектор виджета");
            ui.label(
                RichText::new(format!("ID: {}", draft.spec.id))
                    .small()
                    .weak(),
            );

            ui.horizontal(|ui| {
                ui.label("Заголовок:");
                ui.text_edit_singleline(&mut draft.spec.title);
            });

            ui.horizontal(|ui| {
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
            });

            ui.separator();
            ui.label(
                RichText::new(format!(
                    "Позиция: col={}, row={}, размер={}×{}",
                    draft.spec.col, draft.spec.row, draft.spec.col_span, draft.spec.row_span
                ))
                .strong(),
            );
            ui.horizontal(|ui| {
                ui.label("Сдвиг:");
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
                if ui.button("−").clicked() {
                    resize_delta = Some((-1, 0));
                }
                if ui.button("+").clicked() {
                    resize_delta = Some((1, 0));
                }
                ui.label("Высота:");
                if ui.button(" − ").clicked() {
                    resize_delta = Some((0, -1));
                }
                if ui.button(" + ").clicked() {
                    resize_delta = Some((0, 1));
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Метрика:");
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
            });

            ui.horizontal(|ui| {
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
            });

            ui.horizontal(|ui| {
                ui.label("Иконка:");
                let current_icon = draft.spec.icon.map_or("Нет", icon_label);
                egui::ComboBox::from_id_salt("widget_icon_combo")
                    .selected_text(current_icon)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut draft.spec.icon, None, "Нет");
                        for &icon in all_widget_icons() {
                            ui.selectable_value(&mut draft.spec.icon, Some(icon), icon_label(icon));
                        }
                    });
            });

            if matches!(draft.spec.kind, WidgetKind::Text) {
                let mut text_val = draft.spec.text.clone().unwrap_or_default();
                ui.label("Текст:");
                ui.text_edit_multiline(&mut text_val);
                draft.spec.text = if text_val.is_empty() {
                    None
                } else {
                    Some(text_val)
                };
            }

            if matches!(draft.spec.kind, WidgetKind::Image) {
                let mut key = draft.spec.asset_key.clone().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label("Ключ изображения:");
                    ui.text_edit_singleline(&mut key);
                });
                draft.spec.asset_key = if key.trim().is_empty() {
                    None
                } else {
                    Some(key)
                };
            }

            if matches!(draft.spec.kind, WidgetKind::Custom) {
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

            ui.separator();
            ui.label(RichText::new("Оформление карточки").strong());
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

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Применить свойства").clicked() {
                    apply_edit = true;
                }
                if ui.button("Сбросить стиль").clicked() {
                    draft.spec.style = WidgetStyle::default();
                }
            });
            if ui
                .button(RichText::new("Удалить виджет").color(STATUS_ERR))
                .clicked()
            {
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

    fn draw_agent_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, running: bool) {
        ui.columns(2, |columns| {
            // Left: Runtime control, pairing, autostart, config.json form.
            ScrollArea::vertical()
                .id_salt("agent_left_col")
                .show(&mut columns[0], |ui| {
                    ui.group(|ui| {
                        ui.heading("Локальный процесс nexdeck-agent");
                        ui.horizontal(|ui| {
                            ui.label("Таймер сопряжения (сек):");
                            ui.add(
                                egui::DragValue::new(&mut self.state.pair_ttl_secs)
                                    .speed(10)
                                    .range(30..=1800),
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(!running, egui::Button::new("▶ Запустить (обычный)"))
                                .clicked()
                            {
                                self.state.start_agent(false);
                            }
                            if ui
                                .add_enabled(!running, egui::Button::new("🔗 Запустить с PIN/QR"))
                                .clicked()
                            {
                                self.state.start_agent(true);
                            }
                            if ui
                                .add_enabled(running, egui::Button::new("■ Остановить"))
                                .clicked()
                            {
                                self.state.stop_agent();
                            }
                        });
                        if let Some(note) = self.state.agent_process.last_exit_note() {
                            ui.label(RichText::new(note).small().weak());
                        }
                    });

                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.heading("Автозапуск Windows (без UAC)");
                        if let Some(cmd) = &self.state.autostart_command {
                            ui.label(RichText::new("Статус: включён в HKCU\\...\\Run").color(STATUS_OK));
                            ui.monospace(cmd);
                        } else {
                            ui.label(RichText::new("Статус: отключён").color(STATUS_WARN));
                        }
                        if let Some(note) = &self.state.autostart_note {
                            ui.label(RichText::new(note).small().weak());
                        }
                        ui.horizontal(|ui| {
                            let enabled = self.state.autostart_command.is_some();
                            if ui
                                .add_enabled(!enabled, egui::Button::new("Включить автозапуск"))
                                .clicked()
                            {
                                self.state.set_autostart_enabled(true);
                            }
                            if ui
                                .add_enabled(enabled, egui::Button::new("Отключить автозапуск"))
                                .clicked()
                            {
                                self.state.set_autostart_enabled(false);
                            }
                        });
                    });

                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.heading("Конфигурация (config.json)");
                        ui.horizontal(|ui| {
                            ui.label("WSS адрес:");
                            ui.text_edit_singleline(&mut self.state.settings_draft.bind_addr);
                        });
                        ui.horizontal(|ui| {
                            ui.label("SAN хосты (через запятую):");
                            ui.text_edit_singleline(&mut self.state.settings_draft.san_hosts_csv);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Интервал телеметрии (мс):");
                            ui.text_edit_singleline(&mut self.state.settings_draft.poll_interval_ms);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Очередь уведомлений:");
                            ui.text_edit_singleline(
                                &mut self.state.settings_draft.notification_queue_capacity,
                            );
                        });
                        ui.checkbox(
                            &mut self.state.settings_draft.enable_plugins,
                            "Разрешить изолированный WASM-движок плагинов",
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Сохранить config.json").clicked() {
                                self.state.save_settings_draft();
                            }
                            if ui.button("Сбросить форму").clicked() {
                                self.state.reset_settings_draft();
                            }
                        });
                    });
                });

            // Right: Live pairing PIN/JSON card + log output from child agent.
            ScrollArea::vertical()
                .id_salt("agent_right_col")
                .show(&mut columns[1], |ui| {
                    let (pin_opt, pairing_json_opt) = self.state.agent_process.latest_pairing_info();
                    ui.group(|ui| {
                        ui.heading("Сопряжение мобильного клиента");
                        if let Some(pin) = pin_opt {
                            ui.label(
                                RichText::new(format!("Одноразовый PIN: {pin}"))
                                    .size(22.0)
                                    .strong()
                                    .color(ACCENT_CYAN),
                            );
                        } else {
                            ui.label(
                                "Нажмите «Запустить с PIN/QR», чтобы открыть временную сессию сопряжения.",
                            );
                        }
                        if let Some(json) = pairing_json_opt {
                            if ui.button("Скопировать QR JSON").clicked() {
                                ctx.copy_text(json.clone());
                            }
                            let mut preview = json;
                            ui.add(
                                egui::TextEdit::multiline(&mut preview)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(7),
                            );
                        }
                    });

                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.heading("Журнал агента");
                        let lines = self.state.agent_process.log_lines();
                        if lines.is_empty() {
                            ui.label(RichText::new("Вывод процесса пока пуст.").weak());
                        } else {
                            let mut combined = lines.join("\n");
                            ui.add(
                                egui::TextEdit::multiline(&mut combined)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(16),
                            );
                        }
                    });
                });
        });
    }

    fn draw_devices_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.group(|ui| {
            ui.heading("TLS-отпечаток хоста (SHA-256 Pinning)");
            ui.monospace(&self.state.tls_fingerprint_sha256);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Скопировать отпечаток").clicked() {
                    ctx.copy_text(self.state.tls_fingerprint_sha256.clone());
                }
                if ui
                    .button(RichText::new("Перевыпустить TLS-сертификат").color(STATUS_WARN))
                    .clicked()
                {
                    self.state.rotate_tls_identity();
                }
            });
            ui.label(
                RichText::new(
                    "Перевыпуск сертификата изменяет SHA-256 отпечаток: мобильному приложению потребуется повторное сопряжение.",
                )
                .small()
                .weak(),
            );
        });

        ui.add_space(10.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading(format!(
                    "Авторизованные устройства ({})",
                    self.state.devices.len()
                ));
                if ui.button("Обновить список").clicked() {
                    self.state.refresh_from_disk();
                }
            });

            if self.state.devices.is_empty() {
                ui.label("Сопряжённых устройств пока нет. Запустите агента в режиме --pair.");
                return;
            }

            let mut revoke_target: Option<String> = None;
            ScrollArea::vertical()
                .id_salt("devices_scroll")
                .show(ui, |ui| {
                    for record in &self.state.devices {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} ({})",
                                            record.device_name, record.device_id
                                        ))
                                        .strong(),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "Epoch ms: {} • Hash: {}…",
                                            record.paired_at_epoch_ms,
                                            &record.token_hash_sha256
                                                [..record.token_hash_sha256.len().min(16)]
                                        ))
                                        .small()
                                        .weak(),
                                    );
                                });
                                if ui
                                    .button(RichText::new("Отозвать доступ").color(STATUS_ERR))
                                    .clicked()
                                {
                                    revoke_target = Some(record.device_id.clone());
                                }
                            });
                        });
                    }
                });

            if let Some(id) = revoke_target {
                self.state.revoke_device(&id);
            }
        });
    }
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

fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_CANVAS;
    visuals.window_fill = BG_PANEL;
    ctx.set_visuals(visuals);
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
