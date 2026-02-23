# WASM Module System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:executing-plans to implement this plan task-by-task.

**Goal:** Add a WASM extension system to Vessel so integrations (Home Assistant, Bambu, Spotify, etc.) can be installed as `.wasm` binaries without recompiling Vessel.

**Architecture:** `WasmModule` implements the existing `Module` trait — `ModuleManager` treats it identically to native modules. WASM components are defined via the Component Model (WIT interface + `wasmtime::component::bindgen!`), with a `CapabilityValidator` enforcing deny-by-default permissions declared in each module's `manifest.json`. Protocol stabilization (versioned wire format) lands first as the foundation.

**Tech Stack:** Rust / Wasmtime (component model + async) / wit-bindgen / tokio / reqwest / tokio-tungstenite / sha2 / glob

**Design doc:** `docs/plans/2026-02-24-wasm-module-system-design.md`

---

## Phase 1 — Protocol Stabilization

### Task 1: Redesign `src/protocol.rs`

**Context:** The current wire format is flat JSON with no `type` discriminator, no versioning, and no request/response correlation. This adds a unified envelope used by both native modules and WASM modules.

**Files:**
- Modify: `src/protocol.rs`

**Step 1: Replace the entire file with the new protocol types**

```rust
use crate::module::ModuleEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Client → Vessel
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IncomingMessage {
    /// Fire-and-forget command routed to a module.
    Call {
        request_id: String,
        module: String,
        name: String,
        #[serde(default = "default_version")]
        version: u32,
        #[serde(default)]
        params: Value,
    },
    /// Ask to receive future events matching this module+name.
    /// Wildcards in `name` use glob syntax: "voice.*"
    Subscribe {
        module: String,
        name: String,
    },
}

fn default_version() -> u32 { 1 }

/// Vessel → Client
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutgoingMessage {
    Event {
        module: &'static str,
        name: String,
        #[serde(default)]
        version: u32,
        data: Value,
        timestamp: u64,
    },
    Response {
        request_id: String,
        success: bool,
        data: Value,
    },
}

impl From<ModuleEvent> for OutgoingMessage {
    fn from(event: ModuleEvent) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        OutgoingMessage::Event {
            module: event.source(),
            name: event.event_name().to_owned(),
            version: 1,
            data: event.data().clone(),
            timestamp,
        }
    }
}
```

**Step 2: Compile**

```bash
cargo build 2>&1 | head -40
```

Expected: compile errors in `src/vessel.rs` referencing `msg.module` / `msg.action` — those are addressed in Task 2.

**Step 3: Commit**

```bash
git add src/protocol.rs
git commit -m "feat: versioned wire protocol with type discriminator and request_id"
```

---

### Task 2: Update `src/vessel.rs` to use the new protocol

**Files:**
- Modify: `src/vessel.rs`

**Step 1: Replace the `handle_websocket` parsing block**

Find the `match serde_json::from_str::<IncomingMessage>(line)` block and replace the arm bodies:

```rust
// Old field access:  msg.module, msg.action, msg.params
// New: pattern match on the enum variant

match serde_json::from_str::<IncomingMessage>(line) {
    Ok(IncomingMessage::Call { request_id, module, name, version: _, params }) => {
        println!("Call: module='{}', name='{}'", module, name);
        if let Err(e) = state.module_manager.route_command(&module, name, params).await {
            eprintln!("Route error: {}", e);
        }
        // TODO Phase 2: send Response back over socket with request_id
        let _ = request_id;
    }
    Ok(IncomingMessage::Subscribe { module, name }) => {
        // Subscription filtering is handled per-client in a future task;
        // for now subscriptions are implicit (all events are broadcast to all clients).
        println!("Subscribe: module='{}', name='{}'", module, name);
    }
    Err(e) => {
        eprintln!("Invalid message: {}", e);
    }
}
```

**Step 2: Compile and verify no errors**

```bash
cargo build 2>&1
```

Expected: clean build.

**Step 3: Manual smoke test**
- `cargo run`
- Open `ws://127.0.0.1:8080/ws` in a WebSocket client (e.g. Postman or websocat)
- Send: `{"type":"call","request_id":"test-1","module":"discord","name":"get_voice_settings","version":1,"params":{}}`
- Verify the server doesn't crash and logs: `Call: module='discord', name='get_voice_settings'`

**Step 4: Commit**

```bash
git add src/vessel.rs
git commit -m "feat: update WebSocket handler for new protocol envelope"
```

---

### Task 3: Update the React WebSocket client

**Context:** `connection.ts` sends old-format ActionBinding objects and reads `msg.module / msg.event / msg.data`. Both need updating to match the new protocol.

**Files:**
- Modify: `ui/src/stores/connection.ts`
- Modify: `ui/src/stores/moduleState.ts` (check if it reads `msg.event` key)

**Step 1: Update `connection.ts`**

```typescript
import { create } from "zustand";
import type { ActionBinding } from "@/types/widget";
import { useModuleStateStore } from "./moduleState";

// Stable shape for outgoing calls
interface CallMessage {
  type: "call";
  request_id: string;
  module: string;
  name: string;
  version: number;
  params: Record<string, unknown>;
}

interface ConnectionState {
  status: "connected" | "disconnected" | "connecting";
  ws: WebSocket | null;
  connect: (url: string) => void;
  disconnect: () => void;
  sendAction: (action: ActionBinding) => void;
}

function generateId(): string {
  return Math.random().toString(36).slice(2, 10);
}

export const useConnectionStore = create<ConnectionState>((set, get) => ({
  status: "disconnected",
  ws: null,

  connect: (url: string) => {
    get().ws?.close();
    const ws = new WebSocket(url);

    ws.onopen = () => set({ status: "connected", ws });
    ws.onclose = () => set({ status: "disconnected", ws: null });

    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      if (msg.type === "event") {
        // New format: { type, module, name, version, data, timestamp }
        useModuleStateStore.getState().handleEvent(msg.module, msg.name, msg.data);
      }
      // type === "response" is ignored for now (no pending request tracking yet)
    };

    set({ status: "connecting", ws });
  },

  disconnect: () => {
    get().ws?.close();
    set({ status: "disconnected", ws: null });
  },

  sendAction: (action: ActionBinding) => {
    const ws = get().ws;
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      console.warn("WebSocket not connected");
      return;
    }
    const msg: CallMessage = {
      type: "call",
      request_id: generateId(),
      module: action.module,
      name: action.action,   // ActionBinding still uses "action" field
      version: 1,
      params: action.params ?? {},
    };
    ws.send(JSON.stringify(msg));
  },
}));
```

**Step 2: Check `moduleState.ts` still works**

The `handleEvent(module, eventName, data)` signature is unchanged — only the caller changed from `msg.event` → `msg.name`. Verify:

```bash
cd ui && npm run build 2>&1 | tail -20
```

Expected: clean build, no type errors.

**Step 3: End-to-end test**
- `cargo run` (backend)
- `cd ui && npm run dev` (frontend)
- Open the UI, verify Discord state still loads (the snapshot replay still works because `OutgoingMessage::Event` serializes to `{ type: "event", module, name, ... }`)

**Step 4: Commit**

```bash
cd ui && git add -A && cd .. && git add ui/
git commit -m "feat: update React WS client for versioned protocol"
```

---

## Phase 2 — WIT Interface + Wasmtime Foundation

### Task 4: Create the WIT interface file

**Context:** This is the stable ABI contract between Vessel (host) and WASM modules (guests). Every host function lives here. Changing this file in a breaking way increments `HOST_API_VERSION`.

**Files:**
- Create: `wit/vessel-host.wit`

```wit
package vessel:host@1.0.0;

interface types {
    record event {
        module: string,
        name: string,
        version: u32,
        /// JSON-encoded payload — avoids the WIT interface changing when module payloads change.
        data: string,
        timestamp: u64,
    }

    record http-request {
        method: string,
        url: string,
        headers: list<tuple<string, string>>,
        body: option<string>,
    }

    record http-response {
        status: u32,
        headers: list<tuple<string, string>>,
        body: string,
    }
}

/// Functions the host (Vessel) provides to the WASM module.
interface host {
    use types.{event, http-request, http-response};

    // ── Event bus ──────────────────────────────────────────────────────────
    /// Subscribe to events matching `pattern` (glob: "discord.voice.*").
    subscribe: func(pattern: string) -> result<_, string>;
    /// Emit a new event onto the Vessel event bus.
    emit: func(event: event) -> result<_, string>;

    // ── Driver calls ───────────────────────────────────────────────────────
    /// Route a command to a native module (e.g., call discord set_mute).
    /// `params` is a JSON string. Returns JSON string on success.
    call: func(module: string, name: string, version: u32, params: string) -> result<string, string>;

    // ── Network ────────────────────────────────────────────────────────────
    http-request: func(req: http-request) -> result<http-response, string>;
    /// Returns an opaque connection handle.
    websocket-connect: func(url: string) -> result<u32, string>;
    websocket-send: func(handle: u32, message: string) -> result<_, string>;
    websocket-close: func(handle: u32) -> result<_, string>;

    // ── Storage (auto-namespaced by module id) ─────────────────────────────
    storage-get: func(key: string) -> option<string>;
    storage-set: func(key: string, value: string) -> result<_, string>;
    storage-delete: func(key: string) -> result<_, string>;

    // ── Timers ─────────────────────────────────────────────────────────────
    /// Returns an opaque timer handle.
    set-timeout: func(ms: u64) -> u32;
    set-interval: func(ms: u64) -> u32;
    clear-timer: func(handle: u32);

    // ── Logging ────────────────────────────────────────────────────────────
    log: func(level: string, message: string);
}

/// Functions the WASM module must export to the host.
interface guest {
    use types.{event};

    on-load: func() -> result<_, string>;
    on-unload: func() -> result<_, string>;
    on-event: func(event: event) -> result<_, string>;
    /// `params` is a JSON string. Returns JSON string response.
    on-command: func(action: string, params: string) -> result<string, string>;
    on-timer: func(handle: u32) -> result<_, string>;
    on-websocket-message: func(handle: u32, message: string) -> result<_, string>;
}

world vessel-module {
    import host;
    export guest;
}
```

**Step 2: Commit the WIT file**

```bash
git add wit/
git commit -m "feat: add vessel-host.wit component model interface"
```

---

### Task 5: Add Wasmtime dependencies to `Cargo.toml`

**Context:** `wasmtime` is the WASM runtime. The `component-model` feature enables the WIT-based component model. `async` allows host functions to suspend the WASM fiber while performing tokio async work. `sha2` is for manifest tamper detection. `glob` is for capability pattern matching.

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add to `[dependencies]`**

```toml
wasmtime = { version = "28", features = ["component-model", "async"] }
sha2 = "0.10"
glob = "0.3"
```

> Check [crates.io/crates/wasmtime](https://crates.io/crates/wasmtime) for the latest stable version before pinning.

**Step 2: Verify dependencies resolve**

```bash
cargo fetch 2>&1
```

Expected: all packages fetched without conflicts.

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add wasmtime, sha2, glob dependencies"
```

---

### Task 6: Create the `src/wasm/` module structure + manifest types

**Context:** All WASM host machinery lives in `src/wasm/`. Start by defining the manifest struct (what gets parsed from `manifest.json`) and the module-level `pub mod` declarations.

**Files:**
- Create: `src/wasm/mod.rs`
- Create: `src/wasm/manifest.rs`
- Modify: `src/modules.rs` or `src/main.rs` to add `pub mod wasm;` (whichever owns top-level modules)

**Step 1: Create `src/wasm/mod.rs`**

```rust
pub mod capability;
pub mod manifest;
pub mod module;
pub mod host;

pub use module::WasmModule;
```

**Step 2: Create `src/wasm/manifest.rs`**

```rust
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
```

**Step 3: Add `pub mod wasm;` to `src/main.rs`**

Find the existing `mod` declarations at the top of `src/main.rs` and add:

```rust
mod wasm;
```

**Step 4: Compile**

```bash
cargo build 2>&1 | grep -E "^error"
```

Expected: `src/wasm/capability.rs`, `src/wasm/module.rs`, `src/wasm/host.rs` are missing — add stub files to silence:

```bash
# Create stubs so the build passes before implementing each file
touch src/wasm/capability.rs src/wasm/module.rs src/wasm/host.rs
```

Re-run `cargo build` — expected: clean.

**Step 5: Commit**

```bash
git add src/wasm/ src/main.rs wit/
git commit -m "feat: add wasm module structure and manifest loader"
```

---

## Phase 3 — Capability Enforcement

### Task 7: Implement `src/wasm/capability.rs`

**Context:** `CapabilityValidator` is constructed from a manifest at load time. Every host function checks it before executing. Denial returns a `String` error to the WASM guest via the `result<_, string>` WIT return type.

**Files:**
- Modify: `src/wasm/capability.rs`

**Step 1: Write the implementation**

```rust
use crate::wasm::manifest::Permissions;
use glob::Pattern;
use std::collections::HashSet;

#[derive(Debug)]
pub enum CapabilityError {
    Denied(String),
}

impl std::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapabilityError::Denied(msg) => write!(f, "capability denied: {}", msg),
        }
    }
}

pub struct CapabilityValidator {
    subscribe_patterns: Vec<Pattern>,
    allowed_calls: HashSet<String>,
    pub network_http: bool,
    pub network_websocket: bool,
    pub network_tcp: bool,
    pub storage: bool,
    pub timers: bool,
}

impl CapabilityValidator {
    pub fn from_permissions(perms: &Permissions) -> Self {
        let subscribe_patterns = perms
            .subscribe
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        // Allowed calls are stored as "module.name@version" strings.
        // e.g. "discord.voice.set_mute@1"
        let allowed_calls = perms.call.iter().cloned().collect();

        CapabilityValidator {
            subscribe_patterns,
            allowed_calls,
            network_http: perms.network.http,
            network_websocket: perms.network.websocket,
            network_tcp: perms.network.tcp,
            storage: perms.storage,
            timers: perms.timers,
        }
    }

    pub fn check_subscribe(&self, pattern: &str) -> Result<(), CapabilityError> {
        // The module's declared subscribe patterns must cover what it's trying to subscribe to.
        let allowed = self.subscribe_patterns.iter().any(|p| p.matches(pattern));
        if !allowed {
            return Err(CapabilityError::Denied(format!(
                "subscribe '{}' not declared in manifest",
                pattern
            )));
        }
        Ok(())
    }

    pub fn check_call(&self, module: &str, name: &str, version: u32) -> Result<(), CapabilityError> {
        let key = format!("{}.{}@{}", module, name, version);
        if !self.allowed_calls.contains(&key) {
            return Err(CapabilityError::Denied(format!(
                "call '{}.{}@{}' not declared in manifest",
                module, name, version
            )));
        }
        Ok(())
    }

    pub fn check_network_http(&self) -> Result<(), CapabilityError> {
        if !self.network_http {
            return Err(CapabilityError::Denied("network.http not declared".into()));
        }
        Ok(())
    }

    pub fn check_network_websocket(&self) -> Result<(), CapabilityError> {
        if !self.network_websocket {
            return Err(CapabilityError::Denied("network.websocket not declared".into()));
        }
        Ok(())
    }

    pub fn check_storage(&self) -> Result<(), CapabilityError> {
        if !self.storage {
            return Err(CapabilityError::Denied("storage not declared".into()));
        }
        Ok(())
    }

    pub fn check_timers(&self) -> Result<(), CapabilityError> {
        if !self.timers {
            return Err(CapabilityError::Denied("timers not declared".into()));
        }
        Ok(())
    }
}
```

**Step 2: Compile**

```bash
cargo build 2>&1 | grep "^error"
```

Expected: clean.

**Step 3: Commit**

```bash
git add src/wasm/capability.rs
git commit -m "feat: add CapabilityValidator with deny-by-default enforcement"
```

---

## Phase 4 — WasmModule + Host Functions

### Task 8: Implement `src/wasm/host.rs` — host data and event bus + logging

**Context:** Wasmtime's component model generates a Rust trait from the WIT `host` interface. You implement that trait on a struct (`HostData`) that holds runtime state (capability validator, event publisher, etc.). Each method is called when the WASM guest calls the corresponding function.

The `wasmtime::component::bindgen!` macro reads the WIT file at **compile time** and generates typed Rust interfaces. This is how Wasmtime enforces type safety across the host/guest boundary — no unsafe memory manipulation required.

**Files:**
- Modify: `src/wasm/host.rs`

**Step 1: Write the host data and bindgen invocation**

```rust
use crate::module::{EventPublisher, ModuleEvent};
use crate::wasm::capability::CapabilityValidator;
use std::sync::Arc;
use tokio::sync::mpsc;

// ── Generated bindings ────────────────────────────────────────────────────
//
// This macro reads wit/vessel-host.wit at compile time and generates:
//   - `vessel::host::Host` trait (you implement this below)
//   - `VesselModule` struct with methods like `call_on_load()`, `call_on_event()`
//   - All WIT record types as Rust structs
//
// `with` maps WIT interface names to Rust modules so the generated code
// knows where to find your implementation.
wasmtime::component::bindgen!({
    world: "vessel-module",
    path: "wit/vessel-host.wit",
    async: true,
});

// ── Host state ─────────────────────────────────────────────────────────────

pub struct HostData {
    pub module_id: String,
    pub capability: Arc<CapabilityValidator>,
    pub event_publisher: EventPublisher,
    /// Sends timer-fire notifications into the WasmModule run loop.
    pub timer_tx: mpsc::Sender<u32>,
    /// Sends WebSocket messages into the WasmModule run loop.
    pub ws_tx: mpsc::Sender<(u32, String)>,
    /// Subscribed glob patterns (accumulated via subscribe() calls).
    pub subscriptions: Vec<String>,
    /// Storage root: vessel-data/modules/<id>/storage/
    pub storage_dir: std::path::PathBuf,
}

// ── Host trait implementation ──────────────────────────────────────────────

#[async_trait::async_trait]
impl vessel::host::Host for HostData {
    async fn subscribe(&mut self, pattern: String) -> wasmtime::Result<Result<(), String>> {
        if let Err(e) = self.capability.check_subscribe(&pattern) {
            return Ok(Err(e.to_string()));
        }
        self.subscriptions.push(pattern);
        Ok(Ok(()))
    }

    async fn emit(&mut self, event: vessel::host::types::Event) -> wasmtime::Result<Result<(), String>> {
        let data: serde_json::Value = serde_json::from_str(&event.data)
            .unwrap_or(serde_json::Value::Null);
        // WASM modules emit as Transient events (not stateful — WASM manages its own state)
        self.event_publisher.send(ModuleEvent::Transient {
            source: Box::leak(self.module_id.clone().into_boxed_str()),
            event: event.name,
            data,
        });
        Ok(Ok(()))
    }

    async fn log(&mut self, level: String, message: String) -> wasmtime::Result<()> {
        println!("[{}] [{}] {}", level.to_uppercase(), self.module_id, message);
        Ok(())
    }

    // ── Stub implementations for capabilities added in later tasks ──────────

    async fn call(
        &mut self,
        _module: String, _name: String, _version: u32, _params: String,
    ) -> wasmtime::Result<Result<String, String>> {
        Ok(Err("driver calls not yet implemented".into()))
    }

    async fn http_request(
        &mut self,
        _req: vessel::host::types::HttpRequest,
    ) -> wasmtime::Result<Result<vessel::host::types::HttpResponse, String>> {
        Ok(Err("http not yet implemented".into()))
    }

    async fn websocket_connect(&mut self, _url: String) -> wasmtime::Result<Result<u32, String>> {
        Ok(Err("websocket not yet implemented".into()))
    }

    async fn websocket_send(&mut self, _handle: u32, _message: String) -> wasmtime::Result<Result<(), String>> {
        Ok(Err("websocket not yet implemented".into()))
    }

    async fn websocket_close(&mut self, _handle: u32) -> wasmtime::Result<Result<(), String>> {
        Ok(Err("websocket not yet implemented".into()))
    }

    async fn storage_get(&mut self, _key: String) -> wasmtime::Result<Option<String>> {
        Ok(None)
    }

    async fn storage_set(&mut self, _key: String, _value: String) -> wasmtime::Result<Result<(), String>> {
        Ok(Err("storage not yet implemented".into()))
    }

    async fn storage_delete(&mut self, _key: String) -> wasmtime::Result<Result<(), String>> {
        Ok(Err("storage not yet implemented".into()))
    }

    async fn set_timeout(&mut self, _ms: u64) -> wasmtime::Result<u32> {
        Ok(0)
    }

    async fn set_interval(&mut self, _ms: u64) -> wasmtime::Result<u32> {
        Ok(0)
    }

    async fn clear_timer(&mut self, _handle: u32) -> wasmtime::Result<()> {
        Ok(())
    }
}
```

> **Note on `Box::leak`:** The `ModuleEvent::Transient` source field is `&'static str`. For WASM-emitted events, we need to convert the module ID string to a `'static` reference. `Box::leak` does this safely — it leaks a small allocation. For a small number of modules this is acceptable; a future improvement would be to intern module ID strings.

**Step 2: Compile**

```bash
cargo build 2>&1 | grep "^error"
```

The `bindgen!` macro may produce errors if WIT type names don't exactly match. Fix any generated type name mismatches (the compiler errors will tell you the exact expected names).

**Step 3: Commit**

```bash
git add src/wasm/host.rs
git commit -m "feat: add WasmHost with bindgen! and event bus + logging implementation"
```

---

### Task 9: Implement `src/wasm/module.rs` — WasmModule skeleton

**Context:** `WasmModule` is the struct that implements the `Module` trait. It holds a Wasmtime `Engine` and the manifest. Its `run()` method instantiates the component, calls `on_load()`, then enters the main event dispatch loop.

**Files:**
- Modify: `src/wasm/module.rs`

**Step 1: Write WasmModule**

```rust
use crate::module::{Module, ModuleContext, ModuleEvent};
use crate::wasm::capability::CapabilityValidator;
use crate::wasm::host::{HostData, VesselModule};
use crate::wasm::manifest::{load_manifest, ModuleManifest};
use anyhow::Context;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use wasmtime::component::Component;
use wasmtime::{Config, Engine, Store};

pub struct WasmModule {
    manifest: ModuleManifest,
    wasm_path: PathBuf,
    engine: Engine,
}

impl WasmModule {
    pub fn load(module_dir: PathBuf) -> anyhow::Result<Self> {
        let manifest = load_manifest(&module_dir)?;
        let wasm_path = module_dir.join("module.wasm");

        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        let engine = Engine::new(&config)?;

        Ok(WasmModule { manifest, wasm_path, engine })
    }
}

#[async_trait]
impl Module for WasmModule {
    async fn new(_config: toml::Table) -> anyhow::Result<Self> {
        // WasmModule is constructed via WasmModule::load(), not via the trait.
        // This method is required by the trait but not used for WASM modules.
        anyhow::bail!("Use WasmModule::load() to construct WASM modules")
    }

    fn name(&self) -> &'static str {
        // Safety: module IDs are loaded once at startup and live for the process lifetime.
        Box::leak(self.manifest.id.clone().into_boxed_str())
    }

    async fn run(&self, ctx: ModuleContext) -> anyhow::Result<()> {
        let capability = Arc::new(CapabilityValidator::from_permissions(&self.manifest.permissions));

        // Channels for timer and websocket callbacks back into this run loop
        let (timer_tx, mut timer_rx) = mpsc::channel::<u32>(32);
        let (ws_tx, mut ws_rx) = mpsc::channel::<(u32, String)>(32);

        // Storage directory
        let storage_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("vessel")
            .join("modules")
            .join(&self.manifest.id)
            .join("storage");
        std::fs::create_dir_all(&storage_dir)?;

        let host_data = HostData {
            module_id: self.manifest.id.clone(),
            capability: capability.clone(),
            event_publisher: ctx.event_tx.clone(),
            timer_tx,
            ws_tx,
            subscriptions: Vec::new(),
            storage_dir,
        };

        // ── Instantiate the WASM component ────────────────────────────────
        let wasm_bytes = std::fs::read(&self.wasm_path)
            .with_context(|| format!("reading {}", self.wasm_path.display()))?;
        let component = Component::from_binary(&self.engine, &wasm_bytes)?;

        let mut linker = wasmtime::component::Linker::new(&self.engine);
        VesselModule::add_to_linker(&mut linker, |state: &mut HostData| state)?;

        let mut store = Store::new(&self.engine, host_data);
        let (bindings, _instance) = VesselModule::instantiate_async(&mut store, &component, &linker).await?;

        // ── on_load ───────────────────────────────────────────────────────
        match bindings.vessel_host_guest().call_on_load(&mut store).await? {
            Ok(()) => {}
            Err(msg) => {
                eprintln!("[{}] on_load failed: {}", self.manifest.id, msg);
                return Ok(());
            }
        }

        // ── Subscribe to events matching the module's declared patterns ────
        let mut event_rx = ctx.event_tx.subscribe();

        // ── Main dispatch loop ─────────────────────────────────────────────
        let mut command_rx = ctx.rx;
        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => break,

                // Incoming command from UI
                Some(cmd) = command_rx.recv() => {
                    let params_json = serde_json::to_string(&cmd.params).unwrap_or_default();
                    match bindings.vessel_host_guest()
                        .call_on_command(&mut store, &cmd.action, &params_json)
                        .await
                    {
                        Ok(Ok(_response)) => {}
                        Ok(Err(e)) => eprintln!("[{}] on_command error: {}", self.manifest.id, e),
                        Err(e) => eprintln!("[{}] on_command trap: {}", self.manifest.id, e),
                    }
                }

                // Subscribed event from event bus
                Ok(event) = event_rx.recv() => {
                    let subs = store.data().subscriptions.clone();
                    let event_key = format!("{}.{}", event.source(), event.event_name());
                    let matches = subs.iter().any(|p| {
                        glob::Pattern::new(p).map(|pat| pat.matches(&event_key)).unwrap_or(false)
                    });
                    if matches {
                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let wit_event = crate::wasm::host::vessel::host::types::Event {
                            module: event.source().to_string(),
                            name: event.event_name().to_string(),
                            version: 1,
                            data: serde_json::to_string(event.data()).unwrap_or_default(),
                            timestamp: ts,
                        };
                        let _ = bindings.vessel_host_guest()
                            .call_on_event(&mut store, &wit_event)
                            .await;
                    }
                }

                // Timer fired
                Some(handle) = timer_rx.recv() => {
                    let _ = bindings.vessel_host_guest()
                        .call_on_timer(&mut store, handle)
                        .await;
                }

                // WebSocket message
                Some((handle, message)) = ws_rx.recv() => {
                    let _ = bindings.vessel_host_guest()
                        .call_on_websocket_message(&mut store, handle, &message)
                        .await;
                }
            }
        }

        // ── on_unload ──────────────────────────────────────────────────────
        let _ = bindings.vessel_host_guest().call_on_unload(&mut store).await;
        Ok(())
    }
}
```

**Step 2: Compile — fix any bindgen-generated method name discrepancies**

```bash
cargo build 2>&1 | head -60
```

The `bindgen!` macro generates method names from the WIT kebab-case names. If the compiler reports "method not found", cross-reference the generated code:

```bash
# Find the generated bindings to see exact method names
cargo build 2>&1 | grep "help: items from traits"
```

**Step 3: Commit skeleton (even if host functions are stubs)**

```bash
git add src/wasm/module.rs
git commit -m "feat: WasmModule skeleton implements Module trait with component model dispatch"
```

---

### Task 10: Implement storage host functions

**Files:**
- Modify: `src/wasm/host.rs` — replace the stub `storage_*` implementations

**Step 1: Replace storage stubs**

```rust
async fn storage_get(&mut self, key: String) -> wasmtime::Result<Option<String>> {
    if let Err(_) = self.capability.check_storage() {
        return Ok(None);
    }
    let path = self.storage_dir.join(sanitize_key(&key));
    Ok(std::fs::read_to_string(path).ok())
}

async fn storage_set(&mut self, key: String, value: String) -> wasmtime::Result<Result<(), String>> {
    if let Err(e) = self.capability.check_storage() {
        return Ok(Err(e.to_string()));
    }
    let path = self.storage_dir.join(sanitize_key(&key));
    std::fs::write(path, value).map_err(|e| e.to_string()).map(Ok).map_err(|e| {
        wasmtime::Error::msg(e)
    })
}

async fn storage_delete(&mut self, key: String) -> wasmtime::Result<Result<(), String>> {
    if let Err(e) = self.capability.check_storage() {
        return Ok(Err(e.to_string()));
    }
    let path = self.storage_dir.join(sanitize_key(&key));
    let _ = std::fs::remove_file(path); // Ignore not-found
    Ok(Ok(()))
}
```

Add at the bottom of `src/wasm/host.rs` (outside the impl block):

```rust
/// Converts a storage key to a safe filename by replacing non-alphanumeric chars.
fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}
```

**Step 2: Compile and commit**

```bash
cargo build 2>&1 | grep "^error"
git add src/wasm/host.rs
git commit -m "feat: implement WASM storage host functions (filesystem-backed)"
```

---

### Task 11: Implement timer host functions

**Files:**
- Modify: `src/wasm/host.rs` — replace timer stubs
- Modify: `src/wasm/module.rs` — add timer handle tracking

**Step 1: Add timer handle map to `HostData`**

In `src/wasm/host.rs`, add to `HostData`:

```rust
pub timer_handles: std::collections::HashMap<u32, tokio::task::JoinHandle<()>>,
pub next_handle: u32,
```

**Step 2: Replace timer stubs**

```rust
async fn set_timeout(&mut self, ms: u64) -> wasmtime::Result<u32> {
    if self.capability.check_timers().is_err() {
        return Ok(0);
    }
    let handle = self.next_handle;
    self.next_handle += 1;
    let tx = self.timer_tx.clone();
    let join = tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
        let _ = tx.send(handle).await;
    });
    self.timer_handles.insert(handle, join);
    Ok(handle)
}

async fn set_interval(&mut self, ms: u64) -> wasmtime::Result<u32> {
    if self.capability.check_timers().is_err() {
        return Ok(0);
    }
    let handle = self.next_handle;
    self.next_handle += 1;
    let tx = self.timer_tx.clone();
    let join = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(ms));
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            if tx.send(handle).await.is_err() {
                break;
            }
        }
    });
    self.timer_handles.insert(handle, join);
    Ok(handle)
}

async fn clear_timer(&mut self, handle: u32) -> wasmtime::Result<()> {
    if let Some(join) = self.timer_handles.remove(&handle) {
        join.abort();
    }
    Ok(())
}
```

**Step 3: Compile and commit**

```bash
cargo build 2>&1 | grep "^error"
git add src/wasm/host.rs
git commit -m "feat: implement WASM timer host functions with tokio tasks"
```

---

### Task 12: Implement HTTP host function

**Files:**
- Modify: `src/wasm/host.rs` — replace HTTP stub

**Step 1: Replace `http_request` stub**

```rust
async fn http_request(
    &mut self,
    req: vessel::host::types::HttpRequest,
) -> wasmtime::Result<Result<vessel::host::types::HttpResponse, String>> {
    if let Err(e) = self.capability.check_network_http() {
        return Ok(Err(e.to_string()));
    }

    let client = reqwest::Client::new();
    let method = reqwest::Method::from_bytes(req.method.as_bytes())
        .map_err(|e| wasmtime::Error::msg(e.to_string()))?;

    let mut builder = client.request(method, &req.url);
    for (key, value) in &req.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }
    if let Some(body) = req.body {
        builder = builder.body(body);
    }

    match builder.send().await {
        Ok(response) => {
            let status = response.status().as_u16() as u32;
            let headers: Vec<(String, String)> = response
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();
            let body = response.text().await.unwrap_or_default();
            Ok(Ok(vessel::host::types::HttpResponse { status, headers, body }))
        }
        Err(e) => Ok(Err(e.to_string())),
    }
}
```

**Step 2: Compile and commit**

```bash
cargo build 2>&1 | grep "^error"
git add src/wasm/host.rs
git commit -m "feat: implement WASM HTTP host function via reqwest"
```

---

### Task 13: Implement WebSocket host functions

**Files:**
- Modify: `src/wasm/host.rs` — replace WebSocket stubs
- Add WebSocket handle map to `HostData`

**Step 1: Add WebSocket state to `HostData`**

```rust
pub ws_handles: std::collections::HashMap<u32, tokio::sync::mpsc::Sender<String>>,
```

**Step 2: Replace WebSocket stubs**

```rust
async fn websocket_connect(&mut self, url: String) -> wasmtime::Result<Result<u32, String>> {
    if let Err(e) = self.capability.check_network_websocket() {
        return Ok(Err(e.to_string()));
    }

    use tokio_tungstenite::connect_async;
    use futures_util::StreamExt;

    let handle = self.next_handle;
    self.next_handle += 1;

    let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::channel::<String>(32);
    let inbound_tx = self.ws_tx.clone();
    let module_id = self.module_id.clone();

    tokio::spawn(async move {
        let ws_stream = match connect_async(&url).await {
            Ok((stream, _)) => stream,
            Err(e) => {
                eprintln!("[{}] WS connect failed: {}", module_id, e);
                return;
            }
        };
        let (mut write, mut read) = ws_stream.split();

        loop {
            tokio::select! {
                Some(msg) = outbound_rx.recv() => {
                    use tokio_tungstenite::tungstenite::Message;
                    let _ = futures_util::SinkExt::send(&mut write, Message::Text(msg.into())).await;
                }
                Some(Ok(msg)) = read.next() => {
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                        let _ = inbound_tx.send((handle, text.into_owned())).await;
                    }
                }
                else => break,
            }
        }
    });

    self.ws_handles.insert(handle, outbound_tx);
    Ok(Ok(handle))
}

async fn websocket_send(&mut self, handle: u32, message: String) -> wasmtime::Result<Result<(), String>> {
    match self.ws_handles.get(&handle) {
        Some(tx) => {
            tx.send(message).await.map_err(|e| e.to_string()).map(Ok)
                .map_err(|e| wasmtime::Error::msg(e))
        }
        None => Ok(Err(format!("unknown websocket handle {}", handle))),
    }
}

async fn websocket_close(&mut self, handle: u32) -> wasmtime::Result<Result<(), String>> {
    self.ws_handles.remove(&handle);
    Ok(Ok(()))
}
```

**Step 3: Compile and commit**

```bash
cargo build 2>&1 | grep "^error"
git add src/wasm/host.rs
git commit -m "feat: implement WASM WebSocket host functions"
```

---

### Task 14: Auto-discover WASM modules at startup

**Context:** On startup, Vessel scans `vessel-data/modules/` for directories containing `manifest.json` + `module.wasm`, loads each one, and registers it with `ModuleManager`.

**Files:**
- Modify: `src/main.rs`

**Step 1: Add a `load_wasm_modules` function to `src/main.rs`**

```rust
fn load_wasm_modules(manager: &mut ModuleManager) {
    let modules_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("vessel")
        .join("modules");

    let Ok(entries) = std::fs::read_dir(&modules_dir) else {
        return; // No modules directory yet — fine
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        if !path.join("module.wasm").exists() { continue; }

        match crate::wasm::WasmModule::load(path.clone()) {
            Ok(module) => {
                println!("Loaded WASM module: {}", module.name());
                manager.register_module(Box::new(module));
            }
            Err(e) => {
                eprintln!("Failed to load WASM module at {}: {}", path.display(), e);
            }
        }
    }
}
```

**Step 2: Call it in `main()` after native modules are registered**

Find the section in `main.rs` where native modules are registered and add:

```rust
load_wasm_modules(&mut vessel.module_manager);
```

**Step 3: Compile and commit**

```bash
cargo build 2>&1 | grep "^error"
git add src/main.rs
git commit -m "feat: auto-discover and load WASM modules at startup"
```

---

## Phase 5 — Home Assistant WASM Module

### Task 15: Create the Home Assistant guest module project

**Context:** This is a separate Rust crate that compiles to `wasm32-wasip2`. It uses `wit-bindgen` to generate guest-side bindings from the same WIT file.

**Files:**
- Create: `modules/home-assistant/Cargo.toml`
- Create: `modules/home-assistant/src/lib.rs`
- Create: `modules/home-assistant/wit/` (symlink or copy of `wit/vessel-host.wit`)

**Step 1: Install the wasm32-wasip2 target**

```bash
rustup target add wasm32-wasip2
```

**Step 2: Create `modules/home-assistant/Cargo.toml`**

```toml
[package]
name = "home-assistant"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.30"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
opt-level = "s"   # Optimise for size — WASM binaries should be small
```

> Check [crates.io/crates/wit-bindgen](https://crates.io/crates/wit-bindgen) for the version compatible with your installed wasmtime.

**Step 3: Copy the WIT file**

```bash
mkdir -p modules/home-assistant/wit
cp wit/vessel-host.wit modules/home-assistant/wit/
```

**Step 4: Create `modules/home-assistant/src/lib.rs` skeleton**

```rust
wit_bindgen::generate!({
    world: "vessel-module",
    path: "wit/vessel-host.wit",
});

// Re-export the generated host interface types
use exports::vessel::host::guest::Guest;
use vessel::host::host::{
    subscribe, emit, log, storage_get, storage_set,
    websocket_connect, websocket_send, set_interval, set_timeout,
};
use vessel::host::types::Event;

struct HomeAssistant {
    ws_handle: std::cell::Cell<u32>,
}

impl Guest for HomeAssistant {
    fn on_load() -> Result<(), String> {
        log("info", "Home Assistant module loading");
        // Connection is initiated here; ws_handle stored in global state
        // (WASM is single-threaded, so a static Cell is safe)
        Ok(())
    }

    fn on_unload() -> Result<(), String> {
        log("info", "Home Assistant module unloading");
        Ok(())
    }

    fn on_event(_event: Event) -> Result<(), String> {
        Ok(())
    }

    fn on_command(_action: String, _params: String) -> Result<String, String> {
        Ok("{}".to_string())
    }

    fn on_timer(_handle: u32) -> Result<(), String> {
        Ok(())
    }

    fn on_websocket_message(_handle: u32, _message: String) -> Result<(), String> {
        Ok(())
    }
}

export!(HomeAssistant);
```

**Step 5: Compile the WASM module**

```bash
cd modules/home-assistant
cargo build --target wasm32-wasip2 --release 2>&1 | tail -20
```

Expected: `target/wasm32-wasip2/release/home_assistant.wasm` produced.

**Step 6: Install the module for testing**

```bash
# Create the module directory
mkdir -p "$LOCALAPPDATA/vessel/modules/home-assistant"

# Copy binary
cp target/wasm32-wasip2/release/home_assistant.wasm \
   "$LOCALAPPDATA/vessel/modules/home-assistant/module.wasm"
```

Create `$LOCALAPPDATA/vessel/modules/home-assistant/manifest.json`:

```json
{
  "id": "home-assistant",
  "name": "Home Assistant",
  "version": "0.1.0",
  "api_version": 1,
  "description": "Home Assistant integration",
  "permissions": {
    "subscribe": ["system.window.*"],
    "call": [],
    "network": { "http": false, "websocket": true, "tcp": false },
    "storage": true,
    "timers": true
  }
}
```

**Step 7: Commit**

```bash
cd ../..
git add modules/
git commit -m "feat: add home-assistant WASM module skeleton"
```

---

### Task 16: Implement Home Assistant WebSocket connection and event re-emission

**Context:** Home Assistant exposes a WebSocket API at `ws://<host>:8123/api/websocket`. After connecting, you authenticate with a long-lived access token, subscribe to state changes, and re-emit them as Vessel events.

**Files:**
- Modify: `modules/home-assistant/src/lib.rs`

**Step 1: Write the full implementation**

```rust
wit_bindgen::generate!({
    world: "vessel-module",
    path: "wit/vessel-host.wit",
});

use exports::vessel::host::guest::Guest;
use vessel::host::host::*;
use vessel::host::types::Event;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ── HA WebSocket message types ──────────────────────────────────────────────

#[derive(Deserialize)]
struct HaMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    id: u32,
    #[serde(default)]
    event: Option<HaEvent>,
    #[serde(default)]
    ha_version: Option<String>,
}

#[derive(Deserialize)]
struct HaEvent {
    event_type: String,
    data: Value,
}

// ── Module state (WASM is single-threaded; statics are safe) ───────────────

static mut WS_HANDLE: u32 = 0;
static mut MSG_ID: u32 = 1;

struct HomeAssistant;

impl Guest for HomeAssistant {
    fn on_load() -> Result<(), String> {
        log("info", "Home Assistant: loading");

        let ha_url = storage_get("url")
            .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".into());

        // Subscribe to system window events (optional: could pause polling when idle)
        let _ = subscribe("system.window.*");

        // Connect — authentication happens in on_websocket_message when we receive auth_required
        let handle = websocket_connect(&ha_url)?;
        unsafe { WS_HANDLE = handle; }

        Ok(())
    }

    fn on_unload() -> Result<(), String> {
        log("info", "Home Assistant: unloading");
        Ok(())
    }

    fn on_event(_event: Event) -> Result<(), String> {
        // Could react to system.window events here (e.g. pause/resume polling)
        Ok(())
    }

    fn on_command(action: String, params: String) -> Result<String, String> {
        let params: Value = serde_json::from_str(&params).unwrap_or(Value::Null);

        match action.as_str() {
            "call_service" => {
                // params: { domain, service, entity_id, ... }
                let id = unsafe { MSG_ID += 1; MSG_ID };
                let msg = json!({
                    "id": id,
                    "type": "call_service",
                    "domain": params.get("domain").and_then(Value::as_str).unwrap_or(""),
                    "service": params.get("service").and_then(Value::as_str).unwrap_or(""),
                    "service_data": params.get("service_data").cloned().unwrap_or(json!({})),
                });
                let handle = unsafe { WS_HANDLE };
                websocket_send(handle, &msg.to_string())?;
                Ok("{\"queued\": true}".into())
            }
            _ => Err(format!("unknown action: {}", action)),
        }
    }

    fn on_timer(handle: u32) -> Result<(), String> {
        // Reconnect timer
        log("info", "Home Assistant: attempting reconnect");
        let ha_url = storage_get("url")
            .unwrap_or_else(|| "ws://homeassistant.local:8123/api/websocket".into());
        if let Ok(new_handle) = websocket_connect(&ha_url) {
            unsafe { WS_HANDLE = new_handle; }
        }
        Ok(())
    }

    fn on_websocket_message(handle: u32, message: String) -> Result<(), String> {
        let Ok(msg) = serde_json::from_str::<HaMessage>(&message) else {
            return Ok(());
        };

        match msg.msg_type.as_str() {
            "auth_required" => {
                let token = storage_get("token").unwrap_or_default();
                let auth = json!({ "type": "auth", "access_token": token });
                websocket_send(handle, &auth.to_string())?;
            }

            "auth_ok" => {
                log("info", "Home Assistant: authenticated");
                // Subscribe to all state_changed events
                let id = unsafe { MSG_ID += 1; MSG_ID };
                let sub = json!({
                    "id": id,
                    "type": "subscribe_events",
                    "event_type": "state_changed"
                });
                websocket_send(handle, &sub.to_string())?;
            }

            "auth_invalid" => {
                log("error", "Home Assistant: authentication failed — set token via storage_set");
            }

            "event" => {
                if let Some(event) = msg.event {
                    if event.event_type == "state_changed" {
                        // Re-emit as a Vessel event so dashboards can react
                        let vessel_event = Event {
                            module: "home-assistant".into(),
                            name: "state_changed".into(),
                            version: 1,
                            data: event.data.to_string(),
                            timestamp: 0, // host fills this in
                        };
                        let _ = emit(vessel_event);
                    }
                }
            }

            "result" => {
                // Command acknowledgement — ignore for now
            }

            _ => {}
        }

        Ok(())
    }
}

export!(HomeAssistant);
```

**Step 2: Rebuild and reinstall**

```bash
cd modules/home-assistant
cargo build --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/home_assistant.wasm \
   "$LOCALAPPDATA/vessel/modules/home-assistant/module.wasm"
```

**Step 3: End-to-end test**
- `cargo run` (Vessel)
- Check console output: `Loaded WASM module: home-assistant` and `Home Assistant: loading`
- If you have a Home Assistant instance: set the token via storage file, verify `state_changed` events appear in the Vessel WS stream

**Step 4: Commit**

```bash
cd ../..
git add modules/home-assistant/
git commit -m "feat: implement Home Assistant WASM module with WebSocket + event re-emission"
```

---

## Phase 6 — `/api/modules` REST Endpoint

### Task 17: Create the modules REST API

**Context:** Provides install, list, and enable/disable operations. Install writes the hash file after copy. List reads all manifests from `vessel-data/modules/`.

**Files:**
- Create: `src/api/modules.rs`
- Modify: `src/api.rs` to mount the router

**Step 1: Create `src/api/modules.rs`**

```rust
use crate::wasm::manifest::{load_manifest, write_hash, HOST_API_VERSION};
use axum::{Json, extract::Path};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn modules_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vessel")
        .join("modules")
}

#[derive(Serialize)]
pub struct ModuleInfo {
    id: String,
    name: String,
    version: String,
    api_version: u32,
    description: String,
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
pub struct ApiVersion {
    pub host_api_version: u32,
}

pub async fn api_version() -> Json<ApiVersion> {
    Json(ApiVersion { host_api_version: HOST_API_VERSION })
}
```

**Step 2: Mount in `src/api.rs`**

```rust
pub mod modules;

pub fn router() -> Router<Arc<crate::vessel::AppState>> {
    Router::new()
        .nest("/dashboards", dashboards::router())
        .route("/modules", get(modules::list_modules))
        .route("/modules/version", get(modules::api_version))
}
```

**Step 3: Compile, test with curl, commit**

```bash
cargo build 2>&1 | grep "^error"
# With server running:
# curl http://127.0.0.1:8080/api/modules
git add src/api/modules.rs src/api.rs
git commit -m "feat: add /api/modules REST endpoint (list, api_version)"
```

---

## Summary

| Phase | Tasks | What you learn |
|---|---|---|
| 1 | 1–3 | Serde tagged enums, protocol design, TypeScript type safety |
| 2 | 4–6 | WIT interface language, wasmtime::component::bindgen!, manifest parsing, sha2 |
| 3 | 7 | Capability enforcement, glob pattern matching |
| 4 | 8–14 | Wasmtime Store/Linker/Engine, async host functions, tokio task lifecycle, reqwest, tokio-tungstenite |
| 5 | 15–16 | wit-bindgen guest side, wasm32-wasip2 target, cross-compilation, real WASM integration |
| 6 | 17 | Axum router composition |

The architecture is intentionally layered so each phase produces a working, testable checkpoint. Phase 2 produces a WASM host that loads (but does nothing). Phase 3 adds safety. Phase 4 adds power. Phase 5 proves it works with a real integration.
