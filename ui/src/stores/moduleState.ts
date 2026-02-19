import { create } from "zustand";

interface ModuleStateStore {
	state: Record<string, Record<string, unknown>>;

	handleEvent: (
		module: string,
		event: string,
		data: Record<string, unknown>,
	) => void;
}

export const useModuleStateStore = create<ModuleStateStore>((set) => ({
	state: {},
	handleEvent: (module, event, data) => {
		console.log(`Handling event for module ${module}:`, event, data);
		set((prev) => ({
			state: {
				...prev.state,
				[module]: {
					...prev.state[module],
					[event]: data,
				},
			},
		}));
	},
}));
