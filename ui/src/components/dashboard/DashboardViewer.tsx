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
				gap: "var(--gap)",
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
					style={{
							border: "1px solid var(--border-color)",
							borderRadius: "var(--widget-radius)",
							backgroundColor: "var(--bg-secondary)",
						}}
				/>
			))}

			{dashboard.widgets.map((widget) => (
				<div
					key={widget.id}
					style={{
						gridColumn: `${widget.position.col + 1} / span ${widget.size.w}`,
						gridRow: `${widget.position.row + 1} / span ${widget.size.h}`,
						borderRadius: "var(--widget-radius)",
						overflow: "hidden",
					}}
				>
					<WidgetShell instance={widget} />
				</div>
			))}
		</div>
	);
};

export default DashboardViewer;
