# Expression Context Normalization Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:executing-plans to implement this plan task-by-task.

**Goal:** Make filtrex expressions work with hyphenated module names and dotted HA entity IDs by normalizing the expression context keys before evaluation.

**Architecture:** Add a `normalizeForExpression` utility that transforms the two outer key levels of `moduleState` — replacing `-` and `.` with `_` — and call it inside `resolveValue` and `resolveActiveProfile` before passing state to filtrex. `valueBinding` lookups bypass filtrex and are left untouched.

**Tech Stack:** TypeScript, filtrex 3.x

---

### Task 1: Add normalizeForExpression and wire it into expressions.ts

**Files:**
- Modify: `ui/src/lib/expressions.ts`

**Step 1: Add the normalize utility**

Insert this function at the top of the file, before `resolveValue`:

```typescript
function sanitizeKey(key: string): string {
	return key.replace(/[-\.]/g, "_");
}

function normalizeForExpression(
	state: Record<string, Record<string, unknown>>,
): Record<string, unknown> {
	const result: Record<string, unknown> = {};
	for (const [moduleKey, events] of Object.entries(state)) {
		const normalizedModule = sanitizeKey(moduleKey);
		const normalizedEvents: Record<string, unknown> = {};
		for (const [eventKey, data] of Object.entries(events)) {
			normalizedEvents[sanitizeKey(eventKey)] = data;
		}
		result[normalizedModule] = normalizedEvents;
	}
	return result;
}
```

**Step 2: Wire into resolveValue**

Change:
```typescript
const result = compileExpression(match[1], OPTIONS)(moduleState);
```
To:
```typescript
const result = compileExpression(match[1], OPTIONS)(normalizeForExpression(moduleState));
```

**Step 3: Wire into resolveActiveProfile**

Change:
```typescript
const result = compileExpression(match[1], OPTIONS)(moduleState);
```
To:
```typescript
const result = compileExpression(match[1], OPTIONS)(normalizeForExpression(moduleState));
```

**Step 4: Verify TypeScript compiles**

Run: `cd ui && npx tsc -b --noEmit`
Expected: no errors

**Step 5: Verify biome lint passes**

Run: `cd ui && npx biome check src/lib/expressions.ts`
Expected: no errors

**Step 6: Commit**

```bash
git add ui/src/lib/expressions.ts
git commit -m "feat(expressions): normalize module state keys for filtrex compatibility"
```

---

### Task 2: Update the HA expression in discord-dashboard.json

**Files:**
- Modify: `ui/src/assets/discord-dashboard.json`

**Context:** The `office` widget (id: "office") has an `image` expression that currently uses bracket notation which filtrex cannot parse:

```json
"image": "${state[\"home-assistant\"][\"light.phillips_hue\"] == \"on\" ? \"/packs/hexaza/030-EMOJIS_light_bulb.jpg\" : \"/packs/hexaza/030-EMOJIS_light_bulb_off.jpg\"}"
```

**Step 1: Update the expression**

Replace the `image` value on the office widget with:

```json
"image": "${home_assistant.light_phillips_hue.state == \"on\" ? \"/packs/hexaza/030-EMOJIS_light_bulb.jpg\" : \"/packs/hexaza/030-EMOJIS_light_bulb_off.jpg\"}"
```

Note: `home-assistant` → `home_assistant`, `light.phillips_hue` → `light_phillips_hue`, and the state field is now accessed as `.state` (part of the event data object).

**Step 2: Verify biome lint passes**

Run: `cd ui && npx biome check src/assets/discord-dashboard.json`
Expected: no errors (valid JSON)

**Step 3: Manual smoke test**

Run: `cd ui && yarn dev`

Open the dashboard in a browser. With Home Assistant connected and `light.phillips_hue` reporting state, the office button image should switch between the lit and unlit bulb images. With HA disconnected the expression should silently fall back to the raw string (unchanged behaviour for missing state).

**Step 4: Commit**

```bash
git add ui/src/assets/discord-dashboard.json
git commit -m "fix(dashboard): use normalized HA expression syntax for office light widget"
```
