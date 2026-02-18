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
		connect("ws://localhost:8001");
	}, [connect]);

	useEffect(() => {
		applyTheme(useThemeStore.getState().currentTheme);
	}, [applyTheme]);

	return (
		<div
			className="h-screen w-screen"
			style={{
				backgroundColor: "var(--bg-primary)",
				color: "var(--text-primary)",
				padding: "var(--gap)",
			}}
		>
			<DashboardViewer />
		</div>
	);
}

export default App;
