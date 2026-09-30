// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
	clearClientLogs,
	getClientConfig,
	getClientLogs,
	getClientProfiles,
	getClientStatus,
	getAuthSession,
	resetClientStats,
	saveClientConfig,
	saveClientProfiles,
	startClient,
	stopClient,
} from "./managementApi";
import type { PanelConnection } from "./panelConnection";

describe("managementApi", () => {
	const dummyConnection: PanelConnection = {
		baseUrl: "https://remote-server.com",
		token: "test-token",
	};

	beforeEach(() => {
		vi.restoreAllMocks();
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	afterEach(() => {
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	describe("Client operations (Native Tauri IPC only)", () => {
		it("getClientStatus invokes client_status command directly without HTTP connection", async () => {
			const invokeMock = vi.fn().mockResolvedValue({ running: true, state: "connected" });
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			const status = await getClientStatus();
			expect(invokeMock).toHaveBeenCalledWith("client_status", undefined);
			expect(status).toEqual({ running: true, state: "connected" });
		});

		it("startClient invokes client_start with payload", async () => {
			const invokeMock = vi.fn().mockResolvedValue(undefined);
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			const payload = {
				server_addr: "127.0.0.1:7000",
				transport: "quic",
			};
			const res = await startClient(payload);
			expect(invokeMock).toHaveBeenCalledWith("client_start", { payload });
			expect(res).toEqual({ ok: true });
		});

		it("stopClient invokes client_stop", async () => {
			const invokeMock = vi.fn().mockResolvedValue(undefined);
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			const res = await stopClient();
			expect(invokeMock).toHaveBeenCalledWith("client_stop", undefined);
			expect(res).toEqual({ ok: true });
		});

		it("getClientConfig invokes client_get_config", async () => {
			const invokeMock = vi.fn().mockResolvedValue({
				active_profile_id: "p1",
				profiles: [],
			});
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			const cfg = await getClientConfig();
			expect(invokeMock).toHaveBeenCalledWith("client_get_config", undefined);
			expect(cfg.active_profile_id).toBe("p1");
		});

		it("saveClientConfig invokes client_save_config", async () => {
			const invokeMock = vi.fn().mockResolvedValue(undefined);
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			const payload = { active_profile_id: "p2" };
			const res = await saveClientConfig(payload);
			expect(invokeMock).toHaveBeenCalledWith("client_save_config", { payload });
			expect(res).toEqual({ ok: true });
		});

		it("resetClientStats invokes client_reset_stats", async () => {
			const invokeMock = vi.fn().mockResolvedValue(undefined);
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			const res = await resetClientStats();
			expect(invokeMock).toHaveBeenCalledWith("client_reset_stats", undefined);
			expect(res).toEqual({ ok: true });
		});

		it("getClientProfiles and saveClientProfiles invoke corresponding commands", async () => {
			const invokeMock = vi.fn().mockImplementation((cmd: string) => {
				if (cmd === "client_get_profiles") return Promise.resolve([{ id: "1", name: "test" }]);
				if (cmd === "client_save_profiles") return Promise.resolve(undefined);
				return Promise.resolve(null);
			});
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			const profiles = await getClientProfiles();
			expect(invokeMock).toHaveBeenCalledWith("client_get_profiles", undefined);
			expect(profiles).toHaveLength(1);

			const res = await saveClientProfiles(profiles);
			expect(invokeMock).toHaveBeenCalledWith("client_save_profiles", { profiles });
			expect(res).toEqual({ ok: true });
		});

		it("getClientLogs and clearClientLogs invoke corresponding commands", async () => {
			const invokeMock = vi.fn().mockImplementation((cmd: string) => {
				if (cmd === "client_logs") return Promise.resolve([{ message: "hello" }]);
				if (cmd === "client_clear_logs") return Promise.resolve(undefined);
				return Promise.resolve(null);
			});
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			const logs = await getClientLogs(100);
			expect(invokeMock).toHaveBeenCalledWith("client_logs", { limit: 100 });
			expect(logs).toHaveLength(1);

			const res = await clearClientLogs();
			expect(invokeMock).toHaveBeenCalledWith("client_clear_logs", undefined);
			expect(res).toEqual({ ok: true });
		});

		it("fails when Tauri is not available (no HTTP fallback)", async () => {
			await expect(getClientStatus()).rejects.toThrow("Tauri invoke is not available");
		});

		it("propagates error when Tauri invoke rejects", async () => {
			const invokeMock = vi.fn().mockRejectedValue(new Error("Command failed"));
			(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
				invoke: invokeMock,
			};

			await expect(getClientStatus()).rejects.toThrow("Command failed");
		});
	});

	describe("Remote Management (HTTP RESTful)", () => {
		it("getAuthSession makes HTTP request to remote endpoint", async () => {
			const fetchMock = vi.fn().mockResolvedValue({
				ok: true,
				status: 200,
				text: () =>
					Promise.resolve(
						JSON.stringify({
							authenticated: true,
							username: "admin",
							is_admin: true,
						}),
					),
			});
			vi.stubGlobal("fetch", fetchMock);

			const session = await getAuthSession(dummyConnection);
			expect(fetchMock).toHaveBeenCalledWith(
				"https://remote-server.com/auth/session",
				expect.objectContaining({
					headers: expect.objectContaining({
						"Content-Type": "application/json",
						Authorization: "Bearer test-token",
					}),
				}),
			);
			expect(session.username).toBe("admin");
		});
	});
});
