import { useEffect, useState } from "react";
import { resolveValue } from "@/lib/expressions";
import { useConnectionStore } from "@/stores/connection";
import { useModuleStateStore } from "@/stores/moduleState";
import { registry } from "../../lib/registry";
import type { WidgetInstance } from "../../types/widget";

const WidgetShell: React.FC<{ instance: WidgetInstance }> = ({ instance }) => {
	const definition = registry.getWidget(instance.type);
	const sendAction = useConnectionStore((state) => state.sendAction);
	const moduleState = useModuleStateStore((state) => state.state);
	const [state, setState] = useState<Record<string, unknown>>({});

	const resolve = (value: string) => resolveValue(value, moduleState);

	useEffect(() => {
		const state: Record<string, unknown> = {};
		if (instance.config.valueBinding) {
			const b = instance.config.valueBinding;
			state[b.key] = (
				moduleState[b.module]?.[b.event] as Record<string, unknown> | undefined
			)?.[b.key];
		}
		setState(state);
	}, [moduleState, instance.config.valueBinding]);

	if (!definition) {
		return <div>Unknown widget type: {instance.type}</div>;
	}

	const Widget = definition.component;

	return (
		<Widget
			config={instance.config}
			state={state}
			sendAction={sendAction}
			size={{ width: instance.size.w, height: instance.size.h }}
			resolve={resolve}
		/>
	);
};

export default WidgetShell;
