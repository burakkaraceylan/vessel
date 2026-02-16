# Vessel

A modular desktop automation backend that bridges external systems (Discord, OBS, Home Assistant, Unity, etc.) with a custom touchscreen UI.

Think of it as an open-source alternative to [Bitfocus Companion](https://bitfocus.io/companion) — but instead of a grid of buttons designed for Stream Deck, Vessel targets touchscreens with rich widgets: sliders, knobs, toggles, and gauges.

> **Early development** — Vessel is under active development and not yet ready for general use. Expect breaking changes.

## How It Works

Vessel runs on your PC as a single binary. You open a browser on your tablet, phone, or any touchscreen — it connects to Vessel and gives you a fully customizable control surface. Dashboards can auto-switch based on the active window (e.g., show your Discord controls when Discord is focused, your OBS controls when OBS is focused).

```
Tablet/Phone (browser)
        |
    WebSocket
        |
    Vessel (Rust backend)
        |
   +---------+---------+
   |         |         |
Discord    OBS     Home Assistant
```

### Modules

Each external system is a **module**. Modules handle bidirectional communication: they receive commands from the UI and push real-time events back.

**Available now:**
- **Discord** — Mute, deafen, volume control, device selection, voice channel management

**Planned:**
- OBS (scene switching, recording/streaming control)
- Home Assistant (lights, switches, sensors)
- Unity Editor (play mode, scene switching)
- Active Window Detection (auto-switch dashboards)

### Dashboards

Dashboards are JSON files — one file per dashboard. Drop a `.json` file into your `config/dashboards/` folder and it shows up in the UI. Share dashboards by sharing the file.

```json
{
  "name": "Discord",
  "match_window": "Discord",
  "grid": { "columns": 4, "rows": 3 },
  "widgets": [
    {
      "type": "toggle",
      "position": { "col": 0, "row": 0 },
      "binding": { "module": "discord", "action": "set_mute", "param": "mute" },
      "label": "Mute"
    }
  ]
}
```

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2024)
- Windows (Linux/macOS support planned)

### Build & Run

```bash
cargo build
cargo run
```

Vessel starts a server on `127.0.0.1:8080`. Connect from any device on your local network.

### Testing with ncat

You can test the protocol directly without the UI:

```bash
ncat 127.0.0.1 8001
{"module":"discord","action":"get_voice_settings","params":{}}
```

## Wire Protocol

Newline-delimited JSON over WebSocket (or TCP).

```jsonc
// Client -> Vessel
{"module": "discord", "action": "set_mute", "params": {"mute": true}}

// Vessel -> Client
{"module": "discord", "event": "voice_settings_update", "data": {"mute": true, "deaf": false}}
```

## Roadmap

- [x] Module system with async command/event channels
- [x] TCP + WebSocket transport
- [x] Discord module (OAuth2, voice control, event streaming)
- [ ] Embedded web server serving the React UI
- [ ] Touch UI with drag-and-drop dashboard editor
- [ ] Active window detection (auto-switch dashboards)
- [ ] Configuration file (`vessel.toml`)
- [ ] OBS, Home Assistant, Unity modules
- [ ] WASM plugin system for community modules

## Contributing

Vessel is in early development and contributions are welcome. If you're interested in writing a module, see [ARCHITECTURE.md](ARCHITECTURE.md) for the module system design.

## License

TBD
