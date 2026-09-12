import { isTunnelAdminConnection, type PanelConnection } from "@/lib/panelConnection";
import type { AuthSessionResponse } from "@/types/admin";

export type AdminConsoleAccess = "loading" | "allow" | "deny";

/**
 * Decide whether the admin layout should spin, render, or reject.
 *
 * A confirmed admin always stays allowed while a refresh is in flight.
 * Tunnel `$admin` without a snapshot is "not confirmed yet", not "not an admin".
 */
export function resolveAdminConsoleAccess(input: {
	ready: boolean;
	isLoadingSession: boolean;
	isAdmin: boolean;
	authSession: AuthSessionResponse | null;
	connection: PanelConnection | null;
	tunnelState?: string | null;
}): AdminConsoleAccess {
	if (!input.ready) {
		return "loading";
	}
	if (
		input.connection &&
		isTunnelAdminConnection(input.connection) &&
		input.tunnelState !== undefined
	) {
		if (!input.tunnelState || input.tunnelState === "connecting") {
			return "loading";
		}
		if (input.tunnelState !== "connected") {
			return "deny";
		}
	}
	if (input.isAdmin) {
		return "allow";
	}
	if (input.isLoadingSession) {
		return "loading";
	}
	if (input.connection && isTunnelAdminConnection(input.connection) && input.authSession === null) {
		return "loading";
	}
	return "deny";
}
