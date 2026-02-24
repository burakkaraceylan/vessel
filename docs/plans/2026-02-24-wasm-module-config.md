# WASM Module Config Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:executing-plans to implement this plan task-by-task.

**Goal:** Add a `config_get` host function that lets WASM modules read admin-provided config values from `config.toml`, distinct from mutable runtime storage.

**Architecture:** A new `config-get` WIT function backed by a `HashMap<String, String>` in `HostData`, populated from the module's `[modules.<id>]` section in `config.toml` at load time. No permission required — config only exposes what the admin explicitly wrote.

**Tech Stack:** Rust, wasmtime component model, wit-bindgen, toml crate

---

### Task 1: Add `config-get` to both WIT files

There are **two separate copies** of the WIT file: one for the host and one inside the WASM module crate. Both must be updated.

**Files:**
- Modify: `wit/vessel-host.wit` (host-side, used by `src/wasm/host.rs`)
- Modify: `modules/home-assistant/wit/vessel-host.wit` (guest-side, used by the HA crate)

**Step 1: Add `config-get` to the host WIT**

In `wit/vessel-host.wit`, find the `// ── Storage` section and add `config-get` just above it:

```wit
    // ── Config (read-only admin values from config.toml) ──────────────────
    config-get: func(key: string) -> option<string>;

    // ── Storage (auto-namespaced by module id on the host side) ───────────
```

**Step 2: Apply the same change to the module's WIT copy**

In `modules/home-assistant/wit/vessel-host.wit`, make the identical addition in the same position.

**Step 3: Verify both files compile**

```bash
cargo check
```

Expected: compile errors in `src/wasm/host.rs` — `config_get` method not yet implemented. That's expected and correct — the trait now requires it.

**Step 4: Commit**

```bash
git add wit/vessel-host.wit modules/home-assistant/wit/vessel-host.wit
git commit -m "feat(wasm): add config-get to WIT interface"
```

---

### Task 2: Implement `config_get` on the host

**Files:**
- Modify: `src/wasm/host.rs`

**Step 1: Add `config` field to `HostData`**

In the `HostData` struct (around line 13), add:

```rust
pub config: std::collections::HashMap<String, String>,
```

Place it after `storage_dir` for logical grouping.

**Step 2: Implement the `config_get` host function**

In `impl vessel::host::host::Host for HostData`, add the method. `config_get` is synchronous in nature but the bindgen generates an `async fn`:

```rust
async fn config_get(&mut self, key: String) -> Option<String> {
    self.config.get(&key).cloned()
}
```

**Step 3: Verify**

```bash
cargo check
```

Expected: error in `src/wasm/module.rs` — `HostData` construction missing `config` field. Move to next task.

---

### Task 3: Thread config through `WasmModule`

**Files:**
- Modify: `src/wasm/module.rs`
- Modify: `src/main.rs`

**Step 1: Update `WasmModule::load` signature**

Change `pub fn load(module_dir: PathBuf) -> anyhow::Result<Self>` to accept a config table:

```rust
pub fn load(module_dir: PathBuf, config: toml::Table) -> anyhow::Result<Self> {
```

Add a `config` field to the `WasmModule` struct:

```rust
pub struct WasmModule {
    manifest: ModuleManifest,
    wasm_path: PathBuf,
    engine: Engine,
    name_static: &'static str,
    config: std::collections::HashMap<String, String>,
}
```

**Step 2: Convert toml::Table to HashMap<String, String> at load time**

Add a helper function at the bottom of `module.rs`:

```rust
fn toml_to_string_map(table: &toml::Table) -> std::collections::HashMap<String, String> {
    table.iter().map(|(k, v)| {
        let s = match v {
            toml::Value::String(s) => s.clone(),
            toml::Value::Integer(i) => i.to_string(),
            toml::Value::Float(f) => f.to_string(),
            toml::Value::Boolean(b) => b.to_string(),
            other => other.to_string(),
        };
        (k.clone(), s)
    }).collect()
}
```

In `WasmModule::load`, after computing `engine`:

```rust
let config_map = toml_to_string_map(&config);
Ok(WasmModule { manifest, wasm_path, engine, name_static, config: config_map })
```

**Step 3: Pass config into `HostData` in `run()`**

In `WasmModule::run`, the `HostData` construction currently has no `config` field. Add it:

```rust
let host_data = HostData {
    // ... existing fields ...
    config: self.config.clone(),
};
```

**Step 4: Update `load_wasm_modules` in `main.rs`**

Change the function signature to accept the config:

```rust
fn load_wasm_modules(manager: &mut ModuleManager, config: &config::Config) {
```

Inside the loop, after loading the manifest (which happens inside `WasmModule::load` — we need the module id first). The cleanest approach: load the manifest separately to get the id, then look up config. But `WasmModule::load` already loads the manifest internally.

Instead, look up config by directory name (which equals the module id by convention) before calling `load`:

```rust
for entry in entries.flatten() {
    let path = entry.path();
    if !path.is_dir() || !path.join("module.wasm").exists() {
        continue;
    }
    // Directory name is the module id
    let dir_name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let module_config = config.modules
        .get(dir_name)
        .cloned()
        .unwrap_or_default();

    match WasmModule::load(path.clone(), module_config) {
        Ok(module) => {
            println!("[vessel] loaded WASM module: {}", module.name());
            manager.register_module(Box::new(module));
        }
        Err(e) => {
            eprintln!("[vessel] failed to load WASM module at {}: {}", path.display(), e);
        }
    }
}
```

**Step 5: Update the call site in `main`**

```rust
load_wasm_modules(&mut module_manager, &config);
```

**Step 6: Verify**

```bash
cargo check
```

Expected: clean compile (or only errors in the HA module, which is a separate crate).

**Step 7: Commit**

```bash
git add src/wasm/host.rs src/wasm/module.rs src/main.rs
git commit -m "feat(wasm): implement config_get host function backed by config.toml"
```

---

### Task 4: Update the home-assistant WASM module

**Files:**
- Modify: `modules/home-assistant/src/lib.rs`

The module already has the updated WIT (from Task 1), so `wit_bindgen` will regenerate and expose `config_get`. We just need to call it instead of `storage_get`.

**Step 1: Update `on_load`**

Change:
```rust
let ha_url = storage_get("url")
    .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".to_string());
```
To:
```rust
let ha_url = config_get("url")
    .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".to_string());
```

**Step 2: Update `on_websocket_message`**

Change:
```rust
let token = storage_get("token").unwrap_or_default();
```
To:
```rust
let token = config_get("token").unwrap_or_default();
```

**Step 3: Update `on_timer`**

Change:
```rust
let ha_url = storage_get("url")
    .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".to_string());
```
To:
```rust
let ha_url = config_get("url")
    .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".to_string());
```

**Step 4: Build the WASM module**

```bash
cd modules/home-assistant
cargo build --target wasm32-wasip2 --release
```

Expected: successful build producing `target/wasm32-wasip2/release/home_assistant.wasm`.

**Step 5: Commit**

```bash
git add modules/home-assistant/src/lib.rs
git commit -m "feat(home-assistant): use config_get for url and token"
```

---

### Task 5: Add example config to `config.toml`

**Files:**
- Modify: `config.toml`

**Step 1: Add the home-assistant section**

Append to `config.toml`:

```toml
[modules.home-assistant]
url = "ws://homeassistant.local:8123/api/websocket"
token = "your-long-lived-access-token-here"
```

**Step 2: Verify the server starts**

```bash
cargo run
```

Expected: server starts, home-assistant WASM module loads (if the `.wasm` file is in place), logs `Home Assistant: loading`.

**Step 3: Commit**

```bash
git add config.toml
git commit -m "chore: add home-assistant module config example to config.toml"
```

---

## Notes

- The WASM module's directory name in `%APPDATA%/Local/vessel/modules/` must match its id in `config.toml` (e.g. directory `home-assistant` maps to `[modules.home-assistant]`). This is the same convention the manifest `id` field uses.
- `config_get` requires no permission entry in `manifest.json` — it only exposes values the admin explicitly set.
- Non-string toml values (integers, booleans) are coerced to their string representation so modules can use them uniformly via `config_get`.
