// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { closeWindow, isDesktopApp, minimizeWindow } from "./desktopWindow";

describe("desktopWindow", () => {
	beforeEach(() => {
		vi.restoreAllMocks();
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	afterEach(() => {
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	it("detects non-desktop environment correctly", () => {
		expect(isDesktopApp()).toBe(false);
	});

	it("detects desktop environment when __TAURI_INTERNALS__ is present", () => {
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: vi.fn(),
		};
		expect(isDesktopApp()).toBe(true);
	});

	it("detects desktop environment when __TAURI__ is present", () => {
		(window as unknown as { __TAURI__?: unknown }).__TAURI__ = {};
		expect(isDesktopApp()).toBe(true);
	});

	it("calls plugin:window|minimize when minimizing window in desktop app", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		await minimizeWindow();
		expect(invokeMock).toHaveBeenCalledWith("plugin:window|minimize");
	});

	it("calls plugin:window|close when closing window in desktop app", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		await closeWindow();
		expect(invokeMock).toHaveBeenCalledWith("plugin:window|close");
	});

	it("does nothing in browser environment when minimize or close is invoked", async () => {
		await expect(minimizeWindow()).resolves.toBeUndefined();
		await expect(closeWindow()).resolves.toBeUndefined();
	});
});
