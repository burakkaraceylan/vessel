import {
	compileExpression,
	useDotAccessOperatorAndOptionalChaining,
} from "filtrex";

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
