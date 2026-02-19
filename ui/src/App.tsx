import { useEffect, useState } from "react";
import "./App.css";
import "./components/widgets";
import DashboardViewer from "./components/dashboard/DashboardViewer";
import { useConnectionStore } from "./stores/connection";
import { useThemeStore } from "./stores/theme";

function App() {
	const connect = useConnectionStore((state) => state.connect);
	const applyTheme = useThemeStore((state) => state.applyTheme);

	useEffect(() => {
		connect("ws://192.168.1.122:8001/ws");
	}, [connect]);

	useEffect(() => {
		applyTheme(useThemeStore.getState().currentTheme);
	}, [applyTheme]);

	return (
		<div
			className="h-full w-full"
			style={{
				backgroundColor: "var(--bg-primary)",
				color: "var(--text-primary)",
			}}
		>
			<DashboardViewer />
		</div>
	);
}

export default App;
