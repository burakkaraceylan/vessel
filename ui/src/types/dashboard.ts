import type { WidgetInstance } from "./widget";

export interface Dashboard {
	id: string;
	name: string;
	columns: number;
	rows: number;
	widgets?: WidgetInstance[];
	zones?: Zone[];
	theme?: string;
}

export interface ZoneProfile {
	condition: string;
	default: boolean;
	widgets?: WidgetInstance[];
}

export interface Zone {
	position: { col: number; row: number };
	size: { w: number; h: number };
	profiles?: ZoneProfile[];
}
