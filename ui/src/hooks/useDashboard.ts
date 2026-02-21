import { useMemo } from "react";
import type { Dashboard, Zone, ZoneProfile } from "@/types/dashboard";
import type { WidgetInstance } from "@/types/widget";

// Identifies where a validation error originated.
// Uses a path-style string so errors can point to nested locations:
//   "dashboard.widget[abc]"         — top-level widget
//   "zone[0]"                       — zone rectangle itself
//   "zone[0].profile[1].widget[xyz]" — widget inside a zone profile
export interface ValidationError {
	source: string;
	reason: string;
}

export interface ValidatedDashboard {
	dashboard: Dashboard;
	errors: ValidationError[];
}

// Validates a flat list of widgets against a grid of size (columns × rows).
//
// occupied:     shared set of "col,row" keys already taken by previously
//               validated widgets or zones. Mutated in place — valid widgets
//               add their cells so subsequent widgets can detect conflicts.
// sourcePrefix: prepended to each error's source path, e.g. "dashboard" or
//               "zone[0].profile[1]".
//
// Returns only the widgets that passed all checks. Invalid widgets are dropped
// and their errors appended to the errors array.
function validateWidgets(
	widgets: WidgetInstance[],
	columns: number,
	rows: number,
	occupied: Set<string>,
	sourcePrefix: string,
	errors: ValidationError[],
): WidgetInstance[] {
	const valid: WidgetInstance[] = [];

	for (const widget of widgets) {
		const { col, row } = widget.position;
		const { w, h } = widget.size;
		const source = `${sourcePrefix}.widget[${widget.id}]`;

		if (w <= 0 || h <= 0) {
			errors.push({ source, reason: `Invalid size ${w}x${h}` });
			continue;
		}

		if (col < 0 || row < 0 || col + w > columns || row + h > rows) {
			errors.push({
				source,
				reason: `Out of bounds: position (${col},${row}) size ${w}x${h} exceeds ${columns}x${rows} grid`,
			});
			continue;
		}

		// Collect all cells this widget would occupy before committing any,
		// so a partial overlap doesn't leave the occupied set in a dirty state.
		const cells: string[] = [];
		let overlap = false;

		for (let r = row; r < row + h; r++) {
			for (let c = col; c < col + w; c++) {
				const key = `${c},${r}`;
				if (occupied.has(key)) {
					errors.push({ source, reason: `Overlaps at cell (${c},${r})` });
					overlap = true;
					break;
				}
				cells.push(key);
			}
			if (overlap) break;
		}

		if (overlap) continue;

		for (const key of cells) occupied.add(key);
		valid.push(widget);
	}

	return valid;
}

// Two-level validation:
//
// Level 1 — top-level grid:
//   Widgets and zones share a single occupied set, so they are checked against
//   each other. A zone is treated as an opaque rectangle at this level — its
//   internal contents are not visible to the top-level checker.
//
// Level 2 — zone internals:
//   Each profile is validated independently against the zone's own dimensions.
//   Profiles never share an occupied set because only one profile is active at
//   a time, so there is no cross-profile overlap constraint.
function validateDashboard(dashboard: Dashboard): ValidatedDashboard {
	const { columns, rows } = dashboard;
	// Shared occupied set for the top-level grid (widgets + zone footprints).
	const occupied = new Set<string>();
	const errors: ValidationError[] = [];
	let validWidgets: WidgetInstance[] = [];

	if (dashboard.widgets) {
		validWidgets = validateWidgets(
			dashboard.widgets,
			columns,
			rows,
			occupied,
			"dashboard",
			errors,
		);
	}

	const validZones: Zone[] = [];
	if (dashboard.zones) {
		for (let zi = 0; zi < dashboard.zones.length; zi++) {
			const zone = dashboard.zones[zi];
			const { col, row } = zone.position;
			const { w, h } = zone.size;
			const zoneSource = `zone[${zi}]`;

			if (w <= 0 || h <= 0) {
				errors.push({ source: zoneSource, reason: `Invalid size ${w}x${h}` });
				continue;
			}

			if (col < 0 || row < 0 || col + w > columns || row + h > rows) {
				errors.push({
					source: zoneSource,
					reason: `Out of bounds: position (${col},${row}) size ${w}x${h} exceeds ${columns}x${rows} grid`,
				});
				continue;
			}

			// Check the zone's footprint against top-level occupied cells (same logic
			// as widget overlap detection). Collect cells before committing.
			const cells: string[] = [];
			let overlap = false;

			for (let r = row; r < row + h; r++) {
				for (let c = col; c < col + w; c++) {
					const key = `${c},${r}`;
					if (occupied.has(key)) {
						errors.push({
							source: zoneSource,
							reason: `Overlaps existing widget or zone at cell (${c},${r})`,
						});
						overlap = true;
						break;
					}
					cells.push(key);
				}
				if (overlap) break;
			}

			if (overlap) continue;

			for (const key of cells) occupied.add(key);

			// Validate each profile's widgets independently.
			// Zone-relative coordinates: (0,0) is the top-left of the zone,
			// bounds are (w × h) instead of the dashboard's (columns × rows).
			const validProfiles: ZoneProfile[] = [];

			if (zone.profiles) {
				for (let pi = 0; pi < zone.profiles.length; pi++) {
					const profile = zone.profiles[pi];
					const profileOccupied = new Set<string>();
					let validProfileWidgets: WidgetInstance[] = [];

					if (profile.widgets) {
						validProfileWidgets = validateWidgets(
							profile.widgets,
							w,
							h,
							profileOccupied,
							`${zoneSource}.profile[${pi}]`,
							errors,
						);
					}
					validProfiles.push({ ...profile, widgets: validProfileWidgets });
				}
			}

			validZones.push({ ...zone, profiles: validProfiles });
		}
	}

	return {
		dashboard: { ...dashboard, widgets: validWidgets, zones: validZones },
		errors,
	};
}

export function useDashboard(dashboard: Dashboard): ValidatedDashboard {
	return useMemo(() => validateDashboard(dashboard), [dashboard]);
}
