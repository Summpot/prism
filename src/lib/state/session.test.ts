import { describe, expect, it } from "vitest";

import type { PanelConnection } from "@/lib/panelConnection";
import {
	liveAuthSession,
	sessionIsAdmin,
	shouldAdoptTunnelPanelConnection,
	shouldFetchAdminSession,
	shouldLeaveAdminConsole,
} from "@/lib/state/session";
import type { AuthSessionResponse } from "@/types/admin";

const tunnel: PanelConnection = { baseUrl: "", token: "tok", kind: "tunnel-admin" };
const bearer: PanelConnection = {
	baseUrl: "http://127.0.0.1:8080",
	token: "tok",
	kind: "bearer",
};
const adminSession: AuthSessionResponse = {
	authenticated: true,
	is_admin: true,
	role: "admin",
};

describe("shouldFetchAdminSession", () => {
	it("does not fetch without a connection", () => {
		expect(shouldFetchAdminSession(null, "connected")).toBe(false);
	});

	it("fetches HTTP panel sessions regardless of tunnel state", () => {
		expect(shouldFetchAdminSession(bearer, "idle")).toBe(true);
		expect(shouldFetchAdminSession(bearer, "connected")).toBe(true);
	});

	it("fetches tunnel $admin only while the sidecar is connected", () => {
		expect(shouldFetchAdminSession(tunnel, "connected")).toBe(true);
		expect(shouldFetchAdminSession(tunnel, "connecting")).toBe(false);
		expect(shouldFetchAdminSession(tunnel, "idle")).toBe(false);
		expect(shouldFetchAdminSession(tunnel, "disconnected")).toBe(false);
	});
});

describe("liveAuthSession", () => {
	it("hides a cached tunnel session once the sidecar drops", () => {
		expect(liveAuthSession(tunnel, "connected", adminSession)).toEqual(adminSession);
		expect(liveAuthSession(tunnel, "disconnected", adminSession)).toBeNull();
		expect(liveAuthSession(tunnel, "idle", adminSession)).toBeNull();
	});

	it("keeps an HTTP panel session while the tunnel is down", () => {
		expect(liveAuthSession(bearer, "idle", adminSession)).toEqual(adminSession);
	});
});

describe("shouldLeaveAdminConsole", () => {
	it("leaves admin when a tunnel-admin session idles", () => {
		expect(shouldLeaveAdminConsole(tunnel, "idle", false)).toBe(true);
		expect(shouldLeaveAdminConsole(tunnel, "disconnected", false)).toBe(true);
	});

	it("does not navigate away during connect or for HTTP panels", () => {
		expect(shouldLeaveAdminConsole(tunnel, "idle", true)).toBe(false);
		expect(shouldLeaveAdminConsole(tunnel, "connecting", false)).toBe(false);
		expect(shouldLeaveAdminConsole(bearer, "idle", false)).toBe(false);
	});
});

describe("shouldAdoptTunnelPanelConnection", () => {
	it("adopts a stored token when no panel connection exists", () => {
		expect(shouldAdoptTunnelPanelConnection(null, true, "prism_cl_abc")).toBe(true);
	});

	it("refreshes an existing tunnel-admin connection", () => {
		expect(shouldAdoptTunnelPanelConnection(tunnel, true, "prism_cl_abc")).toBe(true);
	});

	it("does not overwrite a live HTTP panel login", () => {
		expect(shouldAdoptTunnelPanelConnection(bearer, true, "prism_cl_abc")).toBe(false);
	});

	it("does nothing without auto-connect or a token", () => {
		expect(shouldAdoptTunnelPanelConnection(null, false, "prism_cl_abc")).toBe(false);
		expect(shouldAdoptTunnelPanelConnection(null, true, "")).toBe(false);
	});
});

describe("sessionIsAdmin", () => {
	it("treats role=admin as admin even without the flag", () => {
		expect(sessionIsAdmin({ authenticated: true, is_admin: false, role: "admin" })).toBe(true);
		expect(sessionIsAdmin({ authenticated: true, is_admin: false, role: "member" })).toBe(false);
	});
});
