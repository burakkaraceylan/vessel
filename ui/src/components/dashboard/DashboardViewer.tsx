import discordDashboard from "@/assets/discord-dashboard.json";
import { useDashboard } from "@/hooks/useDashboard";
import type { Dashboard } from "@/types/dashboard";
import WidgetShell from "./WidgetShell";

const DashboardViewer: React.FC = () => {
	const exampleDashboard = discordDashboard as Dashboard;
	const { dashboard } = useDashboard(exampleDashboard);

	const occupiedCells = new Set<number>();
	for (const widget of dashboard.widgets) {
		for (
			let r = widget.position.row;
			r < widget.position.row + widget.size.h;
			r++
		) {
			for (
				let c = widget.position.col;
				c < widget.position.col + widget.size.w;
				c++
			) {
				occupiedCells.add(r * dashboard.columns + c);
			}
		}
	}

	return (
		<div
			style={{
				width: "100%",
				height: "100%",
				display: "flex",
				alignItems: "center",
				justifyContent: "center",
			}}
		>
			<div
				style={{
					display: "grid",
					gridTemplateColumns: `repeat(${dashboard.columns}, 1fr)`,
					gridTemplateRows: `repeat(${dashboard.rows}, 1fr)`,
					gap: "var(--gap)",
					aspectRatio: `${dashboard.columns} / ${dashboard.rows}`,
					width: `min(100%, calc(100vh * ${dashboard.columns} / ${dashboard.rows}))`,
				}}
			>
				{Array.from({ length: dashboard.columns * dashboard.rows }).map(
					(_, i) => {
						if (occupiedCells.has(i)) return null;
						const row = Math.floor(i / dashboard.columns);
						const col = i % dashboard.columns;
						return (
							<div
								key={`cell-${
									// biome-ignore lint/suspicious/noArrayIndexKey: <explanation>
									i
								}`}
								style={{
									gridColumn: `${col + 1} / span 1`,
									gridRow: `${row + 1} / span 1`,
									border: "1px solid var(--border-color)",
									borderRadius: "var(--widget-radius)",
									backgroundColor: "var(--bg-secondary)",
								}}
							/>
						);
					},
				)}

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
		</div>
	);
};

export default DashboardViewer;
