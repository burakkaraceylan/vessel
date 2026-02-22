# Vessel

Vessel is a modular desktop automation backend that bridges external systems (Discord, OBS, Home Assistant, Unity, etc.) with a custom touch-screen UI. Think of it as an open-source alternative to Bitfocus Companion — but instead of a grid of buttons designed for Stream Deck, Vessel targets touchscreens with rich widgets (sliders, knobs, toggles, gauges).

## Vision

- **Vessel (Rust backend)**: Runs on the host PC. Manages modules, handles communication with external systems, serves the web UI.
- **Touch UI (React frontend)**: Runs in a browser on a tablet/phone/touchscreen. Connects to Vessel via WebSocket. Dashboards auto-switch based on the active window (e.g., switch to Unity dashboard when Unity is focused).
- **Modules**: Each module bridges one external system. Built-in modules compile into Vessel. Future: WASM plugin marketplace for community modules.

## Current State

### Working
- Module trait system with async command/event channels
- ModuleManager: registration, per-module command routing, shared event aggregation
- TCP protocol: newline-delimited JSON, bidirectional (commands in, events out)
- Discord module: full OAuth2 auth, IPC pipe communication, voice control, event subscriptions
- Discord command handling: mute, deaf, volume, device selection, voice channels

### Not Yet Implemented
- WebSocket transport (for browser-based touch UI)
- Embedded web server to serve React frontend
- Active window detection module
- Configuration file (vessel.toml)
- Module variable/action/feedback announcement protocol

## Architecture

```
Touch UI (React) <--WebSocket--> Vessel <--channels--> ModuleManager
                                   |                        |
                                   |               +--------+--------+
                                   |               |        |        |
                              TCP (legacy)    Discord   Window   Future...
                                              Module    Module
```

### Data Flow

**Inbound (UI -> Module):**
1. UI sends JSON over WebSocket: `{"module":"discord","action":"set_mute","params":{"mute":true}}`
2. `handle_connection` parses `IncomingMessage`, calls `module_manager.route_command()`
3. Command sent via per-module mpsc channel to the module's `run()` loop
4. Module processes command (e.g., calls Discord IPC), emits result as `ModuleEvent`

**Outbound (Module -> UI):**
1. Module sends `ModuleEvent` via shared `event_tx` channel
2. `handle_connection` receives from `event_rx`, serializes as `OutgoingMessage`
3. Written as JSON line to the client: `{"module":"discord","event":"voice_settings_update","data":{...}}`

### Wire Protocol

Newline-delimited JSON (`\n` terminated).

```json
// Client -> Vessel
{"module": "discord", "action": "set_mute", "params": {"mute": true}}

// Vessel -> Client
{"module": "discord", "event": "voice_settings_update", "data": {"mute": true, "deaf": false}}
```

## Project Structure

```
src/
  main.rs              -- Vessel struct, TCP accept loop, handle_connection
  module.rs            -- Module trait, ModuleCommand, ModuleEvent, ModuleContext
  module_manager.rs    -- ModuleManager: register, route_command, run_all, take_event
  protocol.rs          -- IncomingMessage (Deserialize), OutgoingMessage (Serialize)
  modules.rs           -- Module namespace (pub mod discord, etc.)
  modules/
    discord.rs         -- DiscordModule: implements Module trait, command dispatch
    discord/
      commands.rs      -- DiscordCommand enum + FromModuleCommand parsing
      events.rs        -- DiscordEvent enum + IntoModuleEvent conversion
      ipc.rs           -- Low-level Discord IPC (Windows named pipes, binary framing)
      oauth.rs         -- OAuth2 token exchange/refresh with Discord API
      token_cache.rs   -- Token persistence to disk (%APPDATA%/Local/vessel/)
      voice.rs         -- DiscordVoiceController: high-level voice control API
```

## Key Patterns

### Adding a New Module

1. Create `src/modules/mymodule.rs` implementing `Module` trait
2. Add `pub mod mymodule;` to `src/modules.rs`
3. Register in `main.rs`: `vessel.module_manager.register_module(my_module)`

Each module gets:
- `ctx.rx`: receives `ModuleCommand` (action + params) from clients
- `ctx.event_tx`: sends `ModuleEvent` back to clients
- `ctx.cancel_token`: for graceful shutdown

### Module Command/Event Pattern

Each module should define:
- A command enum implementing `FromModuleCommand` (parses action string + params JSON)
- An event enum implementing `IntoModuleEvent` (converts to ModuleEvent with source + event name + data)

See `discord/commands.rs` and `discord/events.rs` for the reference implementation.

## Roadmap

### Phase 1: PoC (current)
- [x] Module system with trait-based architecture
- [x] TCP protocol with JSON line framing
- [x] Discord module: auth, voice control, event streaming
- [x] Bidirectional command/event flow working end-to-end
- [x] Test with ncat to verify full round-trip

### Phase 2: WebSocket + Web UI
- [x] Add WebSocket server alongside TCP (tokio-tungstenite)
- [ ] Embedded HTTP server to serve React static files
- [ ] Minimal React touch UI: connect via WS, display events, send commands
- [ ] Discord dashboard with mute/deaf toggles, volume sliders

### Phase 3: Active Window Module
- [ ] Windows API (GetForegroundWindow) module that emits focus_changed events
- [ ] UI auto-switches dashboard based on active application

### Phase 4: Polish
- [ ] Configuration file (vessel.toml) for server address, module settings
- [ ] Module variable announcement protocol (modules declare their variables on connect)
- [ ] Error handling and reconnection logic
- [ ] Logging with tracing

### Future
- Unity Editor module (scene switching, play mode control)
- OBS module
- Home Assistant module
- Bambu Lab 3D printer module (print progress, status, remote control via MQTT)
- Windows Notifications module (UserNotificationListener WinRT API — per-app notification counts and toast content, drives widget badges)
- WASM plugin system for community modules
- Module marketplace
- Visual node editor for complex action sequences (trigger → condition → action chains)

## Touch UI Architecture

### Tech Stack
- **Bundler**: Vite
- **Language**: TypeScript
- **Framework**: React
- **WebSocket**: Native API + `reconnecting-websocket` for auto-reconnect
- **Styling**: Tailwind CSS with CSS custom properties for theming
- **State**: Zustand
- **Dashboard layout**: react-grid-layout
- **Unstyled primitives**: Radix UI (slider, toggle, switch, etc.)
- **Custom widgets**: Web Components (framework-agnostic, no build step for authors)

### Core Data Model

**Dashboard**: A named page of widgets, user-created. Stored on Vessel backend as JSON.
```
Dashboard
  ├── id, name
  ├── columns, rows (grid dimensions)
  ├── theme override (optional)
  └── widgets: WidgetInstance[]
        ├── id, type (e.g., "button", "slider", "community.eq-visualizer")
        ├── position: { col, row }
        ├── size: { w, h }
        └── config (type-specific, defined by widget's configSchema)
```

**ActionBinding**: What happens when a user interacts with a widget.
```json
{ "module": "discord", "action": "set_mute", "params": { "mute": true } }
```

**ValueBinding**: What live state a widget reflects.
```json
{ "module": "discord", "event": "voice_settings_update", "field": "volume", "writeAction": { ... } }
```

### Two Modes
- **Viewer mode** (default): Widgets fill the screen, touch fires actions, live data flows in.
- **Editor mode**: Grid visible, drag to reposition, tap widget to configure, add from palette.

### Widget Plugin System

All widgets (built-in and custom) implement the same contract:

**WidgetDefinition**: Declares type, label, icon, defaultSize, and `configSchema`.
**configSchema**: Array of field descriptors (`text`, `color`, `number`, `icon-picker`, `action-binding`, `value-binding`, `select`, `bool`). The editor auto-generates config panels from this — no per-widget config UI needed.

**WidgetProps** (what every widget receives):
- `config`: the user's config values
- `state`: live values from value bindings
- `sendAction`: function to fire a command
- `size`: grid cells occupied

**Three tiers of custom widgets:**
1. **Presets (JSON-only)**: Pre-configured existing widgets (e.g., "Discord Mute Toggle" = button with prefilled action). Zero risk, shareable as tiny JSON.
2. **Web Components (JS file)**: New visual elements. Single `.js` file, no build step, loaded from `vessel-data/widgets/`. Gets a config panel for free via configSchema.
3. **WASM widgets (future)**: For heavy computation (audio visualizers, 3D).

Widget folder structure:
```
vessel-data/widgets/community.eq-visualizer/
  manifest.json    -- metadata, configSchema, icon
  widget.js        -- the Web Component
  preview.png      -- screenshot for the palette
```

### Theming

Themes are CSS custom property maps, stored as JSON:
```json
{
  "name": "Cyberpunk",
  "variables": {
    "--bg-primary": "#0a0a0a",
    "--accent": "#ff00ff"
  }
}
```
Applied at runtime via `document.documentElement.style.setProperty()`. Per-dashboard theme overrides supported. Users can also inject global `custom.css` for full override power.

### Customization Layers

**Casual user**: Drag widgets, pick colors, set actions.
**Power user**: Multi-action sequences (with delays), configurable gestures (tap, longPress, swipeUp, swipeDown, doubleTap), conditional actions via expressions.
**Tinkerer**: Per-widget CSS overrides, reactive state visuals (widget changes appearance based on expressions), user-defined variables, conditional widget visibility, import/export dashboards/themes.
**Developer**: Custom Web Component widgets, REST API integration, WASM plugins.

### Expressions

A safe expression evaluator (not full JS) for reactive behavior:
- Dynamic labels: `"${discord.mute ? 'MUTED' : 'Live'}"`
- Reactive colors: `"${obs.streaming ? '#e94560' : '#333'}"`
- Conditional visibility: `"${vars.streamMode == true}"`
- Widget state visuals: map expressions to visual presets (colors, icons, animations)

### REST API (served by Vessel)

```
GET    /api/dashboards          -- list all dashboards
GET    /api/dashboards/:id      -- get one dashboard
PUT    /api/dashboards/:id      -- save/update
DELETE /api/dashboards/:id      -- delete
GET    /api/widgets             -- widget registry (built-in + custom)
POST   /api/action              -- fire any action programmatically
GET    /api/state               -- read all module states
POST   /api/variable            -- set a user-defined variable
```

### Module Announcement Protocol

Modules declare their capabilities so the UI can build config dropdowns dynamically:
```json
{
  "module": "discord",
  "actions": [
    { "name": "set_mute", "label": "Set Mute", "params": [{ "name": "mute", "type": "bool" }] }
  ],
  "events": [
    { "name": "voice_settings_update", "fields": ["mute", "deaf", "volume"] }
  ]
}
```

### UI Build Order

1. Scaffold React + Vite + TS, connect to WebSocket
2. `ButtonWidget` conforming to WidgetDefinition + WidgetProps contract
3. Dashboard data model, render grid of widgets from JSON config
4. Editor mode: add/remove/reposition widgets
5. Generic config panel driven by configSchema
6. Module announcement protocol (Rust side)
7. Dashboard persistence (Rust REST API)
8. Value bindings + live state display on widgets
9. More widget types (slider, toggle, knob, gauge)
10. Auto-switch dashboards on window focus
11. Expression evaluator for reactive behavior
12. Custom widget loading (Web Components from vessel-data/widgets/)
13. Import/export (dashboards, themes, widget presets)
14. Gesture system (longPress, swipe, doubleTap)

## Development

```bash
cargo build
cargo run

# Test with ncat:
ncat 127.0.0.1 8001
{"module":"discord","action":"get_voice_settings","params":{}}
```

## AI Assistant Rules

- **Do not modify code or provide full edits unless explicitly asked.** Guide, explain, and point toward the right direction instead. The goal of this project is to learn Rust.

## Conventions

- Async runtime: tokio with full features
- Error handling: anyhow for module internals, Box<dyn Error> at boundaries
- Channel capacity: 32 for both per-module command channels and shared event channel
- Module names are &'static str (set by Module::name()), command targets are String (from JSON)
- All modules run as spawned tokio tasks; ModuleManager owns the channels
