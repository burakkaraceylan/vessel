use anyhow::Context;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    pub id: String,
    pub name: String,
    pub rows: u32,
    pub columns: u32,
    #[serde(default)]
    pub widgets: Vec<WidgetInstance>,
    #[serde(default)]
    pub zones: Vec<Zone>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetInstance {
    pub id: String,
    #[serde(rename = "type")]
    pub widget_type: String,
    pub size: Size,
    pub position: Position,
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    pub position: Position,
    pub size: Size,
    pub profiles: Vec<ZoneProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneProfile {
    pub name: String,
    pub condition: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub widgets: Vec<WidgetInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub col: u32,
    pub row: u32,
}

pub struct DashboardStore {
    dashboards: DashMap<String, Dashboard>,
}

impl DashboardStore {
    pub fn new() -> Self {
        Self {
            dashboards: DashMap::new(),
        }
    }

    pub fn load_dashboards(&self) -> anyhow::Result<()> {
        let dir = dirs::data_local_dir()
            .context("Could not determine local data directory")?
            .join("vessel")
            .join("dashboards");

        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let content = std::fs::read_to_string(entry.path())?;
                let dashboard: Dashboard = serde_json::from_str(&content).with_context(|| {
                    format!("Failed to parse dashboard file: {:?}", entry.path())
                })?;
                self.dashboards.insert(dashboard.id.clone(), dashboard);
            }
        }

        Ok(())
    }

    pub fn get_dashboard(&self, id: &str) -> Option<Dashboard> {
        self.dashboards.get(id).map(|entry| entry.value().clone())
    }

    pub fn list_dashboards(&self) -> Vec<Dashboard> {
        self.dashboards
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    pub fn save_dashboard(&self, dashboard: &Dashboard) -> anyhow::Result<()> {
        let dir = dirs::data_local_dir()
            .context("Could not determine local data directory")?
            .join("vessel")
            .join("dashboards");

        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }

        let path = dir.join(format!("{}.json", dashboard.id));
        let content = serde_json::to_string_pretty(dashboard)?;
        std::fs::write(path, content)?;

        Ok(())
    }
}
