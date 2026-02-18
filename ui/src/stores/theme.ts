import { create } from "zustand";
import { defaultDarkTheme } from "@/lib/themes";
import type { Theme } from "@/types/theme";

interface ThemeStore {
	currentTheme: Theme;
	setTheme: (theme: Theme) => void;
	applyTheme: (theme: Theme) => void;
}

export const useThemeStore = create<ThemeStore>((set) => ({
	currentTheme: defaultDarkTheme,
	setTheme: (theme) => {
		set({ currentTheme: theme });
		applyToDOM(theme);
	},
	applyTheme: (theme) => {
		applyToDOM(theme);
	},
}));

function applyToDOM(theme: Theme) {
	for (const [key, value] of Object.entries(theme.variables)) {
		document.documentElement.style.setProperty(key, value);
	}
}
