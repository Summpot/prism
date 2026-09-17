// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

import { applyTheme, getStoredTheme, setTheme } from "./theme";

describe("theme module", () => {
	beforeEach(() => {
		window.localStorage.clear();
		document.documentElement.className = "";
		vi.restoreAllMocks();
	});

	it("defaults to system theme when localStorage is empty", () => {
		expect(getStoredTheme()).toBe("system");
	});

	it("applies dark class when theme is dark", () => {
		const isDark = applyTheme("dark");
		expect(isDark).toBe(true);
		expect(document.documentElement.classList.contains("dark")).toBe(true);
	});

	it("removes dark class when theme is light", () => {
		document.documentElement.classList.add("dark");
		const isDark = applyTheme("light");
		expect(isDark).toBe(false);
		expect(document.documentElement.classList.contains("dark")).toBe(false);
	});

	it("persists theme to localStorage and updates DOM via setTheme", () => {
		setTheme("light");
		expect(window.localStorage.getItem("prism_theme")).toBe("light");
		expect(document.documentElement.classList.contains("dark")).toBe(false);

		setTheme("dark");
		expect(window.localStorage.getItem("prism_theme")).toBe("dark");
		expect(document.documentElement.classList.contains("dark")).toBe(true);
	});
});
