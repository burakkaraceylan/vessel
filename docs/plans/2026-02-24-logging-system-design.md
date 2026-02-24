# Logging System Design

**Date:** 2026-02-24
**Status:** Approved

## Goal

Replace scattered `println!`/`eprintln!` calls with a structured, level-filtered logging system using the `tracing` ecosystem. Every module load, every WebSocket message exchanged between clients and modules, and every error must be observable at the appropriate log level.

## Decisions

- **Log level control:** `RUST_LOG` environment variable (standard Rust convention). Default: `info`.
- **Output:** Terminal only (stderr via `tracing_subscriber::fmt`).
- **Message detail:** `DEBUG` for summaries (`→ discord::set_mute`), `TRACE` for full raw JSON payloads.
- **Approach:** `tracing` with spans — connection-scoped and module-scoped context for async tracing.

## Dependencies

```toml
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

`tracing` is already present in `Cargo.toml`.

## Subscriber Initialization

In `main()`, before anything else:

```rust
tracing_subscriber::fmt()
    .with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    )
    .init();
```

Usage: `RUST_LOG=vessel=trace cargo run`

## Log Level Mapping

| Level   | Sites                                      | What                                                                 |
|---------|--------------------------------------------|----------------------------------------------------------------------|
| `ERROR` | `main.rs`, `module_manager.rs`, `vessel.rs` | Module init failures, routing errors, WebSocket read errors         |
| `WARN`  | `module_manager.rs`, `main.rs`              | Module not found, missing config (e.g. discord), WASM load failures |
| `INFO`  | `main.rs`, `vessel.rs`                      | Server started (host:port), each module registered, WS connected/disconnected |
| `DEBUG` | `vessel.rs`                                 | Each inbound call summary (`→ discord::set_mute`), each outbound event summary (`← discord::voice_settings_update`) |
| `TRACE` | `vessel.rs`                                 | Full raw JSON of each inbound/outbound message                      |

## Spans

**`ws_connection` span** — enters in `handle_websocket()`, wraps the entire client lifetime.
- Field: `peer` (client socket address, threaded through from `ws_handler` via `ConnectInfo`)
- All log lines during this connection carry `ws_connection{peer=...}` context

**`module` span** — enters in `module_manager::run_all()`, wraps each module's `run()` task.
- Field: `name` (the module's static name string)
- All log lines inside a module's run loop carry `module{name=...}` context

Example output at DEBUG:
```
INFO vessel::main: server listening on 127.0.0.1:8080
INFO vessel::main: module registered name=discord
INFO vessel::vessel ws_connection{peer=127.0.0.1:52341}: client connected
DEBUG vessel::vessel ws_connection{peer=127.0.0.1:52341}: → discord::set_mute
TRACE vessel::vessel ws_connection{peer=127.0.0.1:52341}: → raw json={"module":"discord","action":"set_mute","params":{"mute":true}}
DEBUG vessel::vessel ws_connection{peer=127.0.0.1:52341}: ← discord::voice_settings_update
```

## WASM `log()` Host Function

The existing `log(level: String, message: String)` host function currently uses `println!`. It will be updated to dispatch through tracing macros:

```rust
match level.as_str() {
    "error" => tracing::error!(target: "wasm", module = %self.module_id, "{}", message),
    "warn"  => tracing::warn!(target: "wasm", module = %self.module_id, "{}", message),
    "info"  => tracing::info!(target: "wasm", module = %self.module_id, "{}", message),
    "debug" => tracing::debug!(target: "wasm", module = %self.module_id, "{}", message),
    _       => tracing::trace!(target: "wasm", module = %self.module_id, "{}", message),
}
```

## Files Changed

| File                     | Change                                                                 |
|--------------------------|------------------------------------------------------------------------|
| `Cargo.toml`             | Add `tracing-subscriber`                                               |
| `src/main.rs`            | Init subscriber; `info!`/`warn!`/`error!` for module loading          |
| `src/vessel.rs`          | Thread peer addr; add `ws_connection` span; `info!`/`debug!`/`trace!`/`error!` |
| `src/module_manager.rs`  | Add `module` span in `run_all()`; `warn!`/`error!` replacements        |
| `src/wasm/host.rs`       | Route `log()` through tracing; replace WS error `eprintln!` with `error!` |
