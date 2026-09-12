import {
	isTunnelAdminConnection,
	normalizeBaseUrl,
	type PanelConnection,
} from "@/lib/panelConnection";
import type { AuthSessionResponse } from "@/types/admin";

export function sessionIsAdmin(res: AuthSessionResponse | null | undefined): boolean {
	return Boolean(res?.is_admin || res?.role?.toLowerCase() === "admin");
}

/**
 * Whether the admin session query should run.
 * Tunnel `$admin` is only reachable while the sidecar is connected.
 */
export function shouldFetchAdminSession(
	connection: PanelConnection | null,
	tunnelState: string | null | undefined,
): boolean {
	if (!connection) {
		return false;
	}
	if (!isTunnelAdminConnection(connection)) {
		return Boolean(normalizeBaseUrl(connection.baseUrl || ""));
	}
	return tunnelState === "connected";
}

/** Cached session is only live while the backing channel is reachable. */
export function liveAuthSession(
	connection: PanelConnection | null,
	tunnelState: string | null | undefined,
	cached: AuthSessionResponse | null | undefined,
): AuthSessionResponse | null {
	if (!shouldFetchAdminSession(connection, tunnelState)) {
		return null;
	}
	return cached ?? null;
}

export function shouldLeaveAdminConsole(
	connection: PanelConnection | null,
	tunnelState: string | null | undefined,
	actionLoading: boolean,
): boolean {
	if (actionLoading) {
		return false;
	}
	if (!connection || !isTunnelAdminConnection(connection)) {
		return false;
	}
	return tunnelState === "idle" || tunnelState === "disconnected";
}

/**
 * Auto-bind the in-band `$admin` connection from a stored client token.
 * Never overwrite a live HTTP panel login.
 */
export function shouldAdoptTunnelPanelConnection(
	connection: PanelConnection | null,
	autoConnectPanel: boolean,
	authToken: string,
): boolean {
	if (!autoConnectPanel || !authToken.trim()) {
		return false;
	}
	if (!connection) {
		return true;
	}
	return isTunnelAdminConnection(connection);
}
