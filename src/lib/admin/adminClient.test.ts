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

	it("throws AdminApiError on non-ok response with parsed message", async () => {
		const fetchMock = vi.fn().mockResolvedValue({
			ok: false,
			status: 403,
			text: async () => JSON.stringify({ error: "Unauthorized access" }),
		});
		vi.stubGlobal("fetch", fetchMock);

		await expect(adminRequest(dummyConnection, "/admin")).rejects.toThrow(AdminApiError);
	});

	it("invokes the admin_rpc Tauri command for in-band $admin", async () => {
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

	it("fetches the GitHub login URL over $admin RPC instead of loopback HTTP", async () => {
		const invokeMock = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			body: { url: "https://github.com/login/oauth/authorize?client_id=abc" },
		});
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};
		const fetchMock = vi.fn();
		vi.stubGlobal("fetch", fetchMock);

		const result = await adminRequest<{ url: string }>(
			{ baseUrl: "", token: "", kind: "tunnel-admin" },
			"/auth/github/login",
		);

		expect(fetchMock).not.toHaveBeenCalled();
		expect(invokeMock).toHaveBeenCalledTimes(1);
		expect(invokeMock).toHaveBeenCalledWith(
			"admin_rpc",
			expect.objectContaining({
				payload: expect.objectContaining({
					method: "auth.github.login",
					payload: {},
				}),
			}),
		);
		expect(invokeMock.mock.calls[0]?.[0]).not.toBe("admin_request");
		expect(result.url).toContain("github.com/login/oauth/authorize");
	});

	it("maps leftover HTTP paths onto $admin RPC methods", () => {
		expect(httpToAdminRpc("/health", "GET")).toEqual({ method: "health", payload: {} });
		expect(httpToAdminRpc("/auth/github/login", "GET")).toEqual({
			method: "auth.github.login",
			payload: {},
		});
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
				"/auth/users/u1",
				"PUT",
				JSON.stringify({ role: "admin", service_rules: ["*"] }),
			),
		).toEqual({
			method: "auth.user.put",
			payload: { user_id: "u1", role: "admin", service_rules: ["*"] },
		});
		expect(
			httpToAdminRpc("/auth/github/login?state=2fb337f6-9a76-4c05-8c3c-5f87fa1af10a", "GET"),
		).toEqual({
			method: "auth.github.login",
			payload: { state: "2fb337f6-9a76-4c05-8c3c-5f87fa1af10a" },
		});
	});

	it("fetches the GitHub login URL with state parameter over $admin RPC", async () => {
		const invokeMock = vi.fn().mockResolvedValue({
			ok: true,
			status: 200,
			body: { url: "https://github.com/login/oauth/authorize?client_id=abc&state=test-state" },
		});
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};
		const fetchMock = vi.fn();
		vi.stubGlobal("fetch", fetchMock);

		const result = await adminRequest<{ url: string }>(
			{ baseUrl: "", token: "", kind: "tunnel-admin" },
			"/auth/github/login?state=test-state",
		);

		expect(fetchMock).not.toHaveBeenCalled();
		expect(invokeMock).toHaveBeenCalledTimes(1);
		expect(invokeMock).toHaveBeenCalledWith(
			"admin_rpc",
			expect.objectContaining({
				payload: expect.objectContaining({
					method: "auth.github.login",
					payload: { state: "test-state" },
				}),
			}),
		);
		expect(result.url).toContain("state=test-state");
	});
});
