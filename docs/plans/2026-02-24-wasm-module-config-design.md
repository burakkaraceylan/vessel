# WASM Module Config Design

**Date:** 2026-02-24
**Status:** Approved

## Problem

WASM modules have no way to receive admin-provided configuration (credentials, URLs, etc.). The home-assistant module worked around this by reading from `storage_get`, which conflates mutable runtime state with static admin config — two different things that should be kept separate.

## Design

### Config file shape

WASM modules get a `[modules.<id>]` section in `config.toml`, mirroring native modules:

```toml
[modules.home-assistant]
url = "ws://homeassistant.local:8123/api/websocket"
token = "your-long-lived-access-token"
```

All values are strings. Non-string toml values (numbers, bools) are coerced to string via `to_string()`. The section is optional — absent keys return `None`.

### WIT interface

A single new function added to the `host` interface in `vessel-host.wit` (and the copy in `modules/home-assistant/wit/`):

```wit
config-get: func(key: string) -> option<string>;
```

- Read-only — no `config-set`
- No new permission required — config only exposes what the admin explicitly wrote
- Same return type as `storage-get` (`option<string>`) for consistency

### Data flow

1. `load_wasm_modules` in `main.rs` receives `&config::Config`
2. After loading the manifest, it looks up `config.modules.get(&manifest.id)`
3. The matching toml table (or empty table if absent) is passed into `WasmModule::load()`
4. `WasmModule` flattens the toml table into `HashMap<String, String>` at load time
5. `HostData` gains a `config: HashMap<String, String>` field
6. The `config_get` host function does a plain map lookup into that field

### Separation of concerns

| Function | Backed by | Writable by module? | Use for |
|---|---|---|---|
| `config_get` | `config.toml` | No | Admin-set credentials, URLs |
| `storage_get/set` | `%APPDATA%/.../storage/` | Yes | Runtime state, caches |

### HA module update

`storage_get("url")` and `storage_get("token")` become `config_get("url")` and `config_get("token")`. The fallback default URL is retained for the case where config is absent.

## Files Changed

- `config.toml` (example/docs) — add `[modules.home-assistant]` example
- `modules/home-assistant/wit/vessel-host.wit` — add `config-get`
- `src/wasm/host.rs` — implement `config_get` host function
- `src/wasm/module.rs` — pass config into `HostData`; update `WasmModule::load()` signature
- `src/main.rs` — pass `&config` into `load_wasm_modules`; extract module config by id
- `modules/home-assistant/src/lib.rs` — switch to `config_get`
