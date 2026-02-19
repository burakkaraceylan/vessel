import type {
	WidgetComponent,
	WidgetDefinition,
	WidgetProps,
} from "@/types/widget";

const ButtonWidget: WidgetComponent = ({ config, sendAction, state, resolve }) => {
	const handleClick = () => {
		const resolvedParams: Record<string, unknown> = {};
		for (const [key, value] of Object.entries(config.action.params)) {
			if (value === "$toggle") {
				resolvedParams[key] = !state[config.valueBinding?.key ?? key];
			} else {
				resolvedParams[key] = value;
			}
		}
		sendAction({ ...config.action, params: resolvedParams });
	};

	const bg = resolve(config.backgroundColor ?? "var(--bg-widget)");
	const border = resolve(config.borderColor ?? "var(--border-color)");
	const image = config.image ? resolve(config.image) : undefined;
	const label = resolve(config.label ?? "");

	return (
		<button
			type="button"
			style={{
				backgroundColor: bg,
				backgroundImage: image ? `url(${image})` : undefined,
				backgroundSize: "cover",
				color: "var(--text-primary)",
				border: `1px solid ${border}`,
				borderRadius: "var(--widget-radius)",
				width: "100%",
				height: "100%",
				cursor: "pointer",
				fontSize: "0.875rem",
				fontWeight: 500,
			}}
			onClick={handleClick}
		>
			{label}
		</button>
	);
};

export const buttonDefinition: WidgetDefinition = {
	type: "button",
	label: "Button",
	icon: "square",
	defaultSize: { w: 1, h: 1 },
	configSchema: [
		/* ... */
	],
	component: ButtonWidget,
};
