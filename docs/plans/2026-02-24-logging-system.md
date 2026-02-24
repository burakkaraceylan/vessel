# Logging System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:executing-plans to implement this plan task-by-task.

**Goal:** Replace all `println!`/`eprintln!` calls with structured `tracing` macros, add a subscriber, and introduce spans that scope logs to WebSocket connections and module run loops.

**Architecture:** `tracing-subscriber` is initialized in `main()` filtered by `RUST_LOG` (default `info`). Two spans provide async context: `ws_connection{peer=...}` wraps each WebSocket client lifetime in `vessel.rs`; `module{name=...}` wraps each module's `run()` task in `module_manager.rs`. WASM modules' `log()` host function routes through the same tracing macros.

**Tech Stack:** `tracing` (already in Cargo.toml), `tracing-subscriber` (new), `axum::extract::ConnectInfo` for peer address.

---

### Task 1: Add `tracing-subscriber` dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add the dependency**

In `Cargo.toml`, after the `tracing = "0.1.37"` line, add:

```toml
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

**Step 2: Verify it compiles**

```bash
cargo build 2>&1 | head -20
```

Expected: resolves and compiles without errors. `tracing-subscriber` and `tracing` are compatible — no conflicts.

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add tracing-subscriber dependency"
```

---

### Task 2: Initialize the subscriber in `main.rs` and update module-loading logs

**Files:**
- Modify: `src/main.rs`

**Context:** `main.rs` currently uses `println!`/`eprintln!` for module loading. We initialize the subscriber first so every subsequent log goes through it.

**Step 1: Add imports at the top of `main.rs`**

Add to the existing `use` block at the top:

```rust
use tracing::{error, info, warn};
```

**Step 2: Initialize subscriber as the first thing in `main()`**

In `main()`, add before `let token = CancellationToken::new();`:

```rust
tracing_subscriber::fmt()
    .with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
    )
    .init();
```

**Step 3: Replace prints in `load_wasm_modules()`**

Replace:
```rust
println!("[vessel] loaded WASM module: {}", module.name());
```
With:
```rust
info!(name = module.name(), "WASM module loaded");
```

Replace:
```rust
eprintln!("[vessel] failed to load WASM module at {}: {}", path.display(), e);
```
With:
```rust
error!(path = %path.display(), "failed to load WASM module: {e:#}");
```

**Step 4: Replace prints in `main()` — module init**

Replace:
```rust
Err(e) => { eprintln!("[vessel] discord module failed to initialize: {e:#}"); }
```
With:
```rust
Err(e) => { error!("discord module failed to initialize: {e:#}"); }
```

Replace:
```rust
None => eprintln!("[vessel] discord module config missing, skipping"),
```
With:
```rust
None => warn!("discord module config missing, skipping"),
```

Replace:
```rust
Err(e) => { eprintln!("[vessel] media module failed to initialize: {e:#}"); }
```
With:
```rust
Err(e) => { error!("media module failed to initialize: {e:#}"); }
```

Replace:
```rust
Err(e) => { eprintln!("[vessel] system module failed to initialize: {e:#}"); }
```
With:
```rust
Err(e) => { error!("system module failed to initialize: {e:#}"); }
```

**Step 5: Replace the Ctrl+C print and add a server start log**

Replace:
```rust
println!("Ctrl+C received, shutting down...");
```
With:
```rust
info!("Ctrl+C received, shutting down");
```

After `let listener = tokio::net::TcpListener::bind(...).await?;`, add:
```rust
info!(host = %config.host, port = config.port, "server listening");
```

**Step 6: Update the `axum::serve` call to enable `ConnectInfo` (needed for Task 3)**

Change:
```rust
axum::serve(listener, build_router(state))
```
To:
```rust
use std::net::SocketAddr;
axum::serve(listener, build_router(state).into_make_service_with_connect_info::<SocketAddr>())
```

**Step 7: Verify**

```bash
cargo build 2>&1
```

Expected: clean build. Then run `cargo run` — you should see structured `INFO` lines like:
```
INFO vessel::main: server listening host=127.0.0.1 port=8080
```

**Step 8: Commit**

```bash
git add src/main.rs
git commit -m "feat(logging): initialize tracing subscriber and update main.rs logs"
```

---

### Task 3: Add `ws_connection` span and message-level logs to `vessel.rs`

**Files:**
- Modify: `src/vessel.rs`

**Context:** This is the busiest log site. We add a `ws_connection` span that scopes all log lines to a specific client. Inbound messages are logged at `DEBUG` (summary) and `TRACE` (full JSON). Outbound events are logged at `DEBUG` (summary) and `TRACE` (full JSON).

**Step 1: Add imports at the top of `vessel.rs`**

Add to the existing `use` block:

```rust
use std::net::SocketAddr;
use tracing::{debug, error, info, trace, Instrument};
use axum::extract::ConnectInfo;
```

**Step 2: Update `ws_handler` to extract peer address and attach span**

Replace the entire `ws_handler` function:

```rust
async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| {
        let span = tracing::info_span!("ws_connection", peer = %peer);
        async move {
            if let Err(e) = handle_websocket(socket, state).await {
                error!("WebSocket handler error: {e}");
            }
        }
        .instrument(span)
    })
}
```

**Step 3: Update `handle_websocket` signature — take `Arc<AppState>` by value**

Change:
```rust
async fn handle_websocket(mut socket: WebSocket, state: &Arc<AppState>) -> anyhow::Result<()> {
```
To:
```rust
async fn handle_websocket(mut socket: WebSocket, state: Arc<AppState>) -> anyhow::Result<()> {
```

All internal uses of `state.` still work — `Arc<T>` derefs to `T`.

**Step 4: Replace the "connection established" print**

Replace:
```rust
println!("WebSocket connection established");
```
With:
```rust
info!("client connected");
```

**Step 5: Add a "client disconnected" log**

The connection loop breaks in two places: `Some(Ok(Message::Close(_))) | None => break` and the cancellation branch. After the loop ends (just before `Ok(())`), add:

```rust
info!("client disconnected");
```

**Step 6: Replace inbound message logs**

Replace:
```rust
println!("Call: module='{}', name='{}'", module, name);
```
With:
```rust
debug!(module = %module, action = %name, "→ call");
trace!(raw = %line, "→ raw");
```

Note: `line` is the raw JSON string already available in the loop at this point.

Replace:
```rust
println!("Subscribe: module='{}', name='{}'", module, name);
```
With:
```rust
debug!(module = %module, event = %name, "→ subscribe");
```

Replace:
```rust
eprintln!("Route error: {}", e);
```
With:
```rust
error!("route error: {e}");
```

Replace:
```rust
eprintln!("Invalid message: {}", e);
```
With:
```rust
error!("invalid message: {e} raw={line}");
```

Replace:
```rust
eprintln!("WebSocket read error: {}", e);
```
With:
```rust
error!("WebSocket read error: {e}");
```

**Step 7: Add outbound event logs**

In the `event = event_rx.recv()` branch, before `socket.send(...)`:

```rust
Ok(event) => {
    debug!(module = event.source(), event = event.event_name(), "← event");
    let msg = OutgoingMessage::from(event);
    let json = serde_json::to_string(&msg)?;
    trace!(raw = %json, "← raw");
    socket.send(Message::Text(json.into())).await?;
}
```

**Step 8: Verify**

```bash
cargo build 2>&1
```

Then run with `RUST_LOG=vessel=debug cargo run` and connect a WebSocket client. Expected output:
```
INFO vessel::vessel ws_connection{peer=127.0.0.1:PORT}: client connected
DEBUG vessel::vessel ws_connection{peer=127.0.0.1:PORT}: → call module=discord action=set_mute
DEBUG vessel::vessel ws_connection{peer=127.0.0.1:PORT}: ← event module=discord event=voice_settings_update
```

**Step 9: Commit**

```bash
git add src/vessel.rs
git commit -m "feat(logging): add ws_connection span and message-level tracing to vessel.rs"
```

---

### Task 4: Add `module` span and update logs in `module_manager.rs`

**Files:**
- Modify: `src/module_manager.rs`

**Context:** Each module runs as a spawned tokio task. Wrapping it in an `info_span!("module", name = ...)` means all log output from inside a module's `run()` loop carries the module name as context — even logs emitted deep inside `discord.rs` or `media.rs`.

**Step 1: Add imports**

Add to the `use` block:

```rust
use tracing::{error, info, info_span, warn, Instrument};
```

**Step 2: Add a log in `register_module()`**

After `self.modules.insert(name, (module, rx));`:

```rust
info!(name, "module registered");
```

**Step 3: Replace the "module not found" warning in `send_command()`**

Replace:
```rust
eprintln!("Module '{}' not found", command.target);
```
With:
```rust
warn!(name = %command.target, "module not found");
```

**Step 4: Add `module` span in `run_all()` and replace the error print**

Replace:
```rust
tokio::spawn(async move {
    if let Err(e) = module.run(ctx).await {
        eprintln!("Module error: {}", e);
    }
});
```
With:
```rust
tokio::spawn(
    async move {
        if let Err(e) = module.run(ctx).await {
            error!("module error: {e:#}");
        }
    }
    .instrument(info_span!("module", name)),
);
```

Note: `name` is `&'static str` captured from `module.name()` earlier in the loop. Tracing accepts it directly.

**Step 5: Verify**

```bash
cargo build 2>&1
```

Run with `RUST_LOG=vessel=info cargo run`. Expected:
```
INFO vessel::module_manager module{name=discord}: module registered
INFO vessel::module_manager module{name=media}: module registered
INFO vessel::module_manager module{name=system}: module registered
```

**Step 6: Commit**

```bash
git add src/module_manager.rs
git commit -m "feat(logging): add module span and structured logs to module_manager.rs"
```

---

### Task 5: Route WASM `log()` through tracing in `wasm/host.rs`

**Files:**
- Modify: `src/wasm/host.rs`

**Context:** WASM modules call `log(level, message)` which currently does `println!`. We route this through tracing macros at the appropriate level so WASM log output appears in the unified log stream and respects `RUST_LOG` filtering.

**Step 1: Add imports**

Add to the `use` block at the top of `host.rs`:

```rust
use tracing::{debug, error, info, warn};
```

**Step 2: Update the `log()` host function**

Replace:
```rust
async fn log(&mut self, level: String, message: String) {
    println!("[{}] [{}] {}", level.to_uppercase(), self.module_id, message);
}
```
With:
```rust
async fn log(&mut self, level: String, message: String) {
    let module = self.module_id.as_str();
    match level.as_str() {
        "error" => error!(target: "wasm", module, "{message}"),
        "warn"  => warn!(target: "wasm", module, "{message}"),
        "info"  => info!(target: "wasm", module, "{message}"),
        "debug" => debug!(target: "wasm", module, "{message}"),
        _       => tracing::trace!(target: "wasm", module, "{message}"),
    }
}
```

The `target: "wasm"` field lets users filter WASM module logs independently: `RUST_LOG=wasm=debug,vessel=info`.

**Step 3: Replace WebSocket connect error `eprintln!`**

In `websocket_connect()`, replace:
```rust
eprintln!("[{}] WS connect failed: {}", module_id, e);
```
With:
```rust
error!(module = %module_id, "WebSocket connect failed: {e}");
```

**Step 4: Verify**

```bash
cargo build 2>&1
```

Expected: clean build. No `println!` or `eprintln!` should remain anywhere in `src/`. Verify:

```bash
grep -rn "println!\|eprintln!" src/
```

Expected: no matches.

**Step 5: Commit**

```bash
git add src/wasm/host.rs
git commit -m "feat(logging): route WASM log() through tracing in host.rs"
```

---

## Usage Reference

```bash
# Default (INFO and above)
cargo run

# See every message exchanged
RUST_LOG=vessel=debug cargo run

# Full trace including raw JSON payloads
RUST_LOG=vessel=trace cargo run

# Trace only a specific module
RUST_LOG=vessel=info,vessel::modules::discord=debug cargo run

# WASM module logs at debug, everything else at info
RUST_LOG=vessel=info,wasm=debug cargo run
```
