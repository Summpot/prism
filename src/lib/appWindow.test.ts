// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
	closeWindow,
	invokeTauri,
	isTauriContext,
	isWindowMaximized,
	minimizeWindow,
	openExternalUrl,
	toggleMaximizeWindow,
} from "./appWindow";

describe("appWindow", () => {
	beforeEach(() => {
		vi.restoreAllMocks();
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	afterEach(() => {
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	it("detects non-Tauri environment correctly", () => {
		expect(isTauriContext()).toBe(false);
	});

	it("detects Tauri environment when __TAURI_INTERNALS__ is present", () => {
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: vi.fn(),
		};
		expect(isTauriContext()).toBe(true);
	});

	it("detects Tauri environment when __TAURI__ is present", () => {
		(window as unknown as { __TAURI__?: unknown }).__TAURI__ = {};
		expect(isTauriContext()).toBe(true);
	});

	it("calls plugin:window|minimize when minimizing window", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		await minimizeWindow();
		expect(invokeMock).toHaveBeenCalledWith("plugin:window|minimize");
	});

	it("calls plugin:window|toggle_maximize when toggling maximize", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		await toggleMaximizeWindow();
		expect(invokeMock).toHaveBeenCalledWith("plugin:window|toggle_maximize");
	});

	it("calls plugin:window|is_maximized and returns boolean", async () => {
		const invokeMock = vi.fn().mockResolvedValue(true);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const res = await isWindowMaximized();
		expect(invokeMock).toHaveBeenCalledWith("plugin:window|is_maximized");
		expect(res).toBe(true);
	});

	it("returns false from isWindowMaximized when not in Tauri environment", async () => {
		const res = await isWindowMaximized();
		expect(res).toBe(false);
	});

	it("calls plugin:window|close when closing window", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		await closeWindow();
		expect(invokeMock).toHaveBeenCalledWith("plugin:window|close");
	});

	it("handles non-Tauri environment gracefully when minimize, maximize, or close is invoked", async () => {
		await expect(minimizeWindow()).resolves.toBeUndefined();
		await expect(toggleMaximizeWindow()).resolves.toBeUndefined();
		await expect(closeWindow()).resolves.toBeUndefined();
	});

	it("calls open_external_url via Tauri invoke in Tauri environment", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		await openExternalUrl("https://github.com/login/oauth/authorize");
		expect(invokeMock).toHaveBeenCalledWith("open_external_url", {
			url: "https://github.com/login/oauth/authorize",
		});
	});

	it("falls back to window.open when Tauri invoke is unavailable", async () => {
		const openSpy = vi.spyOn(window, "open").mockImplementation(() => null);
		await openExternalUrl("https://github.com/login/oauth/authorize");
		expect(openSpy).toHaveBeenCalledWith(
			"https://github.com/login/oauth/authorize",
			"_blank",
			"noopener,noreferrer",
		);
	});

	it("calls invokeTauri successfully in Tauri environment", async () => {
		const invokeMock = vi.fn().mockResolvedValue({ ok: true });
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const res = await invokeTauri<{ ok: boolean }>("client_status");
		expect(invokeMock).toHaveBeenCalledWith("client_status", undefined);
		expect(res).toEqual({ ok: true });
	});

	it("throws error when invokeTauri is called outside Tauri environment", async () => {
		await expect(invokeTauri("client_status")).rejects.toThrow("Tauri invoke is not available");
	});
});
