/** biome-ignore-all lint/suspicious/noExplicitAny: <explanation> */

export type WidgetComponent = React.FC<WidgetProps>;

export interface WidgetInstance {
	id: string;
	type: "button" | "slider" | "toggle" | "knob" | "gauge" | "label";
	position: { col: number; row: number };
	size: { w: number; h: number }; // a button is 1x1, a slider might be 3x1
	config: WidgetConfig; // type-specific
}

export interface WidgetDefinition {
	type: string; // "button", "slider", "community.eq-visualizer"
	label: string; // "Button", "EQ Visualizer"
	icon: string; // shown in the widget palette
	defaultSize: { w: number; h: number };
	configSchema: ConfigField[]; // describes what the config panel should render
	component: WidgetComponent; // the actual renderable
}

interface ConfigField {
	key: string;
	label: string;
	type:
		| "text"
		| "color"
		| "number"
		| "icon-picker"
		| "action-binding"
		| "value-binding"
		| "select"
		| "bool";
	default?: unknown;
	options?: { label: string; value: string }[]; // for "select"
}

type WidgetConfig = ButtonConfig; // Extend this union type as you add more widget types

interface ButtonConfig {
	icon: string;
	label: string;
	backgroundColor: string;
	action: ActionBinding;
}

interface ActionBinding {
	module: string;
	action: string;
	params: Record<string, any>;
}

export interface WidgetProps {
	config: WidgetConfig;
	state: Record<string, unknown>;
	sendAction: (action: ActionBinding) => void;
	size: {
		width: number;
		height: number;
	};
}
