import type { WidgetInstance } from "../../types/widget";
import { registry } from "../../lib/registry";

const WidgetShell: React.FC<{ instance: WidgetInstance }> = ({ instance }) => {
	const definition = registry.getWidget(instance.type);

	if (!definition) {
		return <div>Unknown widget type: {instance.type}</div>;
	}

	const Widget = definition.component;

	return (
		<Widget
			config={instance.config}
			state={{}}
			sendAction={() => {}}
			size={{ width: instance.size.w, height: instance.size.h }}
		/>
	);
};

export default WidgetShell;
