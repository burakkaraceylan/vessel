use crate::wasm::manifest::{load_manifest, HOST_API_VERSION};
use axum::Json;
use serde::Serialize;
use std::path::PathBuf;

fn modules_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vessel")
        .join("modules")
}

#[derive(Serialize)]
pub struct ModuleInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub description: String,
}

pub async fn list_modules() -> Json<Vec<ModuleInfo>> {
    let dir = modules_dir();
    let mut result = Vec::new();

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Json(result);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(manifest) = load_manifest(&path) {
            result.push(ModuleInfo {
                id: manifest.id,
                name: manifest.name,
                version: manifest.version,
                api_version: manifest.api_version,
                description: manifest.description,
            });
        }
    }

    Json(result)
}

#[derive(Serialize)]
pub struct ApiVersionInfo {
    pub host_api_version: u32,
}

pub async fn api_version() -> Json<ApiVersionInfo> {
    Json(ApiVersionInfo {
        host_api_version: HOST_API_VERSION,
    })
}
