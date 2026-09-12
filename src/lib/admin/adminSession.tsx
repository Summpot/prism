import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";

import { getAuthSession } from "@/lib/admin/adminApi";
import {
	clearPanelConnection,
	isTunnelAdminConnection,
	loadPanelConnection,
	type PanelConnection,
	persistPanelConnection,
} from "@/lib/panelConnection";
import type { AuthSessionResponse } from "@/types/admin";

export interface AdminSessionContextValue {
	connection: PanelConnection | null;
	ready: boolean;
	authSession: AuthSessionResponse | null;
	isAdmin: boolean;
	isLoadingSession: boolean;
	refreshSession: () => Promise<AuthSessionResponse | null>;
	saveConnection: (value: PanelConnection) => void;
	applySessionSnapshot: (session: AuthSessionResponse) => void;
	suspendSession: () => void;
	clearConnection: () => void;
}

function sessionIsAdmin(res: AuthSessionResponse | null): boolean {
	return Boolean(res?.is_admin || res?.role?.toLowerCase() === "admin");
}

export type PanelSessionContextValue = AdminSessionContextValue;

const AdminSessionContext = createContext<AdminSessionContextValue | null>(null);

export function AdminSessionProvider({ children }: { children: React.ReactNode }) {
	const [connection, setConnection] = useState<PanelConnection | null>(null);
	const [ready, setReady] = useState(false);
	const [authSession, setAuthSession] = useState<AuthSessionResponse | null>(null);
	const [isAdmin, setIsAdmin] = useState(false);
	const [isLoadingSession, setIsLoadingSession] = useState(false);

	const fetchSession = useCallback(
		async (conn: PanelConnection | null): Promise<AuthSessionResponse | null> => {
			if (!conn || (!conn.baseUrl && !isTunnelAdminConnection(conn))) {
				setAuthSession(null);
				setIsAdmin(false);
				return null;
			}

			setIsLoadingSession(true);
			try {
				const res = await getAuthSession(conn);
				setAuthSession(res);
				setIsAdmin(sessionIsAdmin(res) && Boolean(res.authenticated));
				return res;
			} catch (err) {
				console.debug("Failed to get auth session:", err);
				// Transient fetch failures must not drop a confirmed admin session.
				return null;
			} finally {
				setIsLoadingSession(false);
			}
		},
		[],
	);

	useEffect(() => {
		if (typeof window === "undefined") {
			setReady(true);
			return;
		}

		const initialConn = loadPanelConnection(window.localStorage);
		setConnection(initialConn);
		setReady(true);
		// Tunnel `$admin` is only reachable while the sidecar is connected.
		// ClientContext refreshes the session when the tunnel comes up.
		if (initialConn && !isTunnelAdminConnection(initialConn)) {
			void fetchSession(initialConn);
		}
	}, [fetchSession]);

	const saveConnection = useCallback(
		(next: PanelConnection) => {
			if (typeof window !== "undefined") {
				const saved = persistPanelConnection(window.localStorage, next);
				setConnection(saved);
				if (!isTunnelAdminConnection(saved)) {
					void fetchSession(saved);
				}
			}
		},
		[fetchSession],
	);

	const applySessionSnapshot = useCallback((session: AuthSessionResponse) => {
		setAuthSession(session);
		setIsAdmin(sessionIsAdmin(session) && Boolean(session.authenticated));
	}, []);

	const suspendSession = useCallback(() => {
		setAuthSession(null);
		setIsAdmin(false);
	}, []);

	const clearConnection = useCallback(() => {
		if (typeof window !== "undefined") {
			clearPanelConnection(window.localStorage);
		}
		setConnection(null);
		setAuthSession(null);
		setIsAdmin(false);
	}, []);

	const refreshSession = useCallback(async () => {
		return fetchSession(connection);
	}, [connection, fetchSession]);

	const value = useMemo<AdminSessionContextValue>(
		() => ({
			connection,
			ready,
			authSession,
			isAdmin,
			isLoadingSession,
			refreshSession,
			saveConnection,
			applySessionSnapshot,
			suspendSession,
			clearConnection,
		}),
		[
			connection,
			ready,
			authSession,
			isAdmin,
			isLoadingSession,
			refreshSession,
			saveConnection,
			applySessionSnapshot,
			suspendSession,
			clearConnection,
		],
	);

	return <AdminSessionContext.Provider value={value}>{children}</AdminSessionContext.Provider>;
}

export function useAdminSession() {
	const value = useContext(AdminSessionContext);
	if (!value) {
		throw new Error("useAdminSession must be used within AdminSessionProvider");
	}
	return value;
}

// Backwards compatibility aliases
export const PanelSessionProvider = AdminSessionProvider;
export const usePanelSession = useAdminSession;
