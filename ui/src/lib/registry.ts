import type { WidgetDefinition, WidgetProps } from "@/types/widget";

class WidgetRegistry {
	private registry: Map<string, WidgetDefinition>;

	constructor() {
		this.registry = new Map();
	}

	registerWidget(type: string, definition: WidgetDefinition): void {
		this.registry.set(type, definition);
	}

	getWidget(type: string): WidgetDefinition | undefined {
		return this.registry.get(type);
	}
}

export const registry = new WidgetRegistry();
