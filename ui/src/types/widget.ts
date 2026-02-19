/** biome-ignore-all lint/suspicious/noExplicitAny: <explanation> */

export type WidgetComponent = React.FC<WidgetProps<any>>;

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

export type WidgetConfig = ButtonConfig | ImageConfig; // Extend this union type as you add more widget types

export interface ButtonConfig {
	icon?: string;
	label?: string;
	image?: string;
	backgroundColor?: string;
	borderColor?: string;
	action: ActionBinding;
	valueBinding?: ValueBinding;
}

export interface ImageConfig {
	image?: string;
	backgroundColor?: string;
	borderColor?: string;
	label?: string;
	labelPosition?: "t" | "b" | "l" | "r" | "c" | "tl" | "tr" | "bl" | "br";
}

export interface ValueBinding {
	module: string;
	event: string;
	key: string;
}
export interface ActionBinding {
	module: string;
	action: string;
	params: Record<string, any>;
}

export interface WidgetProps<T extends WidgetConfig = WidgetConfig> {
	config: T;
	state: Record<string, unknown>;
	sendAction: (action: ActionBinding) => void;
	size: {
		width: number;
		height: number;
	};
	resolve: (value: string) => string;
}
