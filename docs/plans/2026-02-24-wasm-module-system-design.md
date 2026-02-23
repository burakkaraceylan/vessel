# WASM Module System Design

**Date:** 2026-02-24
**Status:** Approved

## Goal

Turn Vessel into a capability-based control kernel with a WASM-powered extension ecosystem.

- Native drivers handle privileged OS/system access (Discord IPC, Windows APIs, SMTC)
- WASM modules handle integrations, automation logic, and future marketplace extensions
- Zero recompilation required to install a module

---

## 1. Overall Architecture

The existing `Module` trait, `ModuleManager`, and `EventPublisher` stay **completely unchanged**. WASM support is added as a new `WasmModule` struct that implements `Module` — `ModuleManager` treats it identically to Discord, Media, or System.

```
Touch UI  <──WS──>  Vessel (axum)  <──channels──>  ModuleManager
                         │                               │
                    REST API                 ┌──────┬───┴────┬──────────┐
                  /api/dashboards         Discord  Media  System   WasmModule
                  /api/modules            (native) (native)(native)  │
                                                                  WasmHost
                                                              (Wasmtime engine)
                                                                      │
                                                          CapabilityValidator
                                                                      │
                                                          Host functions (WIT)
```

`WasmModule::run()` bridges the existing async mpsc/broadcast world into WASM's synchronous call model:
- Receives a `ModuleCommand` on the mpsc channel → calls `on-command()` on the WASM component
- Receives a matching `ModuleEvent` on the broadcast channel → calls `on-event()` on the WASM component
- Selects on `cancel_token` → calls `on-unload()` and exits

A new REST endpoint (`/api/modules`) handles install, list, enable/disable. Module `.wasm` binaries and `manifest.json` live in `vessel-data/modules/<id>/`.

---

## 2. Protocol Stabilization

The current wire format has no `type` discriminator, no versioning, and no request/response correlation. The new format adds all three.

### Client → Vessel

```json
// Fire-and-forget action
{ "type": "call", "request_id": "uuid", "module": "discord", "name": "voice.set_mute", "version": 1, "params": {} }

// Subscribe to events
{ "type": "subscribe", "module": "discord", "name": "voice.*" }
```

### Vessel → Client

```json
// Event broadcast
{ "type": "event", "module": "discord", "name": "voice.settings_updated", "version": 1, "data": {}, "timestamp": 1700000000 }

// Response to a call
{ "type": "response", "request_id": "uuid", "success": true, "data": {} }
```

### Rules

- Event names are dot-separated and hierarchical (`voice.settings_updated`, `media.track_changed`)
- `version` is required on all calls and events — bumped only on breaking payload shape changes
- `request_id` enables the UI to correlate responses to calls (loading states, error handling)
- The existing flat format is removed — both sides migrate together since there are no external consumers

This replaces `protocol.rs`'s `IncomingMessage` and `OutgoingMessage` and requires updating the React WebSocket client in parallel.

---

## 3. WIT Interface Design

The Component Model contract lives in a single `vessel-host.wit` file. This is the **stable ABI** — changing it increments `api_version`.

```wit
package vessel:host@1.0.0;

interface types {
    record event {
        module: string,
        name: string,
        version: u32,
        data: string,       // JSON-encoded payload
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

// Host exports — what a WASM module can call
interface host {
    use types.{event, http-request, http-response};

    // Event bus
    subscribe: func(pattern: string) -> result<_, string>;
    emit: func(event: event) -> result<_, string>;

    // Driver calls (routes into ModuleManager)
    call: func(module: string, name: string, version: u32, params: string) -> result<string, string>;

    // Network
    http-request: func(req: http-request) -> result<http-response, string>;
    websocket-connect: func(url: string) -> result<u32, string>;  // returns handle
    websocket-send: func(handle: u32, message: string) -> result<_, string>;
    websocket-close: func(handle: u32) -> result<_, string>;

    // Storage (namespaced per module automatically by the host)
    storage-get: func(key: string) -> option<string>;
    storage-set: func(key: string, value: string) -> result<_, string>;
    storage-delete: func(key: string) -> result<_, string>;

    // Timers
    set-timeout: func(ms: u64) -> u32;
    set-interval: func(ms: u64) -> u32;
    clear-timer: func(handle: u32);

    // Logging
    log: func(level: string, message: string);
}

// Guest exports — what the host calls on the WASM module
interface guest {
    use types.{event};

    on-load: func() -> result<_, string>;
    on-unload: func() -> result<_, string>;
    on-event: func(event: event) -> result<_, string>;
    on-command: func(action: string, params: string) -> result<string, string>;
    on-timer: func(handle: u32) -> result<_, string>;
    on-websocket-message: func(handle: u32, message: string) -> result<_, string>;
}

world vessel-module {
    import host;
    export guest;
}
```

### Design rationale

- `data`/`params` are JSON strings rather than typed WIT records — keeps the ABI stable as individual module payloads evolve without touching the WIT file
- `websocket-connect` returns an opaque `u32` handle — the host owns the connection, the guest holds only an ID
- Storage is automatically namespaced by module ID on the host side — WASM code never specifies a namespace, preventing cross-module data access

---

## 4. Module Manifest

Each module ships a `manifest.json` alongside its `.wasm` binary.

```json
{
  "id": "home-assistant",
  "name": "Home Assistant",
  "version": "1.0.0",
  "api_version": 1,
  "description": "Home Assistant REST + WebSocket integration",
  "author": "someone",
  "permissions": {
    "subscribe": ["system.window.*"],
    "call": [],
    "network": {
      "http": true,
      "websocket": true,
      "tcp": false
    },
    "storage": true,
    "timers": true
  }
}
```

### Module directory layout

```
vessel-data/modules/
  home-assistant/
    manifest.json
    module.wasm
    manifest.hash     ← written by Vessel on install, never by the module author
```

`manifest.hash` is a SHA-256 of `manifest.json + module.wasm` concatenated, written by Vessel at install time. Used for tamper detection on every subsequent load.

### Install-time validation

1. Parse manifest — reject malformed JSON
2. Check `api_version` ≤ current host API version — reject incompatible
3. Display declared permissions to user, require confirmation before activating
4. Write `manifest.hash`

### Load-time validation (every startup)

1. Re-read and re-hash manifest + binary — reject if changed since install
2. Re-validate `api_version`

---

## 5. Capability Enforcement

Every host function call goes through a `CapabilityValidator` constructed from the manifest at load time. It holds no mutable state — just the parsed permission set.

```rust
struct CapabilityValidator {
    subscribe_patterns: Vec<glob::Pattern>,  // compiled from manifest.permissions.subscribe
    allowed_calls: HashSet<String>,          // "module.name@version"
    network_http: bool,
    network_websocket: bool,
    network_tcp: bool,
    storage: bool,
    timers: bool,
}

impl CapabilityValidator {
    fn check_subscribe(&self, pattern: &str) -> Result<(), CapabilityError>;
    fn check_call(&self, module: &str, name: &str, version: u32) -> Result<(), CapabilityError>;
    fn check_network_http(&self) -> Result<(), CapabilityError>;
    fn check_network_websocket(&self) -> Result<(), CapabilityError>;
    fn check_storage(&self) -> Result<(), CapabilityError>;
    fn check_timers(&self) -> Result<(), CapabilityError>;
}
```

Each host function implementation receives an `Arc<CapabilityValidator>`. Before doing any work, it calls the appropriate `check_*` method. On failure it returns `Err("capability denied: storage not declared in manifest")` to the guest as a `result::err` variant.

**No escalation path at runtime** — there is no "request more permissions" API. If a module needs a capability it didn't declare, it fails. More permissions require updating the manifest, which requires user confirmation at install time again.

---

## 6. Module Lifecycle

### Load sequence

```
1. Read + validate manifest.json
2. Verify manifest.hash (tamper check)
3. Check api_version compatibility
4. Build CapabilityValidator from permissions
5. Load .wasm bytes → wasmtime::Component::from_binary()
6. Build wasmtime::Linker, add host functions (each closes over Arc<CapabilityValidator>)
7. Instantiate component → call on_load()
8. If on_load() returns Err → log error, mark module failed, do not run
9. Register with ModuleManager as a normal Module
```

### Runtime (WasmModule::run() main loop)

```
loop {
    select! {
        cmd = rx.recv()          => call on_command(action, params_json)
        event = event_rx.recv()  => if matches subscription → call on_event(event)
        handle = timer_fires     => call on_timer(handle)
        msg = ws_message         => call on_websocket_message(handle, msg)
        _ = cancel_token         => break
    }
}
```

Timer and WebSocket handles are tracked in a `HashMap<u32, ...>` owned by `WasmModule`. Timers are `tokio::time::interval` tasks that send into a channel selected by the main loop.

### Unload sequence

```
1. Cancel token fires → exit select loop
2. Call on_unload() on the component
3. Cancel all interval tasks
4. Close all open WebSocket connections
5. Drop wasmtime Instance + Store
```

### Crash isolation

If the WASM component traps, Wasmtime catches it at the host boundary and returns an error. `WasmModule::run()` logs the trap, emits a `Transient` event `wasm.module_crashed { id, reason }`, and exits. Other modules are unaffected. A future restart policy can re-instantiate crashed modules with backoff.

---

## 7. Versioning Strategy

Two completely independent version layers.

### Host API version (`api_version` integer)

Tracks breaking changes to the WIT interface. Stored in a single constant:

```rust
pub const HOST_API_VERSION: u32 = 1;
```

Incremented only when a host function is removed, renamed, or its signature changes. Adding new functions is non-breaking. Vessel rejects any module where `manifest.api_version > HOST_API_VERSION`. The WIT package version (`vessel:host@1.0.0`) mirrors this integer.

### Module semantic version (`version` string)

Standard semver. Used for marketplace update checks and user display only. Vessel makes no runtime decisions based on it.

### What triggers an api_version bump

| Change | Breaking? | Bumps api_version? |
|---|---|---|
| Add new host function | No | No |
| Remove host function | Yes | Yes |
| Change function signature | Yes | Yes |
| Add new field to WIT record | No (additive) | No |
| Remove field from WIT record | Yes | Yes |
| Change event name convention | Yes | Yes |

---

## 8. Architecture Validation: Home Assistant Module

Home Assistant exercises every capability category, confirming the design is complete.

| Requirement | Host capability |
|---|---|
| Connect to HA WebSocket API on startup | `websocket-connect` |
| Authenticate via long-lived access token | `storage-get` |
| Subscribe to HA state change events | `on-websocket-message` |
| Re-emit relevant state as Vessel events | `emit` |
| Send commands to HA (turn on light, etc.) | `on-command` → `websocket-send` |
| Reconnect on disconnect | `set-timeout` + `on-timer` |
| React to active window changes | `subscribe("system.window.*")` |

**Manifest:**

```json
{
  "id": "home-assistant",
  "api_version": 1,
  "permissions": {
    "subscribe": ["system.window.*"],
    "call": [],
    "network": { "http": false, "websocket": true, "tcp": false },
    "storage": true,
    "timers": true
  }
}
```

---

## Implementation Phases

| Phase | Scope |
|---|---|
| 1 | Protocol stabilization — new wire format in `protocol.rs` + React WS client |
| 2 | WIT file + Wasmtime host skeleton — `WasmModule` implements `Module`, no capability enforcement yet |
| 3 | Capability enforcement — `CapabilityValidator`, manifest loading, tamper detection |
| 4 | Full host capability surface — network, storage, timers, logging |
| 5 | Home Assistant WASM module (Rust, compiled to WASM) |
| 6 | `/api/modules` REST endpoint — install, list, enable/disable |
| 7 | Marketplace foundations — registry protocol, signed manifests |
