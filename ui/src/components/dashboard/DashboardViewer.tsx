import discordDashboard from "@/assets/discord-dashboard.json";
import { useDashboard } from "@/hooks/useDashboard";
import type { Dashboard } from "@/types/dashboard";
import WidgetShell from "./WidgetShell";

const DashboardViewer: React.FC = () => {
	const exampleDashboard = discordDashboard as Dashboard;
	const { dashboard } = useDashboard(exampleDashboard);

	return (
		<div
			style={{
				display: "grid",
				gridTemplateColumns: `repeat(${dashboard.columns}, 1fr)`,
				gridTemplateRows: `repeat(${dashboard.rows}, 1fr)`,
				gap: "10px",
				width: "100%",
				height: "100%",
			}}
		>
			{Array.from({
				length: dashboard.columns * dashboard.rows,
			}).map((_, i) => (
				<div
					key={`cell-${
						// biome-ignore lint/suspicious/noArrayIndexKey: <explanation>
						i
					}`}
					className="border border-black"
				/>
			))}

			{dashboard.widgets.map((widget) => (
				<div
					key={widget.id}
					style={{
						gridColumn: `${widget.position.col + 1} / span ${widget.size.w}`,
						gridRow: `${widget.position.row + 1} / span ${widget.size.h}`,
						border: "1px solid #ccc",
					}}
				>
					<WidgetShell instance={widget} />
				</div>
			))}
		</div>
	);
};

export default DashboardViewer;
