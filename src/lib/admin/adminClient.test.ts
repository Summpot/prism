// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AdminApiError, adminRequest } from "./adminClient";
import type { PanelConnection } from "@/lib/panelConnection";

describe("adminClient (adminRequest & AdminApiError)", () => {
	const dummyConnection: PanelConnection = {
		baseUrl: "https://remote-server.com",
		token: "test-token",
		kind: "bearer",
	};

	beforeEach(() => {
		vi.restoreAllMocks();
	});

	afterEach(() => {
		vi.restoreAllMocks();
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
});
