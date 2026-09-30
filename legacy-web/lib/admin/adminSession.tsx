import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";

import { getAuthSession } from "@/lib/admin/adminApi";
import { getClientStatus } from "@/lib/client/clientIpc";
import type { PanelConnection } from "@/lib/panelConnection";
import { usePanelStore } from "@/lib/state/panelStore";
import { queryKeys } from "@/lib/state/queryKeys";
import { liveAuthSession, sessionIsAdmin, shouldFetchAdminSession } from "@/lib/state/session";
import type { AuthSessionResponse } from "@/types/admin";

export interface AdminSessionContextValue {
	connection: PanelConnection | null;
	ready: boolean;
	authSession: AuthSessionResponse | null;
	isAdmin: boolean;
	isLoadingSession: boolean;
	tunnelState: string | null;
	refreshSession: () => Promise<AuthSessionResponse | null>;
	saveConnection: (value: PanelConnection) => void;
	applySessionSnapshot: (session: AuthSessionResponse) => void;
	suspendSession: () => void;
	clearConnection: () => void;
}

export type PanelSessionContextValue = AdminSessionContextValue;

export function useAdminSession(): AdminSessionContextValue {
	const queryClient = useQueryClient();
	const connection = usePanelStore((s) => s.connection);
	const ready = usePanelStore((s) => s.ready);
	const saveConnection = usePanelStore((s) => s.saveConnection);
	const clearConnection = usePanelStore((s) => s.clearConnection);

	const statusQuery = useQuery({
		queryKey: queryKeys.client.status,
		queryFn: getClientStatus,
		refetchInterval: 1_500,
		select: (status) => status.state,
	});
	const tunnelState =
		statusQuery.isPending && !statusQuery.data ? null : (statusQuery.data ?? "idle");

	const sessionEnabled = ready && shouldFetchAdminSession(connection, tunnelState);
	const sessionQuery = useQuery({
		queryKey: queryKeys.admin.session(connection),
		queryFn: () => getAuthSession(connection!),
		enabled: sessionEnabled,
		staleTime: 5_000,
		retry: 1,
	});

	const authSession = liveAuthSession(connection, tunnelState, sessionQuery.data);
	const isAdmin = sessionIsAdmin(authSession) && Boolean(authSession?.authenticated);
	const isLoadingSession = sessionEnabled && sessionQuery.isPending && authSession === null;

	const refreshSession = useCallback(async () => {
		if (!shouldFetchAdminSession(connection, tunnelState)) {
			return null;
		}
		const result = await queryClient.fetchQuery({
			queryKey: queryKeys.admin.session(connection),
			queryFn: () => getAuthSession(connection!),
		});
		return result;
	}, [connection, queryClient, tunnelState]);

	const applySessionSnapshot = useCallback(
		(session: AuthSessionResponse) => {
			queryClient.setQueryData(queryKeys.admin.session(connection), session);
		},
		[connection, queryClient],
	);

	const suspendSession = useCallback(() => {
		queryClient.setQueryData(queryKeys.admin.session(connection), null);
	}, [connection, queryClient]);

	return useMemo<AdminSessionContextValue>(
		() => ({
			connection,
			ready,
			authSession,
			isAdmin,
			isLoadingSession,
			tunnelState,
			refreshSession,
			saveConnection,
			applySessionSnapshot,
			suspendSession,
			clearConnection,
		}),
		[
			applySessionSnapshot,
			authSession,
			clearConnection,
			connection,
			isAdmin,
			isLoadingSession,
			ready,
			refreshSession,
			saveConnection,
			suspendSession,
			tunnelState,
		],
	);
}

export function usePanelSession(): AdminSessionContextValue {
	return useAdminSession();
}

/** No-op: session is derived from the query cache and panel store. */
export function AdminSessionProvider({ children }: { children: React.ReactNode }) {
	return <>{children}</>;
}

export const PanelSessionProvider = AdminSessionProvider;
