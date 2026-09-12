import { useNavigate } from "@tanstack/react-router";
import { useCallback } from "react";

import {
	connectClient,
	connectFromLink,
	disconnectClient,
	handleManualOAuthCallback as exchangeOAuthCallback,
	resetStats,
	signOut,
} from "@/lib/state/clientActions";
import { useClientUiStore } from "@/lib/state/clientUiStore";
import { getQueryClient } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import type { ClientStatusResponse } from "@/types/client";

export function useClientActions() {
	const navigate = useNavigate();
	const actionLoading = useClientUiStore((s) => s.actionLoading);
	const error = useClientUiStore((s) => s.error);
	const setError = useClientUiStore((s) => s.setError);

	const goAdminIfNeeded = useCallback(
		async (result: { goAdmin: boolean }) => {
			if (result.goAdmin) {
				void navigate({ to: "/admin" });
			}
		},
		[navigate],
	);

	const handleConnect = useCallback(() => connectClient(), []);
	const handleDisconnect = useCallback(() => disconnectClient(), []);
	const handleResetStats = useCallback(() => resetStats(), []);

	const handleToggleTunnel = useCallback(() => {
		const status = getQueryClient().getQueryData<ClientStatusResponse>(queryKeys.client.status);
		if (status?.running) {
			void disconnectClient();
		} else {
			void connectClient();
		}
	}, []);

	const handleConnectFromLink = useCallback(
		async (customLink?: string) => {
			const result = await connectFromLink(customLink);
			await goAdminIfNeeded(result);
		},
		[goAdminIfNeeded],
	);

	const handleManualOAuthCallback = useCallback(
		async (input: string) => {
			const result = await exchangeOAuthCallback(input);
			await goAdminIfNeeded(result);
		},
		[goAdminIfNeeded],
	);

	const handleSignOut = useCallback(() => signOut(), []);

	return {
		actionLoading,
		error,
		setError,
		handleConnect,
		handleDisconnect,
		handleToggleTunnel,
		handleResetStats,
		handleConnectFromLink,
		handleManualOAuthCallback,
		handleSignOut,
	};
}
