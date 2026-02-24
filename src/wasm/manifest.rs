use serde::Deserialize;
use std::path::Path;
use sha2::{Sha256, Digest};
use anyhow::{Context, bail};

pub const HOST_API_VERSION: u32 = 1;

#[derive(Deserialize, Debug, Clone)]
pub struct ModuleManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub permissions: Permissions,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Permissions {
    #[serde(default)]
    pub subscribe: Vec<String>,
    #[serde(default)]
    pub call: Vec<String>,
    #[serde(default)]
    pub network: NetworkPermissions,
    #[serde(default)]
    pub storage: bool,
    #[serde(default)]
    pub timers: bool,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct NetworkPermissions {
    #[serde(default)]
    pub http: bool,
    #[serde(default)]
    pub websocket: bool,
    #[serde(default)]
    pub tcp: bool,
}

/// Loads and validates a module manifest from `module_dir/manifest.json`.
/// Checks api_version compatibility and verifies the tamper-detection hash if present.
pub fn load_manifest(module_dir: &Path) -> anyhow::Result<ModuleManifest> {
    let manifest_path = module_dir.join("manifest.json");
    let wasm_path = module_dir.join("module.wasm");
    let hash_path = module_dir.join("manifest.hash");

    let manifest_bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let wasm_bytes = std::fs::read(&wasm_path)
        .with_context(|| format!("reading {}", wasm_path.display()))?;

    // Tamper detection: if a hash file exists, verify it.
    // The hash is written at install time by `write_hash()`. Modules that have
    // never been hashed (e.g. hand-placed dev modules) are loaded without verification.
    if hash_path.exists() {
        let stored_hash = std::fs::read_to_string(&hash_path)
            .context("reading manifest.hash")?;
        let computed = compute_hash(&manifest_bytes, &wasm_bytes);
        if stored_hash.trim() != computed {
            bail!("Module tamper detected: hash mismatch for {}", module_dir.display());
        }
    }

    let manifest: ModuleManifest = serde_json::from_slice(&manifest_bytes)
        .context("parsing manifest.json")?;

    if manifest.api_version > HOST_API_VERSION {
        bail!(
            "Module '{}' requires api_version {} but host only supports {}",
            manifest.id, manifest.api_version, HOST_API_VERSION
        );
    }

    Ok(manifest)
}

/// Writes the tamper-detection hash for a freshly installed module.
pub fn write_hash(module_dir: &Path) -> anyhow::Result<()> {
    let manifest_bytes = std::fs::read(module_dir.join("manifest.json"))?;
    let wasm_bytes = std::fs::read(module_dir.join("module.wasm"))?;
    let hash = compute_hash(&manifest_bytes, &wasm_bytes);
    std::fs::write(module_dir.join("manifest.hash"), hash)?;
    Ok(())
}

fn compute_hash(manifest: &[u8], wasm: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(manifest);
    hasher.update(wasm);
    format!("{:x}", hasher.finalize())
}
