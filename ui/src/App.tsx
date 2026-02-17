import { useEffect, useState } from "react";
import "./App.css";
import "./components/widgets";
import WidgetShell from "./components/dashboard/WidgetShell";
import { useConnectionStore } from "./stores/connection";

function App() {
	const [count, setCount] = useState(0);
	const connect = useConnectionStore((state) => state.connect);

	useEffect(() => {
		connect("ws://localhost:8001");
	}, [connect]);

	return (
		<>
			<WidgetShell
				instance={{
					id: "widget1",
					type: "button",
					position: { col: 0, row: 0 },
					size: { w: 200, h: 100 },
					config: {
						icon: "square",
						label: "Button",
						backgroundColor: "black",
						action: {
							module: "discord",
							action: "set_mute",
							params: {
								mute: "$toggle",
							},
						},
						valueBinding: {
							module: "discord",
							event: "voice_settings_update",
							key: "mute",
						},
					},
				}}
			/>
			<p className="read-the-docs">
				Click on the Vite and React logos to learn more
			</p>
		</>
	);
}

export default App;
