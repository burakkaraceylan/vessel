import {
	compileExpression,
	useDotAccessOperatorAndOptionalChaining,
} from "filtrex";
import type { Zone, ZoneProfile } from "@/types/dashboard";

const OPTIONS = { customProp: useDotAccessOperatorAndOptionalChaining };

const EXPR_RE = /^\$\{(.+)\}$/s;

function sanitizeKey(key: string): string {
	return key.replace(/[-.]/g, "_");
}

// Normalizes moduleState keys for filtrex evaluation. Filtrex requires
// valid JS identifiers, so hyphens and dots in module/event names are
// replaced with underscores (e.g. "home-assistant" -> "home_assistant",
// "light.phillips_hue" -> "light_phillips_hue"). Inner data fields are
// untouched. Note: valueBinding lookups in WidgetShell bypass this function
// and use raw wire-format keys directly against the Zustand store.
function normalizeForExpression(
	state: Record<string, Record<string, unknown>>,
): Record<string, Record<string, unknown>> {
	const result: Record<string, Record<string, unknown>> = {};
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

export function resolveValue(
	value: string,
	moduleState: Record<string, Record<string, unknown>>,
): string {
	const match = value.match(EXPR_RE);
	if (!match) return value;

	try {
		const result = compileExpression(
			match[1],
			OPTIONS,
		)(normalizeForExpression(moduleState));
		return String(result);
	} catch (e) {
		console.error(`Error evaluating expression: ${value}`, e);
		return value;
	}
}

export function resolveActiveProfile(
	zone: Zone,
	moduleState: Record<string, Record<string, unknown>>,
): ZoneProfile | null {
	if (!zone.profiles) return null;
	const defaultProfile = zone.profiles.find((p) => p.default);

	for (const profile of zone.profiles) {
		if (!profile.condition) continue;
		const match = profile.condition.match(EXPR_RE);
		if (!match) continue;

		try {
			const result = compileExpression(
				match[1],
				OPTIONS,
			)(normalizeForExpression(moduleState));
			if (result) return profile;
		} catch (e) {
			console.error(
				`Error evaluating profile condition: ${profile.condition}`,
				e,
			);
		}
	}

	return defaultProfile || null;
}
