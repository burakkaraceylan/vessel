import type { WidgetInstance } from "./widget";

export interface Dashboard {
	id: string;
	name: string;
	columns: number;
	rows: number;
	widgets: WidgetInstance[];
}
