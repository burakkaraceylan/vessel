# Expression Context Normalization

**Date:** 2026-02-24
**Status:** Approved

## Problem

Filtrex (the expression engine used for `${}` templates) only accepts valid JS-like identifiers. Two categories of module state keys violate this:

- Module names containing hyphens: `home-assistant` → tokenized as `home - assistant` (subtraction)
- HA entity IDs containing dots: `light.phillips_hue` → interpreted as nested property access (`light` → `phillips_hue`) rather than a single key

The moduleState store structure is:

```
{
  "home-assistant": {
    "light.phillips_hue": { "state": "on", "attributes": { ... } }
  }
}
```

## Decision

Normalize the expression context before passing it to filtrex. Apply the universal flatten rule:

- Replace `-` and `.` with `_` in **module keys**
- Replace `-` and `.` with `_` in **event keys**
- Leave inner data fields untouched

Examples:

| Wire key | Expression identifier |
|---|---|
| `home-assistant` | `home_assistant` |
| `light.phillips_hue` | `light_phillips_hue` |
| `voice_settings_update` | `voice_settings_update` (unchanged) |
| `track_changed` | `track_changed` (unchanged) |

Resulting expression syntax:

```
home_assistant.light_phillips_hue.state == "on"
discord.voice_settings_update.mute
media.track_changed.title
```

## Scope

### What changes

**`ui/src/lib/expressions.ts`**
Add `normalizeForExpression(state)` — maps over the two outer key levels, applies `key.replace(/[-\.]/g, '_')` to each. Call it inside both `resolveValue` and `resolveActiveProfile` before passing state to filtrex.

**`ui/src/assets/discord-dashboard.json`**
Update the HA button widget's `image` expression from:
```
state["home-assistant"]["light.phillips_hue"] == "on" ? ...
```
to:
```
home_assistant.light_phillips_hue.state == "on" ? ...
```

### What does not change

- `moduleState.ts` — store keeps raw wire keys
- `WidgetShell.tsx` — `valueBinding` uses raw keys (`moduleState[b.module]?.[b.event]?.[b.key]`), untouched
- Backend wire protocol — unchanged

## Alternatives Considered

**Nesting by dot:** `light.phillips_hue` → `{ light: { phillips_hue: ... } }`, giving `home_assistant.light.phillips_hue.state`. Preserves HA domain semantics but introduces depth inconsistency (HA expressions are 4 levels, all others are 3). Rejected in favour of the simpler universal rule.

**Replace filtrex with jexl:** supports bracket notation natively, wire names match expressions exactly. Rejected — larger migration, more complex expression syntax for users (`state["home-assistant"]`).
