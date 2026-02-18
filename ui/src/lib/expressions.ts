function evaluate(expr: string, ctx: Record<string, unknown>): unknown {
	const fn = new Function(...Object.keys(ctx), `return (${expr})`);
	return fn(...Object.values(ctx));
}
