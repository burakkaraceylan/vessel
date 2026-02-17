import { useMemo } from "react";
import type { Dashboard } from "@/types/dashboard";
import type { WidgetInstance } from "@/types/widget";

export interface ValidationError {
	widgetId: string;
	reason: string;
}

export interface ValidatedDashboard {
	dashboard: Dashboard;
	errors: ValidationError[];
}

function validateDashboard(dashboard: Dashboard): ValidatedDashboard {
	const { columns, rows } = dashboard;
	const occupied = new Set<string>();
	const validWidgets: WidgetInstance[] = [];
	const errors: ValidationError[] = [];

	for (const widget of dashboard.widgets) {
		const { col, row } = widget.position;
		const { w, h } = widget.size;

		if (w <= 0 || h <= 0) {
			errors.push({ widgetId: widget.id, reason: `Invalid size ${w}x${h}` });
			continue;
		}

		if (col < 0 || row < 0 || col + w > columns || row + h > rows) {
			errors.push({
				widgetId: widget.id,
				reason: `Out of bounds: position (${col},${row}) size ${w}x${h} exceeds ${columns}x${rows} grid`,
			});
			continue;
		}

		const cells: string[] = [];
		let overlap = false;

		for (let r = row; r < row + h; r++) {
			for (let c = col; c < col + w; c++) {
				const key = `${c},${r}`;
				if (occupied.has(key)) {
					errors.push({
						widgetId: widget.id,
						reason: `Overlaps another widget at cell (${c},${r})`,
					});
					overlap = true;
					break;
				}
				cells.push(key);
			}
			if (overlap) break;
		}

		if (overlap) continue;

		for (const key of cells) {
			occupied.add(key);
		}
		validWidgets.push(widget);
	}

	return { dashboard: { ...dashboard, widgets: validWidgets }, errors };
}

export function useDashboard(dashboard: Dashboard): ValidatedDashboard {
	return useMemo(() => validateDashboard(dashboard), [dashboard]);
}
