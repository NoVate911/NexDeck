//! Shared panel builder and widget layout schema for desktop and mobile clients.
//!
//! Covers customizable grids, widget styles, refresh intervals, telemetry
//! bindings, safe allowlisted actions, and validated JSON import/export.

use std::collections::HashSet;
use std::fmt;

use nexdeck_transport::control::ControlAction;
use serde::{Deserialize, Serialize};

/// Current JSON schema version for [`DeckConfig`] and [`PanelConfig`].
pub const PANEL_SCHEMA_VERSION: u16 = 1;

/// Minimum allowed grid column count per panel.
pub const MIN_PANEL_COLUMNS: u16 = 2;

/// Maximum allowed grid column count per panel.
pub const MAX_PANEL_COLUMNS: u16 = 8;

/// Minimum widget refresh interval in milliseconds.
pub const MIN_REFRESH_INTERVAL_MS: u64 = 100;

/// Maximum widget refresh interval in milliseconds (`1 minute`).
pub const MAX_REFRESH_INTERVAL_MS: u64 = 60_000;

/// Supported visual element types in the Stage 3 panel builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WidgetKind {
    /// Static or labeled text block.
    Text,
    /// Numeric readout bound to a [`TelemetryBinding`].
    MetricNumber,
    /// Push button invoking a [`WidgetAction`].
    Button,
    /// Toggle switch tracking boolean state or mute action.
    Toggle,
    /// Real-time line/area chart bound to a [`TelemetryBinding`].
    Chart,
    /// Percentage progress gauge bound to a [`TelemetryBinding`].
    Gauge,
    /// Built-in semantic vector icon.
    Icon,
    /// Built-in synthetic/registered asset image referenced by safe key.
    Image,
    /// Extension widget provided by an authorized plugin (Stage 6).
    Custom,
}

/// Hardware telemetry metrics bindable to numeric, chart, or gauge widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TelemetryBinding {
    /// Total CPU utilization percentage.
    CpuUsagePct,
    /// Average/base CPU frequency in MHz.
    CpuFrequencyMhz,
    /// CPU package temperature in Celsius.
    CpuTemperatureC,
    /// Physical RAM utilization percentage.
    RamUsagePct,
    /// Used physical RAM in bytes.
    RamUsedBytes,
    /// Primary GPU utilization percentage.
    GpuUsagePct,
    /// Primary GPU VRAM utilization percentage.
    GpuVramPct,
    /// Aggregate network download speed in bytes/sec.
    NetworkRxBps,
    /// Aggregate network upload speed in bytes/sec.
    NetworkTxBps,
    /// Battery charge percentage.
    BatteryPct,
    /// Host uptime in seconds.
    UptimeSecs,
}

/// Action dispatched when an interactive [`WidgetSpec`] (`Button`/`Toggle`) is activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "action")]
pub enum WidgetAction {
    /// No-op / display-only widget.
    #[default]
    None,
    /// Allowlisted host media/volume control command.
    Control(ControlAction),
    /// WebSocket keepalive ping.
    Heartbeat,
}

/// Curated cross-platform icon identifiers supported without untrusted assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WidgetIcon {
    /// Processor icon.
    Cpu,
    /// Memory module icon.
    Memory,
    /// Graphics adapter icon.
    Gpu,
    /// Storage drive icon.
    Disk,
    /// Network adapter icon.
    Network,
    /// Battery status icon.
    Battery,
    /// Play/pause media icon.
    PlayPause,
    /// Previous track icon.
    PreviousTrack,
    /// Next track icon.
    NextTrack,
    /// Volume increase icon.
    VolumeUp,
    /// Volume decrease icon.
    VolumeDown,
    /// Audio mute icon.
    VolumeMute,
    /// Terminal/command status icon.
    Terminal,
    /// Security/shield status icon.
    Shield,
}

/// Visual customization applied to an individual [`WidgetSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetStyle {
    /// Background color in `#RRGGBB` or `#AARRGGBB` hex form.
    pub background_hex: String,
    /// Primary text/foreground color in `#RRGGBB` or `#AARRGGBB` hex form.
    pub foreground_hex: String,
    /// Accent/chart highlight color in `#RRGGBB` or `#AARRGGBB` hex form.
    pub accent_hex: String,
    /// Base title/value font size in logical sp (`10..=48`).
    pub font_size_sp: u8,
    /// Card corner radius in logical px (`0..=32`).
    pub corner_radius: u8,
}

impl Default for WidgetStyle {
    fn default() -> Self {
        Self {
            background_hex: "#1E293B".to_owned(),
            foreground_hex: "#F8FAFC".to_owned(),
            accent_hex: "#38BDF8".to_owned(),
            font_size_sp: 14,
            corner_radius: 12,
        }
    }
}

impl WidgetStyle {
    /// Validates color strings and typography/geometry bounds.
    ///
    /// # Errors
    ///
    /// Returns [`PanelValidationError`] if any color hex or size range is invalid.
    pub fn validate(&self) -> Result<(), PanelValidationError> {
        validate_hex_color("background_hex", &self.background_hex)?;
        validate_hex_color("foreground_hex", &self.foreground_hex)?;
        validate_hex_color("accent_hex", &self.accent_hex)?;
        if !(10..=48).contains(&self.font_size_sp) {
            return Err(PanelValidationError::OutOfBounds(
                "font_size_sp must be between 10 and 48",
            ));
        }
        if self.corner_radius > 32 {
            return Err(PanelValidationError::OutOfBounds(
                "corner_radius must be at most 32",
            ));
        }
        Ok(())
    }
}

/// Positioned and styled widget placed on a [`PanelConfig`] grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetSpec {
    /// Unique identifier within the parent panel (`1..=64` safe ID chars).
    pub id: String,
    /// Display caption shown on the card or button.
    pub title: String,
    /// Element rendering kind.
    pub kind: WidgetKind,
    /// Zero-based grid column index.
    pub col: u16,
    /// Zero-based grid row index.
    pub row: u16,
    /// Horizontal span in grid columns (`>= 1`).
    pub col_span: u16,
    /// Vertical span in grid rows (`>= 1`).
    pub row_span: u16,
    /// Optional telemetry metric binding for dynamic readout/chart/gauge widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<TelemetryBinding>,
    /// Optional interactive action triggered on press/toggle.
    #[serde(default)]
    pub action: WidgetAction,
    /// Static or supplemental text content for `Text` widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Optional built-in icon for `Icon`/`Button`/`Toggle` widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<WidgetIcon>,
    /// Safe asset identifier for `Image` widgets (`[A-Za-z0-9_-]`, no path traversal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_key: Option<String>,
    /// Custom extension component identifier for `Custom` widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_component: Option<String>,
    /// Per-widget appearance configuration.
    #[serde(default)]
    pub style: WidgetStyle,
}

impl WidgetSpec {
    /// Validates identifiers, dimensions, style, and safe key fields.
    ///
    /// # Errors
    ///
    /// Returns [`PanelValidationError`] if any field violates schema constraints.
    pub fn validate(&self, panel_columns: u16) -> Result<(), PanelValidationError> {
        validate_safe_id("widget.id", &self.id)?;
        if self.title.trim().is_empty() || self.title.len() > 80 {
            return Err(PanelValidationError::InvalidField(
                "widget.title must be 1..=80 characters",
            ));
        }
        if self.col_span == 0 || self.row_span == 0 {
            return Err(PanelValidationError::OutOfBounds(
                "col_span and row_span must be at least 1",
            ));
        }
        let right_edge = self
            .col
            .checked_add(self.col_span)
            .ok_or(PanelValidationError::OutOfBounds("column span overflow"))?;
        if right_edge > panel_columns {
            return Err(PanelValidationError::OutOfBounds(
                "widget extends beyond panel column count",
            ));
        }
        let _bottom_edge = self
            .row
            .checked_add(self.row_span)
            .ok_or(PanelValidationError::OutOfBounds("row span overflow"))?;

        if let Some(key) = &self.asset_key {
            validate_safe_id("asset_key", key)?;
        }
        if let Some(comp) = &self.custom_component {
            validate_safe_id("custom_component", comp)?;
        }
        if let Some(text) = &self.text
            && text.len() > 280
        {
            return Err(PanelValidationError::InvalidField(
                "widget.text must not exceed 280 characters",
            ));
        }

        self.style.validate()?;
        Ok(())
    }

    /// Returns `true` if `self` occupies any shared grid cell with `other`.
    #[must_use]
    pub const fn overlaps(&self, other: &Self) -> bool {
        let a_right = self.col.saturating_add(self.col_span);
        let b_right = other.col.saturating_add(other.col_span);
        let a_bottom = self.row.saturating_add(self.row_span);
        let b_bottom = other.row.saturating_add(other.row_span);

        !(a_right <= other.col
            || b_right <= self.col
            || a_bottom <= other.row
            || b_bottom <= self.row)
    }
}

/// Complete configuration for a single grid page/dashboard panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelConfig {
    /// Unique panel identifier (`1..=64` safe chars).
    pub id: String,
    /// Human-readable panel name.
    pub name: String,
    /// Number of columns in the layout grid (`2..=8`).
    pub columns: u16,
    /// Preferred telemetry update cadence in milliseconds (`100..=60000`).
    pub refresh_interval_ms: u64,
    /// Ordered list of widgets placed on this panel.
    pub widgets: Vec<WidgetSpec>,
}

impl PanelConfig {
    /// Validates panel dimensions, unique widget IDs, and non-overlapping grid placements.
    ///
    /// # Errors
    ///
    /// Returns [`PanelValidationError`] on invalid bounds, duplicate IDs, or overlapping cells.
    pub fn validate(&self) -> Result<(), PanelValidationError> {
        validate_safe_id("panel.id", &self.id)?;
        if self.name.trim().is_empty() || self.name.len() > 64 {
            return Err(PanelValidationError::InvalidField(
                "panel.name must be 1..=64 characters",
            ));
        }
        if !(MIN_PANEL_COLUMNS..=MAX_PANEL_COLUMNS).contains(&self.columns) {
            return Err(PanelValidationError::OutOfBounds(
                "panel.columns must be between 2 and 8",
            ));
        }
        if !(MIN_REFRESH_INTERVAL_MS..=MAX_REFRESH_INTERVAL_MS).contains(&self.refresh_interval_ms)
        {
            return Err(PanelValidationError::OutOfBounds(
                "panel.refresh_interval_ms must be between 100 and 60000",
            ));
        }

        let mut seen_ids = HashSet::with_capacity(self.widgets.len());
        for (idx, widget) in self.widgets.iter().enumerate() {
            if !seen_ids.insert(widget.id.as_str()) {
                return Err(PanelValidationError::DuplicateId(widget.id.clone()));
            }
            widget.validate(self.columns)?;
            for other in &self.widgets[..idx] {
                if widget.overlaps(other) {
                    return Err(PanelValidationError::GridOverlap {
                        first_id: other.id.clone(),
                        second_id: widget.id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Serializes a single panel to pretty JSON for sharing/export.
    ///
    /// # Errors
    ///
    /// Returns [`PanelValidationError`] if validation or JSON encoding fails.
    pub fn export_json(&self) -> Result<String, PanelValidationError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|err| PanelValidationError::Json(err.to_string()))
    }

    /// Parses and validates a single panel exported as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`PanelValidationError`] if JSON decoding or grid validation fails.
    pub fn import_json(raw: &str) -> Result<Self, PanelValidationError> {
        let panel: Self =
            serde_json::from_str(raw).map_err(|err| PanelValidationError::Json(err.to_string()))?;
        panel.validate()?;
        Ok(panel)
    }
}

/// Top-level persisted deck configuration containing one or more [`PanelConfig`] tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckConfig {
    /// Schema version (`1`).
    pub schema_version: u16,
    /// Currently selected panel ID.
    pub active_panel_id: String,
    /// Configured panels in display order.
    pub panels: Vec<PanelConfig>,
}

impl Default for DeckConfig {
    fn default() -> Self {
        Self::default_starter()
    }
}

impl DeckConfig {
    /// Builds a validated default starter deck matching Stage 2/3 monitoring and media widgets.
    #[must_use]
    pub fn default_starter() -> Self {
        let system_panel = PanelConfig {
            id: "system-overview".to_owned(),
            name: "Системный обзор".to_owned(),
            columns: 4,
            refresh_interval_ms: 1_000,
            widgets: vec![
                WidgetSpec {
                    id: "cpu-chart".to_owned(),
                    title: "CPU Нагрузка".to_owned(),
                    kind: WidgetKind::Chart,
                    col: 0,
                    row: 0,
                    col_span: 2,
                    row_span: 2,
                    telemetry: Some(TelemetryBinding::CpuUsagePct),
                    action: WidgetAction::None,
                    text: None,
                    icon: Some(WidgetIcon::Cpu),
                    asset_key: None,
                    custom_component: None,
                    style: WidgetStyle::default(),
                },
                WidgetSpec {
                    id: "ram-gauge".to_owned(),
                    title: "RAM %".to_owned(),
                    kind: WidgetKind::Gauge,
                    col: 2,
                    row: 0,
                    col_span: 2,
                    row_span: 1,
                    telemetry: Some(TelemetryBinding::RamUsagePct),
                    action: WidgetAction::None,
                    text: None,
                    icon: Some(WidgetIcon::Memory),
                    asset_key: None,
                    custom_component: None,
                    style: WidgetStyle::default(),
                },
                WidgetSpec {
                    id: "gpu-usage".to_owned(),
                    title: "GPU %".to_owned(),
                    kind: WidgetKind::MetricNumber,
                    col: 2,
                    row: 1,
                    col_span: 2,
                    row_span: 1,
                    telemetry: Some(TelemetryBinding::GpuUsagePct),
                    action: WidgetAction::None,
                    text: None,
                    icon: Some(WidgetIcon::Gpu),
                    asset_key: None,
                    custom_component: None,
                    style: WidgetStyle::default(),
                },
                WidgetSpec {
                    id: "media-play".to_owned(),
                    title: "Пауза / Пуск".to_owned(),
                    kind: WidgetKind::Button,
                    col: 0,
                    row: 2,
                    col_span: 2,
                    row_span: 1,
                    telemetry: None,
                    action: WidgetAction::Control(ControlAction::MediaPlayPause),
                    text: None,
                    icon: Some(WidgetIcon::PlayPause),
                    asset_key: None,
                    custom_component: None,
                    style: WidgetStyle {
                        accent_hex: "#22D3EE".to_owned(),
                        ..WidgetStyle::default()
                    },
                },
                WidgetSpec {
                    id: "volume-mute".to_owned(),
                    title: "Без звука".to_owned(),
                    kind: WidgetKind::Toggle,
                    col: 2,
                    row: 2,
                    col_span: 2,
                    row_span: 1,
                    telemetry: None,
                    action: WidgetAction::Control(ControlAction::VolumeMuteToggle),
                    text: None,
                    icon: Some(WidgetIcon::VolumeMute),
                    asset_key: None,
                    custom_component: None,
                    style: WidgetStyle {
                        accent_hex: "#F59E0B".to_owned(),
                        ..WidgetStyle::default()
                    },
                },
            ],
        };

        Self {
            schema_version: PANEL_SCHEMA_VERSION,
            active_panel_id: "system-overview".to_owned(),
            panels: vec![system_panel],
        }
    }

    /// Validates schema version, non-empty panels, unique panel IDs, active panel, and layouts.
    ///
    /// # Errors
    ///
    /// Returns [`PanelValidationError`] if any invariant fails.
    pub fn validate(&self) -> Result<(), PanelValidationError> {
        if self.schema_version != PANEL_SCHEMA_VERSION {
            return Err(PanelValidationError::UnsupportedSchema(self.schema_version));
        }
        if self.panels.is_empty() {
            return Err(PanelValidationError::InvalidField(
                "deck must contain at least one panel",
            ));
        }

        let mut seen = HashSet::with_capacity(self.panels.len());
        for panel in &self.panels {
            if !seen.insert(panel.id.as_str()) {
                return Err(PanelValidationError::DuplicateId(panel.id.clone()));
            }
            panel.validate()?;
        }

        if !seen.contains(self.active_panel_id.as_str()) {
            return Err(PanelValidationError::InvalidField(
                "active_panel_id does not match any configured panel",
            ));
        }
        Ok(())
    }

    /// Validates and serializes the full [`DeckConfig`] into pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns [`PanelValidationError`] on validation or JSON encoding failure.
    pub fn export_json(&self) -> Result<String, PanelValidationError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|err| PanelValidationError::Json(err.to_string()))
    }

    /// Parses and validates a serialized [`DeckConfig`] JSON document.
    ///
    /// # Errors
    ///
    /// Returns [`PanelValidationError`] on JSON decode or validation failure.
    pub fn import_json(raw: &str) -> Result<Self, PanelValidationError> {
        let deck: Self =
            serde_json::from_str(raw).map_err(|err| PanelValidationError::Json(err.to_string()))?;
        deck.validate()?;
        Ok(deck)
    }
}

/// Errors returned when validating, importing, or exporting panel layouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelValidationError {
    /// Unsupported `schema_version` value.
    UnsupportedSchema(u16),
    /// Missing or malformed field value.
    InvalidField(&'static str),
    /// Numeric parameter or grid coordinate fell outside allowed bounds.
    OutOfBounds(&'static str),
    /// Duplicate panel or widget identifier encountered.
    DuplicateId(String),
    /// Two widgets occupied overlapping grid coordinates.
    GridOverlap {
        /// Identifier of the previously placed widget.
        first_id: String,
        /// Identifier of the colliding widget.
        second_id: String,
    },
    /// JSON serialization or deserialization failed.
    Json(String),
}

impl fmt::Display for PanelValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema(v) => write!(f, "unsupported panel schema version: {v}"),
            Self::InvalidField(msg) | Self::OutOfBounds(msg) => write!(f, "{msg}"),
            Self::DuplicateId(id) => write!(f, "duplicate panel/widget identifier: {id}"),
            Self::GridOverlap {
                first_id,
                second_id,
            } => write!(f, "widget '{second_id}' overlaps '{first_id}' on grid"),
            Self::Json(err) => write!(f, "panel JSON error: {err}"),
        }
    }
}

impl std::error::Error for PanelValidationError {}

fn validate_safe_id(field: &'static str, value: &str) -> Result<(), PanelValidationError> {
    let valid_chars = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.';
    if value.is_empty()
        || value.len() > 64
        || value.contains("..")
        || !value.bytes().all(valid_chars)
    {
        return Err(match field {
            "widget.id" => PanelValidationError::InvalidField(
                "widget.id must use 1..=64 safe ASCII identifier chars",
            ),
            "panel.id" => PanelValidationError::InvalidField(
                "panel.id must use 1..=64 safe ASCII identifier chars",
            ),
            "asset_key" => PanelValidationError::InvalidField(
                "asset_key must use 1..=64 safe identifier chars without '..'",
            ),
            _ => PanelValidationError::InvalidField(
                "identifier must use 1..=64 safe ASCII identifier chars",
            ),
        });
    }
    Ok(())
}

fn validate_hex_color(field: &'static str, value: &str) -> Result<(), PanelValidationError> {
    let valid = value.starts_with('#')
        && (value.len() == 7 || value.len() == 9)
        && value.bytes().skip(1).all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(match field {
            "background_hex" => {
                PanelValidationError::InvalidField("background_hex must be #RRGGBB or #AARRGGBB")
            }
            "foreground_hex" => {
                PanelValidationError::InvalidField("foreground_hex must be #RRGGBB or #AARRGGBB")
            }
            _ => PanelValidationError::InvalidField("accent_hex must be #RRGGBB or #AARRGGBB"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_deck_validates_and_round_trips_json() {
        let deck = DeckConfig::default_starter();
        assert_eq!(deck.validate(), Ok(()));

        let json = deck.export_json().expect("export starter deck");
        let reimported = DeckConfig::import_json(&json).expect("import starter deck");
        assert_eq!(reimported, deck);
    }

    #[test]
    fn rejects_overlapping_widgets_and_out_of_bounds_columns() {
        let mut deck = DeckConfig::default_starter();
        let panel = &mut deck.panels[0];

        // Collides with `cpu-chart` at (0,0)-(2,2).
        panel.widgets.push(WidgetSpec {
            id: "overlap".to_owned(),
            title: "Overlap".to_owned(),
            kind: WidgetKind::Text,
            col: 1,
            row: 1,
            col_span: 1,
            row_span: 1,
            telemetry: None,
            action: WidgetAction::None,
            text: Some("Sample".to_owned()),
            icon: None,
            asset_key: None,
            custom_component: None,
            style: WidgetStyle::default(),
        });

        assert!(matches!(
            panel.validate(),
            Err(PanelValidationError::GridOverlap { .. })
        ));

        panel.widgets.last_mut().expect("exists").col = 3;
        panel.widgets.last_mut().expect("exists").row = 5;
        panel.widgets.last_mut().expect("exists").col_span = 2; // 3 + 2 > 4
        assert!(matches!(
            panel.validate(),
            Err(PanelValidationError::OutOfBounds(_))
        ));
    }

    #[test]
    fn rejects_path_traversal_asset_keys_and_invalid_colors() {
        let mut panel = DeckConfig::default_starter().panels[0].clone();
        panel.widgets[0].asset_key = Some("../secret".to_owned());
        assert!(panel.validate().is_err());

        panel.widgets[0].asset_key = Some("logo_main".to_owned());
        panel.widgets[0].style.accent_hex = "blue".to_owned();
        assert!(panel.validate().is_err());
    }
}
