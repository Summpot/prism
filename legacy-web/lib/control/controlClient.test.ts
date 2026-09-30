// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
	ControlApiError,
	invokeControlRpc,
	requestControl,
} from "./controlClient";
import type { PanelConnection } from "@/lib/panelConnection";

describe("controlClient (invokeControlRpc & requestControl)", () => {
	beforeEach(() => {
		vi.restoreAllMocks();
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	afterEach(() => {
		vi.restoreAllMocks();
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	it("directly invokes control_rpc Tauri command without URL parsing", async () => {
		const invokeMock = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			body: { count: 42 },
		});
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const result = await invokeControlRpc<{ count: number }>("connections", {}, "my-token");
		expect(invokeMock).toHaveBeenCalledWith("control_rpc", {
			payload: {
				method: "connections",
				payload: {},
				token: "my-token",
			},
		});
		expect(result).toEqual({ count: 42 });
	});

	it("throws ControlApiError on unsuccessful RPC response", async () => {
		const invokeMock = vi.fn().mockResolvedValue({
			ok: false,
			status: 401,
			message: "Unauthorized token",
		});
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		await expect(invokeControlRpc("health")).rejects.toThrow(ControlApiError);
	});

	it("routes tunnel-control connection through in-band RPC via requestControl", async () => {
		const invokeMock = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			body: [{ id: "sess-1" }],
		});
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const conn: PanelConnection = { baseUrl: "", token: "tok-1", kind: "tunnel-control" };
		const result = await requestControl<{ id: string }[]>(conn, {
			rpc: { method: "connections" },
			http: { path: "/conns" },
		});

		expect(invokeMock).toHaveBeenCalledWith("control_rpc", {
			payload: {
				method: "connections",
				payload: {},
				token: "tok-1",
			},
		});
		expect(result).toEqual([{ id: "sess-1" }]);
	});

	it("routes external baseUrl connection through HTTP via requestControl", async () => {
		const fetchMock = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			text: async () => JSON.stringify({ ok: true }),
		});
		vi.stubGlobal("fetch", fetchMock);

		const conn: PanelConnection = { baseUrl: "https://my-server.com", token: "tok-2", kind: "bearer" };
		const result = await requestControl<{ ok: boolean }>(conn, {
			rpc: { method: "health" },
			http: { path: "/health", method: "GET" },
		});

		expect(fetchMock).toHaveBeenCalledWith(
			"https://my-server.com/health",
			expect.objectContaining({
				method: "GET",
				headers: expect.objectContaining({
					Authorization: "Bearer tok-2",
				}),
			}),
		);
		expect(result).toEqual({ ok: true });
	});
});
