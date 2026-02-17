import type {
	WidgetComponent,
	WidgetDefinition,
	WidgetProps,
} from "@/types/widget";

const ButtonWidget: WidgetComponent = ({ config, sendAction, state, size }) => {
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
				backgroundColor: "black",
				width: size.width,
				height: size.height,
			}}
			onClick={handleClick}
		>
			{state.mute ? "Unmute" : "Mute"}
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
