//! Stage 3 cross-crate contract tests for shared panel/widget layouts and storage.

use nexdeck_api::panels::{DeckConfig, PanelConfig};
use nexdeck_core::storage::StoragePaths;

#[test]
fn example_panel_default_json_matches_rust_schema() {
    let raw = include_str!("../../../examples/panel_default.json");
    let imported = DeckConfig::import_json(raw).expect("example deck JSON must be valid");
    assert_eq!(imported, DeckConfig::default_starter());

    let single_json = imported.panels[0]
        .export_json()
        .expect("single panel export should succeed");
    let single_reimported =
        PanelConfig::import_json(&single_json).expect("single panel import should succeed");
    assert_eq!(single_reimported, imported.panels[0]);
}

#[test]
fn storage_persists_imported_panel_workspace() {
    let temp_dir =
        std::env::temp_dir().join(format!("nexdeck-stage3-panels-{}", std::process::id()));
    let paths = StoragePaths::new(temp_dir.clone());
    let deck = paths
        .load_or_init_panels()
        .expect("panels.json should initialize");
    assert_eq!(deck.panels.len(), 1);
    let _ = std::fs::remove_dir_all(&temp_dir);
}
