import { useEffect, useState } from "react";
import "./App.css";
import "./components/widgets";
import DashboardViewer from "./components/dashboard/DashboardViewer";
import { useConnectionStore } from "./stores/connection";

function App() {
	const connect = useConnectionStore((state) => state.connect);

	useEffect(() => {
		connect("ws://localhost:8001");
	}, [connect]);

	return (
		<div className="h-screen w-screen">
			<DashboardViewer />
		</div>
	);
}

export default App;
