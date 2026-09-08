import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";

import { type AuthSessionResponse, getAuthSession } from "@/lib/managementApi";
import {
	clearPanelConnection,
	loadPanelConnection,
	type PanelConnection,
	persistPanelConnection,
} from "@/lib/panelConnection";

export interface PanelSessionContextValue {
	connection: PanelConnection | null;
	ready: boolean;
	authSession: AuthSessionResponse | null;
	isAdmin: boolean;
	isLoadingSession: boolean;
	refreshSession: () => Promise<AuthSessionResponse | null>;
	saveConnection: (value: PanelConnection) => void;
	clearConnection: () => void;
}

const PanelSessionContext = createContext<PanelSessionContextValue | null>(null);

export function PanelSessionProvider({ children }: { children: React.ReactNode }) {
	const [connection, setConnection] = useState<PanelConnection | null>(null);
	const [ready, setReady] = useState(false);
	const [authSession, setAuthSession] = useState<AuthSessionResponse | null>(null);
	const [isAdmin, setIsAdmin] = useState(false);
	const [isLoadingSession, setIsLoadingSession] = useState(false);

	const fetchSession = useCallback(
		async (conn: PanelConnection | null): Promise<AuthSessionResponse | null> => {
			if (!conn || !conn.baseUrl) {
				setAuthSession(null);
				setIsAdmin(false);
				return null;
			}

			setIsLoadingSession(true);
			try {
				const res = await getAuthSession(conn);
				setAuthSession(res);
				const admin = Boolean(res.is_admin || res.role === "admin");
				setIsAdmin(admin);
				return res;
			} catch (err) {
				console.debug("Failed to get auth session:", err);
				setAuthSession(null);
				setIsAdmin(false);
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
		if (initialConn) {
			void fetchSession(initialConn);
		}
	}, [fetchSession]);

	const saveConnection = useCallback(
		(next: PanelConnection) => {
			if (typeof window !== "undefined") {
				const saved = persistPanelConnection(window.localStorage, next);
				setConnection(saved);
				void fetchSession(saved);
			}
		},
		[fetchSession],
	);

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

	const value = useMemo<PanelSessionContextValue>(
		() => ({
			connection,
			ready,
			authSession,
			isAdmin,
			isLoadingSession,
			refreshSession,
			saveConnection,
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
			clearConnection,
		],
	);

	return <PanelSessionContext.Provider value={value}>{children}</PanelSessionContext.Provider>;
}

export function usePanelSession() {
	const value = useContext(PanelSessionContext);
	if (!value) {
		throw new Error("usePanelSession must be used within PanelSessionProvider");
	}
	return value;
}
