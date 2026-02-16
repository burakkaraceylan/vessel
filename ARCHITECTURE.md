# Vessel — Modular Architecture Plan

## Context

Vessel is a Bitfocus Companion client that communicates with Companion over TCP and controls various systems (Discord, active window, telemetry, etc.) through independent modules. Currently, the Discord module is fully implemented but wired directly in `main.rs` as demo code. We need a unified trait-based module system so modules can be added, started, and communicated with in a consistent way.

**Key constraint:** Discord IPC uses blocking `std::fs::File` I/O (`src/modules/discord/ipc.rs:36`), while the rest of the app is async tokio. This must be bridged.

---

## Architecture Overview

```
Companion (TCP) <--JSON--> Vessel TCP Handler <--channels--> ModuleManager
                                                                 |
                                                    +------------+------------+
                                                    |            |            |
                                              DiscordModule  TelemetryMod  FutureMod...
                                              (blocking thread)  (async)     (async)
```

- **Inbound:** Companion sends JSON commands over TCP. The TCP handler parses them, looks up the target module by name, and forwards via a per-module `mpsc` channel.
- **Outbound:** All modules push events into a single shared `mpsc` channel. The TCP handler reads from it and writes JSON lines back to Companion.

---

## 1. Module Trait — `src/module.rs` (new file)

```rust
#[async_trait]
pub trait Module: Send + Sync {
    /// Unique routing key, e.g. "discord", "telemetry"
    fn name(&self) -> &'static str;

    /// Main run loop. Process commands from ctx.command_rx,
    /// emit events via ctx.event_tx, exit on ctx.cancel.
    async fn run(&mut self, ctx: ModuleContext) -> Result<()>;
}
```

Supporting types in the same file:

| Type | Purpose |
|------|---------|
| `ModuleCommand { action: String, params: Value }` | Inbound command from Companion to a module |
| `ModuleEvent { module: String, event: String, data: Value }` | Outbound event from a module to Companion |
| `ModuleContext { command_rx, event_tx, cancel }` | Handed to each module at startup |

**Dependency:** add `async-trait = "0.1"` to `Cargo.toml`. (Native async traits in edition 2024 don't support `dyn` dispatch.)

---

## 2. Module Manager — `src/module_manager.rs` (new file)

Responsibilities:
- **Register** modules before startup via `register(Box<dyn Module>)`
- **Start** all modules — each gets its own `tokio::spawn` task with a dedicated `ModuleContext`
- **Route** commands — `route_command(module_name, cmd)` looks up the module's `mpsc::Sender` in a `HashMap<&'static str, Sender<ModuleCommand>>` and sends
- **Expose** a single `mpsc::Receiver<ModuleEvent>` (via `take_event_rx()`) for the TCP handler to consume

Key design decisions:
- Per-module command channel (capacity 64) — modules have different command shapes; avoids broadcast filtering
- Single shared event channel (capacity 256) — all events go to the same TCP socket anyway
- `start()` drains the modules vec and moves each into its spawned task; after start, interaction is channels only
- Wrap in `Arc` after start so the TCP handler can call `route_command()`

---

## 3. TCP Protocol — `src/protocol.rs` (new file)

**Wire format:** newline-delimited JSON (one JSON object per `\n`).

Companion -> Vessel:
```json
{"module": "discord", "action": "set_mute", "params": {"mute": true}}
```

Vessel -> Companion:
```json
{"module": "discord", "event": "voice_settings_update", "data": {"mute": true, "deaf": false}}
```

Serde types:
- `IncomingMessage { module: String, action: String, params: Value }`
- `OutgoingMessage { module: String, event: String, data: Value }`

Connection handler function (`handle_connection`):
- Splits `TcpStream` into reader/writer
- `tokio::select!` loop:
  - Read lines from Companion → parse `IncomingMessage` → `manager.route_command()`
  - Recv from `event_rx` → serialize `OutgoingMessage` → write line to Companion
  - Cancel token → break

---

## 4. Discord Module Refactor — `src/modules/discord.rs`

The existing sub-modules (`ipc.rs`, `oauth.rs`, `token_cache.rs`, `voice.rs`) stay **unchanged**. Add a `DiscordModule` struct in `discord.rs` that implements `Module`.

**Blocking I/O bridge pattern:**

Since `DiscordVoiceController` does blocking named pipe reads, the module spawns a dedicated **OS thread** (`std::thread::spawn`) that owns the controller. Communication between the async `run()` loop and the blocking thread uses channels:

```
[async run() loop]  --mpsc-->  [blocking OS thread owns DiscordVoiceController]
                    <--oneshot-- (reply per command)
                    <--mpsc---- (Discord events pushed via blocking_send)
```

The async side:
1. Calls `connect_and_auth()` (async, because OAuth uses reqwest)
2. Spawns the blocking thread, handing it the controller
3. Enters `select!` loop: receives `ModuleCommand` from context, translates to a `DiscordRequest`, sends to blocking thread, awaits oneshot reply, emits result as `ModuleEvent`

The blocking thread:
1. Subscribes to voice settings events
2. Loops: `try_recv()` for command requests (non-blocking), then `recv_event()` for Discord IPC events (blocking read)
3. Pushes Discord events to the shared event channel via `event_tx.blocking_send()`
4. Exits when cancel token is cancelled (checked between iterations)

**Shutdown caveat:** `recv_event()` blocks on pipe read. The thread won't notice cancellation until the next event arrives or the pipe closes. This is acceptable for now — graceful shutdown will complete within a few seconds. A future improvement could set a read timeout on the pipe handle.

Action-to-method mapping (example subset):

| action string | DiscordVoiceController method |
|---|---|
| `"set_mute"` | `set_mute(params["mute"])` |
| `"set_deaf"` | `set_deaf(params["deaf"])` |
| `"set_input_volume"` | `set_input_volume(params["volume"])` |
| `"set_output_volume"` | `set_output_volume(params["volume"])` |
| `"get_voice_settings"` | `get_voice_settings()` |
| `"get_selected_voice_channel"` | `get_selected_voice_channel()` |

---

## 5. Configuration — `src/config.rs` (new file)

Load a `vessel.toml` config file from the working directory (or a path passed as CLI arg).

```rust
#[derive(Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub discord: Option<DiscordConfig>,
    // pub telemetry: Option<TelemetryConfig>,  // future modules
}

#[derive(Deserialize)]
pub struct ServerConfig {
    pub address: String,  // e.g. "127.0.0.1:8001"
}

#[derive(Deserialize)]
pub struct DiscordConfig {
    pub client_id: String,
    pub client_secret: String,
}
```

Example `vessel.toml`:
```toml
[server]
address = "127.0.0.1:8001"

[discord]
client_id = "1471623206942146844"
client_secret = "pH0cd2Isv2UswQv9BX5GxnEdSdYEdw5T"
```

Each module section is `Option<T>` — if the section is absent, the module is not registered. This gives a clean way to enable/disable modules without code changes.

**Dependency:** add `toml = "0.8"` to `Cargo.toml`.

---

## 6. Main Entry Point — `src/main.rs`

Rewrite to:
1. Load `vessel.toml` config
2. Init tracing
3. Create `CancellationToken` + Ctrl+C handler (already exists)
4. Create `ModuleManager`, conditionally register modules based on config:
   ```rust
   if let Some(discord) = &config.discord {
       manager.register(Box::new(DiscordModule::new(discord.client_id.clone(), discord.client_secret.clone())));
   }
   ```
5. `take_event_rx()` before starting
6. `manager.start()` — spawns all module tasks (all start together, run until shutdown)
7. Wrap manager in `Arc`
8. TCP accept loop → `handle_connection(stream, manager, event_rx, cancel)`

Delete `messages.rs` — replaced by `protocol.rs` and `module.rs`.

---

## 7. File Changes Summary

| File | Action |
|------|--------|
| `src/module.rs` | **Create** — Module trait + ModuleCommand/Event/Context |
| `src/module_manager.rs` | **Create** — ModuleManager |
| `src/protocol.rs` | **Create** — Wire format types + handle_connection |
| `src/config.rs` | **Create** — Config structs + TOML loading |
| `vessel.toml` | **Create** — Default config file |
| `src/modules/discord.rs` | **Modify** — Add DiscordModule impl, keep sub-module re-exports |
| `src/modules/discord/ipc.rs` | No change |
| `src/modules/discord/oauth.rs` | No change |
| `src/modules/discord/token_cache.rs` | No change |
| `src/modules/discord/voice.rs` | No change |
| `src/main.rs` | **Rewrite** — Config loading, module registration, TCP loop |
| `src/messages.rs` | **Delete** — Superseded |
| `src/modules.rs` | No change (already has `pub mod discord;`) |
| `Cargo.toml` | **Modify** — Add `async-trait = "0.1"`, `toml = "0.8"` |

---

## 8. Adding a New Module (the pattern)

To prove the architecture works, here's what adding a `telemetry` module looks like:

1. Create `src/modules/telemetry.rs` — implement `Module` trait
2. Add `pub mod telemetry;` to `src/modules.rs`
3. Add `manager.register(Box::new(TelemetryModule::new()))` in `main.rs`

Zero changes to the trait, manager, protocol, or any existing module.

---

## 9. Verification

1. `cargo build` — confirms everything compiles
2. Start vessel, connect a TCP client (e.g. `netcat 127.0.0.1 8001`)
3. Send: `{"module": "discord", "action": "get_voice_settings", "params": {}}`
4. Expect: JSON line back with `{"module": "discord", "event": "get_voice_settings_result", "data": {...}}`
5. Send: `{"module": "discord", "action": "set_mute", "params": {"mute": true}}`
6. Verify Discord actually mutes
