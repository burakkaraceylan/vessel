import {
	compileExpression,
	useDotAccessOperatorAndOptionalChaining,
} from "filtrex";
import type { Zone, ZoneProfile } from "@/types/dashboard";

const OPTIONS = { customProp: useDotAccessOperatorAndOptionalChaining };

const EXPR_RE = /^\$\{(.+)\}$/s;

export function resolveValue(
	value: string,
	moduleState: Record<string, unknown>,
): string {
	const match = value.match(EXPR_RE);
	if (!match) return value;

	try {
		const result = compileExpression(match[1], OPTIONS)(moduleState);
		return String(result);
	} catch (e) {
		return value;
	}
}

export function resolveActiveProfile(
	zone: Zone,
	moduleState: Record<string, unknown>,
): ZoneProfile | null {
	if (!zone.profiles) return null;
	const defaultProfile = zone.profiles.find((p) => p.default);

	for (const profile of zone.profiles) {
		if (!profile.condition) continue;
		const match = profile.condition.match(EXPR_RE);
		if (!match) continue;

		try {
			const result = compileExpression(match[1], OPTIONS)(moduleState);
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
