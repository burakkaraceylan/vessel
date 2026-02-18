import type {
	WidgetComponent,
	WidgetDefinition,
	WidgetProps,
} from "@/types/widget";

const ButtonWidget: WidgetComponent = ({ config, sendAction, state }) => {
	const isActive = !!state[config.valueBinding?.key || ""];

	const handleClick = () => {
		const resolvedParams: Record<string, unknown> = {};
		for (const [key, value] of Object.entries(config.action.params)) {
			if (value === "$toggle") {
				resolvedParams[key] = !state[key];
			} else {
				resolvedParams[key] = value;
			}
		}
		sendAction({ ...config.action, params: resolvedParams });
	};

	return (
		<button
			type="button"
			style={{
				backgroundColor: isActive
					? config.activeBackgroundColor ||
						config.backgroundColor ||
						"var(--bg-widget)"
					: config.backgroundColor || "var(--bg-widget)",
				backgroundImage: isActive
					? config.activeImage
						? `url(${config.activeImage})`
						: undefined
					: config.image
						? `url(${config.image})`
						: undefined,
				backgroundSize: "cover",
				color: "var(--text-primary)",
				border: "1px solid var(--border-color)",
				borderRadius: "var(--widget-radius)",
				width: "100%",
				height: "100%",
				cursor: "pointer",
				fontSize: "0.875rem",
				fontWeight: 500,
			}}
			onClick={handleClick}
		>
			{isActive && config.activeLabel ? config.activeLabel : config.label}
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
