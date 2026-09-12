// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
	clearClientLogs,
	getClientConfig,
	getClientLogs,
	getClientProfiles,
	getClientStatus,
	listLocalMiddlewares,
	resetClientStats,
	resetLocalMiddlewareConfig,
	saveClientConfig,
	saveClientProfiles,
	startClient,
	stopClient,
	updateLocalMiddlewareConfig,
} from "./clientIpc";

describe("clientIpc (Native Tauri IPC)", () => {
	beforeEach(() => {
		vi.restoreAllMocks();
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	afterEach(() => {
		delete (window as unknown as { __TAURI__?: unknown }).__TAURI__;
		delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
	});

	it("getClientStatus invokes client_status command", async () => {
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

	it("getClientProfiles invokes client_get_profiles", async () => {
		const mockProfiles = [
			{ id: "p1", name: "Profile 1", server_addr: "1.1.1.1:7000", transport: "quic" },
		];
		const invokeMock = vi.fn().mockResolvedValue(mockProfiles);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const profiles = await getClientProfiles();
		expect(invokeMock).toHaveBeenCalledWith("client_get_profiles", undefined);
		expect(profiles).toEqual(mockProfiles);
	});

	it("saveClientProfiles invokes client_save_profiles", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const profiles = [
			{
				id: "p1",
				name: "P1",
				server_addr: "1.1.1.1",
				transport: "quic",
				auth_token: "",
				listen_addr: "",
				fake_lan_broadcast: false,
			},
		];
		const res = await saveClientProfiles(profiles);
		expect(invokeMock).toHaveBeenCalledWith("client_save_profiles", { profiles });
		expect(res).toEqual({ ok: true });
	});

	it("getClientConfig invokes client_get_config", async () => {
		const mockConfig = {
			active_profile_id: "p1",
			active_config: { profile_name: "Default" },
		};
		const invokeMock = vi.fn().mockResolvedValue(mockConfig);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const config = await getClientConfig();
		expect(invokeMock).toHaveBeenCalledWith("client_get_config", undefined);
		expect(config).toEqual(mockConfig);
	});

	it("saveClientConfig invokes client_save_config with payload", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const payload = { active_profile_id: "p1" };
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

	it("getClientLogs passes limit parameter", async () => {
		const mockLogs = [{ timestamp: "12:00", level: "info", target: "prism", message: "ready" }];
		const invokeMock = vi.fn().mockResolvedValue(mockLogs);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const logs = await getClientLogs(50);
		expect(invokeMock).toHaveBeenCalledWith("client_logs", { limit: 50 });
		expect(logs).toEqual(mockLogs);
	});

	it("listLocalMiddlewares invokes client_list_middlewares", async () => {
		const invokeMock = vi
			.fn()
			.mockResolvedValue([{ name: "minecraft", schema: null, effective_config: {} }]);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const list = await listLocalMiddlewares();
		expect(invokeMock).toHaveBeenCalledWith("client_list_middlewares", undefined);
		expect(list[0]?.name).toBe("minecraft");
	});

	it("updateLocalMiddlewareConfig invokes client_update_middleware_config", async () => {
		const invokeMock = vi.fn().mockResolvedValue({
			status: "ok",
			name: "minecraft",
			config: { compression: 3 },
		});
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const res = await updateLocalMiddlewareConfig("minecraft", { compression: 3 });
		expect(invokeMock).toHaveBeenCalledWith("client_update_middleware_config", {
			name: "minecraft",
			config: { compression: 3 },
		});
		expect(res.status).toBe("ok");
	});

	it("resetLocalMiddlewareConfig invokes client_reset_middleware_config", async () => {
		const invokeMock = vi.fn().mockResolvedValue({ status: "ok", name: "minecraft", reset: true });
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const res = await resetLocalMiddlewareConfig("minecraft");
		expect(invokeMock).toHaveBeenCalledWith("client_reset_middleware_config", {
			name: "minecraft",
		});
		expect(res.reset).toBe(true);
	});

	it("clearClientLogs invokes client_clear_logs", async () => {
		const invokeMock = vi.fn().mockResolvedValue(undefined);
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
			invoke: invokeMock,
		};

		const res = await clearClientLogs();
		expect(invokeMock).toHaveBeenCalledWith("client_clear_logs", undefined);
		expect(res).toEqual({ ok: true });
	});
});
