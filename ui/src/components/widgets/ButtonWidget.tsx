import { useState } from "react";
import type {
	ButtonConfig,
	WidgetComponent,
	WidgetDefinition,
	WidgetProps,
} from "@/types/widget";

const ButtonWidget: React.FC<WidgetProps<ButtonConfig>> = ({
	config,
	sendAction,
	state,
	resolve,
}) => {
	const [isActive, setIsActive] = useState(false);

	const handleClick = () => {
		const params = config.action.params || {};
		const resolvedParams: Record<string, unknown> = {};
		for (const [key, value] of Object.entries(params)) {
			if (value === "$toggle") {
				resolvedParams[key] = !state[config.valueBinding?.key ?? key];
			} else {
				resolvedParams[key] = value;
			}
		}
		const action = resolve(config.action.action);
		sendAction({ ...config.action, action, params: resolvedParams });
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
				opacity: isActive ? 0.5 : 1,
				transition: "opacity 0.1s",
			}}
			onMouseDown={() => setIsActive(true)}
			onMouseUp={() => setIsActive(false)}
			onMouseLeave={() => setIsActive(false)}
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
