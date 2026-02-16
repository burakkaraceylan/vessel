import type {
	WidgetComponent,
	WidgetDefinition,
	WidgetProps,
} from "@/types/widget";

const ButtonWidget: WidgetComponent = ({ config, sendAction, state, size }) => {
	return (
		<button
			type="button"
			style={{
				backgroundColor: "black",
				width: size.width,
				height: size.height,
			}}
		>
			"Button"
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
	component: ButtonWidget, // ← the React component lives here
};
