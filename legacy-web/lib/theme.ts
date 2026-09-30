import { useEffect, useState } from "react";

export type Theme = "system" | "light" | "dark";

const THEME_STORAGE_KEY = "prism_theme";

export function getStoredTheme(): Theme {
	if (typeof window === "undefined") return "system";
	try {
		const val = window.localStorage.getItem(THEME_STORAGE_KEY);
		if (val === "light" || val === "dark" || val === "system") {
			return val;
		}
	} catch {
		// Ignore local storage read errors
	}
	return "system";
}

export function applyTheme(theme: Theme): boolean {
	if (typeof window === "undefined") return false;
	const isDark =
		theme === "dark" ||
		(theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);

	if (isDark) {
		document.documentElement.classList.add("dark");
	} else {
		document.documentElement.classList.remove("dark");
	}

	return isDark;
}

export function setTheme(theme: Theme): void {
	if (typeof window === "undefined") return;
	try {
		window.localStorage.setItem(THEME_STORAGE_KEY, theme);
	} catch {
		// Ignore local storage write errors
	}
	applyTheme(theme);
	window.dispatchEvent(new CustomEvent("prism:theme-change", { detail: theme }));
}

export function useTheme(): {
	theme: Theme;
	setTheme: (theme: Theme) => void;
	isDark: boolean;
} {
	const [theme, setLocalTheme] = useState<Theme>(() => getStoredTheme());
	const [isDark, setIsDark] = useState<boolean>(() => {
		if (typeof window === "undefined") return true;
		const current = getStoredTheme();
		return (
			current === "dark" ||
			(current === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches)
		);
	});

	useEffect(() => {
		const sync = () => {
			const current = getStoredTheme();
			setLocalTheme(current);
			const dark = applyTheme(current);
			setIsDark(dark);
		};

		// Initial sync
		sync();

		const handleStorage = (e: StorageEvent) => {
			if (e.key === THEME_STORAGE_KEY) {
				sync();
			}
		};

		const handleCustom = () => sync();

		const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
		const handleMedia = () => {
			if (getStoredTheme() === "system") {
				sync();
			}
		};

		window.addEventListener("storage", handleStorage);
		window.addEventListener("prism:theme-change", handleCustom);
		mediaQuery.addEventListener("change", handleMedia);

		return () => {
			window.removeEventListener("storage", handleStorage);
			window.removeEventListener("prism:theme-change", handleCustom);
			mediaQuery.removeEventListener("change", handleMedia);
		};
	}, []);

	const handleSetTheme = (next: Theme) => {
		setTheme(next);
		setLocalTheme(next);
		setIsDark(applyTheme(next));
	};

	return {
		theme,
		setTheme: handleSetTheme,
		isDark,
	};
}
