import { QueryClientProvider, useQuery } from "@tanstack/react-query";
import { useLocation, useNavigate } from "@tanstack/react-router";
import { useEffect, useRef } from "react";

import { useClientConfig } from "@/hooks/useClientConfig";
import { getClientStatus } from "@/lib/client/clientIpc";
import {
	applyImportedProfile,
	finalizeLogin,
	redetectProviders,
	syncTunnelPanelConnection,
} from "@/lib/state/clientActions";
import { useClientUiStore } from "@/lib/state/clientUiStore";
import { usePanelStore } from "@/lib/state/panelStore";
import { getQueryClient } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import { shouldLeaveAdminConsole } from "@/lib/state/session";
import type { ClientProfile } from "@/types/client";
import { m } from "@/paraglide/messages";

function PanelHydrator({ children }: { children: React.ReactNode }) {
	const hydrate = usePanelStore((s) => s.hydrate);

	useEffect(() => {
		hydrate();
	}, [hydrate]);

	return <>{children}</>;
}

function ClientOrchestrator() {
	const navigate = useNavigate();
	const location = useLocation();
	const statusQuery = useQuery({
		queryKey: queryKeys.client.status,
		queryFn: getClientStatus,
		refetchInterval: 1_500,
	});
	const { configLoaded, serverAddr, authToken, autoConnectPanel } = useClientConfig();
	const ingestStatus = useClientUiStore((s) => s.ingestStatus);
	const actionLoading = useClientUiStore((s) => s.actionLoading);
	const connection = usePanelStore((s) => s.connection);
	const seededLink = useRef(false);
	const status = statusQuery.data;
	const tunnelState = status?.state ?? null;

	useEffect(() => {
		ingestStatus(status);
	}, [ingestStatus, status]);

	useEffect(() => {
		if (!configLoaded) {
			return;
		}
		if (!seededLink.current && serverAddr) {
			seededLink.current = true;
			useClientUiStore.getState().setRemoteLinkInput((prev) => prev || serverAddr);
		}
		syncTunnelPanelConnection();
	}, [authToken, autoConnectPanel, configLoaded, serverAddr]);

	useEffect(() => {
		if (!shouldLeaveAdminConsole(connection, tunnelState, actionLoading)) {
			return;
		}
		useClientUiStore.getState().setLoginAdminUnlocked(false);
		const onAdmin = location.pathname === "/admin" || location.pathname.startsWith("/admin/");
		if (onAdmin) {
			void navigate({ to: "/" });
		}
	}, [actionLoading, connection, location.pathname, navigate, tunnelState]);

	useEffect(() => {
		if (status?.state !== "connected") {
			if (!status?.running) {
				useClientUiStore.getState().clearProviders();
			}
			return;
		}
		if (authToken) {
			return;
		}
		const ui = useClientUiStore.getState();
		if (ui.checkingProviders || ui.oauthWaitingCallback || ui.oauthExchanging) {
			return;
		}
		if (ui.providersResult || ui.providersError) {
			return;
		}
		void redetectProviders();
	}, [authToken, status?.running, status?.state]);

	useEffect(() => {
		const handleExchangeStart = () => {
			const ui = useClientUiStore.getState();
			ui.setOauthWaitingCallback(false);
			ui.setOauthExchanging(true);
			ui.setAuthError(null);
		};

		const handleExchangeError = (event: Event) => {
			const customEvent = event as CustomEvent<{ error?: string }>;
			const ui = useClientUiStore.getState();
			ui.setOauthWaitingCallback(false);
			ui.setOauthExchanging(false);
			ui.setOauthLoading(false);
			ui.setAuthError(customEvent.detail?.error || m.client_authorization_failed());
		};

		const handleDeepLinkAuth = (event: Event) => {
			const customEvent = event as CustomEvent<{
				token: string;
				userId?: string;
				username?: string;
				role?: string;
				token_id?: string;
				expires_at_unix_ms?: number | null;
			}>;
			const { token, role, token_id, userId, username, expires_at_unix_ms } = customEvent.detail;
			if (!token) {
				return;
			}
			void finalizeLogin(token, {
				token_id,
				user_id: userId,
				username,
				role,
				expires_at: expires_at_unix_ms,
			}).then((result) => {
				if (result.goAdmin) {
					void navigate({ to: "/admin" });
				}
			});
		};

		const handleDeepLinkProfile = (event: Event) => {
			const customEvent = event as CustomEvent<Partial<ClientProfile>>;
			applyImportedProfile(customEvent.detail);
		};

		window.addEventListener("prism:deep-link-exchange-start", handleExchangeStart);
		window.addEventListener("prism:deep-link-exchange-error", handleExchangeError);
		window.addEventListener("prism:deep-link-auth", handleDeepLinkAuth);
		window.addEventListener("prism:deep-link-profile", handleDeepLinkProfile);
		return () => {
			window.removeEventListener("prism:deep-link-exchange-start", handleExchangeStart);
			window.removeEventListener("prism:deep-link-exchange-error", handleExchangeError);
			window.removeEventListener("prism:deep-link-auth", handleDeepLinkAuth);
			window.removeEventListener("prism:deep-link-profile", handleDeepLinkProfile);
		};
	}, [navigate]);

	return null;
}

export function AppStateProvider({ children }: { children: React.ReactNode }) {
	const queryClient = getQueryClient();
	return (
		<QueryClientProvider client={queryClient}>
			<PanelHydrator>
				<ClientOrchestrator />
				{children}
			</PanelHydrator>
		</QueryClientProvider>
	);
}
