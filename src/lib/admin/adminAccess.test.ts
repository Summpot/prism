import { describe, expect, it } from "vitest";

import { resolveAdminConsoleAccess } from "./adminAccess";
import type { PanelConnection } from "@/lib/panelConnection";
import type { AuthSessionResponse } from "@/types/admin";

const tunnel: PanelConnection = { baseUrl: "", token: "tok", kind: "tunnel-admin" };
const bearer: PanelConnection = {
	baseUrl: "http://127.0.0.1:8080",
	token: "tok",
	kind: "bearer",
};
const memberSession: AuthSessionResponse = {
	authenticated: true,
	is_admin: false,
	role: "member",
};

describe("resolveAdminConsoleAccess", () => {
	it("waits until the session store is ready", () => {
		expect(
			resolveAdminConsoleAccess({
				ready: false,
				isLoadingSession: false,
				isAdmin: false,
				authSession: null,
				connection: null,
			}),
		).toBe("loading");
	});

	it("keeps a confirmed admin on the console while a refresh is in flight", () => {
		expect(
			resolveAdminConsoleAccess({
				ready: true,
				isLoadingSession: true,
				isAdmin: true,
				authSession: { authenticated: true, is_admin: true, role: "admin" },
				connection: tunnel,
			}),
		).toBe("allow");
	});

	it("does not flash access-denied when tunnel admin has no snapshot yet", () => {
		expect(
			resolveAdminConsoleAccess({
				ready: true,
				isLoadingSession: false,
				isAdmin: false,
				authSession: null,
				connection: tunnel,
			}),
		).toBe("loading");
	});

	it("denies a confirmed member on the tunnel admin channel", () => {
		expect(
			resolveAdminConsoleAccess({
				ready: true,
				isLoadingSession: false,
				isAdmin: false,
				authSession: memberSession,
				connection: tunnel,
			}),
		).toBe("deny");
	});

	it("denies a ready HTTP panel session that is not admin", () => {
		expect(
			resolveAdminConsoleAccess({
				ready: true,
				isLoadingSession: false,
				isAdmin: false,
				authSession: null,
				connection: bearer,
			}),
		).toBe("deny");
	});

	it("shows a spinner while the first session fetch is in flight", () => {
		expect(
			resolveAdminConsoleAccess({
				ready: true,
				isLoadingSession: true,
				isAdmin: false,
				authSession: null,
				connection: bearer,
			}),
		).toBe("loading");
	});

	it("denies tunnel admin when the sidecar is down", () => {
		expect(
			resolveAdminConsoleAccess({
				ready: true,
				isLoadingSession: false,
				isAdmin: false,
				authSession: null,
				connection: tunnel,
				tunnelState: "disconnected",
			}),
		).toBe("deny");
	});

	it("waits while the tunnel is connecting", () => {
		expect(
			resolveAdminConsoleAccess({
				ready: true,
				isLoadingSession: false,
				isAdmin: false,
				authSession: null,
				connection: tunnel,
				tunnelState: "connecting",
			}),
		).toBe("loading");
	});
});
