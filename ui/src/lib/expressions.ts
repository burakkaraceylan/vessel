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
		console.log(moduleState);
		const result = compileExpression(match[1], OPTIONS)(moduleState);
		console.log("Expression result:", result);
		return String(result);
	} catch (e) {
		console.error("Expression failed:", match[1], e);
		return value;
	}
}
