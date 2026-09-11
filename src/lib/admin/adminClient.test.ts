// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AdminApiError, adminRequest, httpToAdminRpc } from "./adminClient";
import type { PanelConnection } from "@/lib/panelConnection";

describe("adminClient (adminRequest & AdminApiError)", () => {
	const dummyConnection: PanelConnection = {
		baseUrl: "https://remote-server.com",
		token: "test-token",
		kind: "bearer",
	};

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

	it("sends Bearer token header for bearer connection", async () => {
		const fetchMock = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			text: async () => JSON.stringify({ ok: true }),
		});
		vi.stubGlobal("fetch", fetchMock);

		const result = await adminRequest<{ ok: boolean }>(dummyConnection, "/health");
		expect(fetchMock).toHaveBeenCalledWith(
			"https://remote-server.com/health",
			expect.objectContaining({
				headers: expect.objectContaining({
					Authorization: "Bearer test-token",
				}),
			}),
		);
		expect(result).toEqual({ ok: true });
	});

	it("sends X-Prism-Desktop-Token header for desktop-token connection", async () => {
		const desktopConn: PanelConnection = {
			baseUrl: "https://remote-server.com",
			token: "secret-desktop-tok",
			kind: "desktop-token",
		};
		const fetchMock = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			text: async () => JSON.stringify({ status: "active" }),
		});
		vi.stubGlobal("fetch", fetchMock);

		const result = await adminRequest<{ status: string }>(desktopConn, "/status");
		expect(fetchMock).toHaveBeenCalledWith(
			"https://remote-server.com/status",
			expect.objectContaining({
				headers: expect.objectContaining({
					"X-Prism-Desktop-Token": "secret-desktop-tok",
				}),
			}),
		);
		expect(result).toEqual({ status: "active" });
	});

	it("throws AdminApiError on non-ok response with parsed message", async () => {
		const fetchMock = vi.fn().mockResolvedValue({
			ok: false,
			status: 403,
			text: async () => JSON.stringify({ error: "Unauthorized access" }),
		});
		vi.stubGlobal("fetch", fetchMock);

		await expect(adminRequest(dummyConnection, "/admin")).rejects.toThrow(AdminApiError);
	});

	it("invokes the admin_rpc Tauri command for in-band $admin on desktop", async () => {
		const invokeMock = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			body: { token: "prism_cl_x" },
		});
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};
		const fetchMock = vi.fn();
		vi.stubGlobal("fetch", fetchMock);

		const result = await adminRequest<{ token: string }>(
			{ baseUrl: "", token: "", kind: "tunnel-admin" },
			"/auth/github/exchange",
			{ method: "POST", body: JSON.stringify({ code: "abc" }) },
		);

		expect(fetchMock).not.toHaveBeenCalled();
		expect(invokeMock).toHaveBeenCalledWith(
			"admin_rpc",
			expect.objectContaining({
				payload: expect.objectContaining({
					method: "auth.github.exchange",
					payload: { code: "abc" },
				}),
			}),
		);
		expect(result).toEqual({ token: "prism_cl_x" });
	});

	it("maps leftover HTTP paths onto $admin RPC methods", () => {
		expect(httpToAdminRpc("/health", "GET")).toEqual({ method: "health", payload: {} });
		expect(httpToAdminRpc("/conns", "GET")).toEqual({
			method: "connections",
			payload: {},
		});
		expect(httpToAdminRpc("/auth/tokens/tok-1", "DELETE")).toEqual({
			method: "auth.tokens.revoke",
			payload: { token_id: "tok-1" },
		});
		expect(
			httpToAdminRpc(
				"/managed/nodes/n1/config",
				"PUT",
				JSON.stringify({ desired_config: { listeners: [] } }),
			),
		).toEqual({
			method: "managed.node.config.put",
			payload: { node_id: "n1", desired_config: { listeners: [] } },
		});
	});
});
