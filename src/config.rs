//! Config module - manages persisted state via JSON file

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: 100,
            y: 100,
            width: 340,
            height: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub symbols: Vec<String>,
    pub names: std::collections::HashMap<String, String>,
    pub window: WindowState,
    #[serde(default = "default_opacity")]
    pub opacity: u8,
}

fn default_opacity() -> u8 {
    209
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            symbols: vec![
                "AAPL".into(),
                "GOOGL".into(),
                "MSFT".into(),
                "SPY".into(),
                "QQQ".into(),
            ],
            names: Default::default(),
            window: Default::default(),
            opacity: default_opacity(),
        }
    }
}

impl AppConfig {
    /// Get the config file path (stored alongside the executable)
    pub fn config_path() -> PathBuf {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(dir) = exe_path.parent() {
                return dir.join("stock_widget_config.json");
            }
        }
        PathBuf::from("stock_widget_config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&contents) {
                return config;
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {e}"))?;
        }
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Serialization failed: {e}"))?;
        fs::write(&path, contents)
            .map_err(|e| format!("Failed to write config: {e}"))?;
        Ok(())
    }

    pub fn add_symbol(&mut self, symbol: &str) {
        let sym = symbol.to_uppercase().trim().to_string();
        if !self.symbols.contains(&sym) {
            self.symbols.push(sym);
        }
    }

    pub fn remove_symbol(&mut self, symbol: &str) {
        let sym = symbol.to_uppercase().trim().to_string();
        self.symbols.retain(|s| s != &sym);
        self.names.remove(&sym);
    }
}
