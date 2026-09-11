import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate } from "@tanstack/react-router";

import { useClientLogs } from "@/hooks/useClientLogs";
import { useClientProfiles } from "@/hooks/useClientProfiles";
import { useClientStatus } from "@/hooks/useClientStatus";
import { usePrismLink } from "@/hooks/usePrismLink";
import { openExternalUrl } from "@/lib/desktopWindow";
import {
	exchangeGitHubCode,
	getAuthProviders,
	getClientStatus,
	getGitHubLoginUrl,
	resetClientStats,
	saveClientConfig,
	startClient,
	stopClient,
} from "@/lib/managementApi";
import { parseDeepLink } from "@/lib/deepLink";
import {
	TUNNEL_ADMIN_CONNECTION,
	isTunnelAdminConnection,
	normalizeBaseUrl,
	tunnelAdminConnection,
} from "@/lib/panelConnection";
import { useAdminSession } from "@/lib/admin/adminSession";
import { SUPPORTED_LINK_PROTOCOLS, resolveRemoteConnection } from "@/lib/prismLink";
import type {
	AuthProvidersResponse,
	ClientContextValue,
	ClientProfile,
	ClientStatusResponse,
	UserRecord,
} from "@/types/client";
import { m } from "@/paraglide/messages";

type LoginSessionExtra = {
	token_id?: string;
	user_id?: string;
	username?: string;
	display_name?: string | null;
	avatar_url?: string | null;
	role?: string;
	expires_at?: number | null;
	panelUrl?: string;
};

export type { ClientContextValue };

export const DEFAULT_CLIENT_CONTEXT: ClientContextValue = {
	status: null,
	cumulativeStats: null,
	statsViewMode: "session",
	setStatsViewMode: () => {},
	throughputSamples: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
	uptimeSeconds: 0,
	formatUptime: () => "00:00:00",
	isRunning: false,
	isConnected: false,
	isConnecting: false,
	rawBytes: 0,
	wireBytes: 0,
	savedRatio: 0,

	actionLoading: false,
	error: null,
	setError: () => {},
	copied: null,
	copyText: () => {},
	handleConnect: async () => {},
	handleDisconnect: async () => {},
	handleToggleTunnel: () => {},
	handleResetStats: async () => {},
	handleShareLink: () => {},

	profiles: [],
	selectedProfileId: "",
	profileName: m.client_default_realm(),
	setProfileName: () => {},
	serverAddr: "127.0.0.1",
	setServerAddr: () => {},
	transport: "auto",
	setTransport: () => {},
	authToken: "",
	setAuthToken: () => {},
	listenAddr: "127.0.0.1:25565",
	setListenAddr: () => {},
	fakeLanBroadcast: true,
	setFakeLanBroadcast: () => {},
	autoConnectPanel: true,
	setAutoConnectPanel: () => {},
	autoConnect: true,
	setAutoConnect: () => {},
	managementUrl: "http://127.0.0.1:8080",
	handleSelectProfile: () => {},
	handleSaveProfile: async () => {},
	handleDeleteProfile: async () => {},

	remoteLinkInput: "",
	setRemoteLinkInput: () => {},
	linkProtocol: "auto://",
	setLinkProtocol: () => {},
	handleSelectProtocol: () => {},
	handleAddressChange: () => {},
	handleAddressPaste: () => {},
	handleAddressCopy: () => {},
	handleConnectFromLink: async () => {},

	logs: [],
	filteredLogs: [],
	logFilterLevel: "ALL",
	setLogFilterLevel: () => {},
	logSearchQuery: "",
	setLogSearchQuery: () => {},
	autoScrollLogs: true,
	setAutoScrollLogs: () => {},
	isAtBottom: true,
	logsContainerRef: { current: null },
	handleLogsScroll: () => {},
	scrollToBottom: () => {},
	handleClearLogs: async () => {},
	handleCopyAllLogs: () => {},

	loginModalOpen: false,
	setLoginModalOpen: () => {},
	checkingProviders: false,
	providersResult: null,
	setProvidersResult: () => {},
	providersError: null,
	setProvidersError: () => {},
	authServerUrl: "http://127.0.0.1:8080",
	setAuthServerUrl: () => {},
	authError: null,
	setAuthError: () => {},
	oauthLoading: false,
	oauthWaitingCallback: false,
	setOauthWaitingCallback: () => {},
	oauthExchanging: false,
	manualCallbackInput: "",
	setManualCallbackInput: () => {},
	handleManualOAuthCallback: async () => {},
	startGitHubAuthWithUrl: async () => {},
	handleRedetectProviders: async () => {},
	loginAdminUnlocked: false,

	importModalOpen: false,
	setImportModalOpen: () => {},
	importUrl: "",
	setImportUrl: () => {},
	importError: null,
	setImportError: () => {},
	handleImportLink: () => {},
};

export const ClientContext =
	(typeof globalThis !== "undefined" &&
		(globalThis as unknown as { __PRISM_CLIENT_CONTEXT__?: React.Context<ClientContextValue> })
			.__PRISM_CLIENT_CONTEXT__) ||
	createContext<ClientContextValue>(DEFAULT_CLIENT_CONTEXT);

if (typeof globalThis !== "undefined") {
	(
		globalThis as unknown as { __PRISM_CLIENT_CONTEXT__?: React.Context<ClientContextValue> }
	).__PRISM_CLIENT_CONTEXT__ = ClientContext;
}

export function ClientProvider({ children }: { children: React.ReactNode }) {
	const navigate = useNavigate();
	const location = useLocation();
	const { connection, saveConnection, refreshSession, applySessionSnapshot, suspendSession } =
		useAdminSession();

	const [actionLoading, setActionLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [loginAdminUnlocked, setLoginAdminUnlocked] = useState(false);

	// 1. Status hook
	const clientStatus = useClientStatus();
	const { status, setStatus, fetchStatus, setCumulativeStats } = clientStatus;

	// 2. Profiles hook
	const clientProfiles = useClientProfiles({
		onCumulativeStatsLoaded: setCumulativeStats,
		onRemoteLinkInputSync: (sAddr) => {
			prismLink.setRemoteLinkInput((prev) => prev || sAddr);
		},
	});
	const {
		selectedProfileId,
		profileName,
		setProfileName,
		serverAddr,
		setServerAddr,
		transport,
		setTransport,
		authToken,
		setAuthToken,
		listenAddr,
		setListenAddr,
		fakeLanBroadcast,
		setFakeLanBroadcast,
		autoConnectPanel,
		autoConnect,
		deviceId,
		configLoaded,
		fetchClientConfigData,
	} = clientProfiles;

	// 3. Prism link hook
	const prismLink = usePrismLink({
		serverAddr,
		setServerAddr,
		transport,
		setTransport,
		profileName,
		setProfileName,
		listenAddr,
		setListenAddr,
		authToken,
		fakeLanBroadcast,
		setFakeLanBroadcast,
		onConnectFromLink: (link) => {
			void handleConnectFromLink(link);
		},
	});

	// 4. Logs hook
	const clientLogs = useClientLogs(prismLink.setCopied);
	const { fetchLogs } = clientLogs;

	// Login modal & auth state
	const [loginModalOpen, setLoginModalOpen] = useState(false);
	const [checkingProviders, setCheckingProviders] = useState(false);
	const [providersResult, setProvidersResult] = useState<AuthProvidersResponse | null>(null);
	const [providersError, setProvidersError] = useState<string | null>(null);
	const [authServerUrl, setAuthServerUrl] = useState("http://127.0.0.1:8080");
	const [authError, setAuthError] = useState<string | null>(null);
	const [oauthLoading, setOauthLoading] = useState(false);
	const [oauthWaitingCallback, setOauthWaitingCallback] = useState(false);
	const [oauthExchanging, setOauthExchanging] = useState(false);
	const [manualCallbackInput, setManualCallbackInput] = useState("");

	const waitForClientConnected = useCallback(async (): Promise<ClientStatusResponse | null> => {
		let latest: ClientStatusResponse | null = null;
		for (let i = 0; i < 25; i++) {
			const st = await getClientStatus().catch(() => null);
			if (st) {
				latest = st;
				if (st.state === "connected") {
					setStatus(st);
					return st;
				}
			}
			await new Promise((resolve) => setTimeout(resolve, 200));
		}
		if (latest) setStatus(latest);
		return latest;
	}, [setStatus]);

	// Connect / Disconnect Handlers
	const handleConnect = useCallback(async () => {
		setActionLoading(true);
		setError(null);
		try {
			await startClient({
				server_addr: serverAddr,
				transport,
				auth_token: authToken,
				listen_addr: listenAddr,
				fake_lan_broadcast: fakeLanBroadcast,
				profile_id: selectedProfileId || undefined,
				profile_name: profileName || undefined,
			});
			await waitForClientConnected();
			fetchLogs();
			fetchClientConfigData();
			if (authToken) {
				await refreshSession();
			}
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setActionLoading(false);
		}
	}, [
		authToken,
		fakeLanBroadcast,
		fetchClientConfigData,
		fetchLogs,
		listenAddr,
		profileName,
		refreshSession,
		selectedProfileId,
		serverAddr,
		transport,
		waitForClientConnected,
	]);

	const handleDisconnect = useCallback(async () => {
		setActionLoading(true);
		setError(null);
		try {
			await stopClient();
			const st = await getClientStatus().catch(() => null);
			if (st) setStatus(st);
			fetchLogs();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setActionLoading(false);
		}
	}, [fetchLogs, setStatus]);

	const handleResetStats = useCallback(async () => {
		try {
			await resetClientStats();
			fetchStatus();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		}
	}, [fetchStatus]);

	const handleToggleTunnel = useCallback(() => {
		if (status?.running) {
			void handleDisconnect();
		} else {
			void handleConnect();
		}
	}, [handleConnect, handleDisconnect, status?.running]);

	const persistTunnelAuth = useCallback(
		async (token: string, extra?: LoginSessionExtra) => {
			setAuthToken(token);
			if (autoConnectPanel && token) {
				saveConnection(tunnelAdminConnection(token));
			}
			if (extra?.user_id || extra?.username || extra?.role) {
				const role = extra.role?.toLowerCase() ?? "";
				applySessionSnapshot({
					authenticated: true,
					user_id: extra.user_id ?? null,
					username: extra.username ?? null,
					display_name: extra.display_name ?? extra.username ?? null,
					avatar_url: extra.avatar_url ?? null,
					role: extra.role ?? null,
					is_admin: role === "admin",
				});
			}
			await saveClientConfig({
				active_profile_id: selectedProfileId || null,
				active_config: {
					profile_name: profileName,
					server_addr: serverAddr,
					transport,
					auth_token: token,
					listen_addr: listenAddr,
					fake_lan_broadcast: fakeLanBroadcast,
					auto_connect_panel: autoConnectPanel,
					auto_connect: autoConnect,
					token_id: extra?.token_id,
					user_id: extra?.user_id,
					username: extra?.username,
					expires_at: extra?.expires_at ?? undefined,
				},
			}).catch(() => {});
		},
		[
			applySessionSnapshot,
			autoConnect,
			autoConnectPanel,
			fakeLanBroadcast,
			listenAddr,
			profileName,
			saveConnection,
			selectedProfileId,
			serverAddr,
			setAuthToken,
			transport,
		],
	);

	const finalizeLogin = useCallback(
		async (token: string, extra?: LoginSessionExtra) => {
			if (extra?.role?.toLowerCase() === "admin") {
				setLoginAdminUnlocked(true);
			}
			setLoginModalOpen(false);
			setProvidersResult(null);
			setProvidersError(null);
			setAuthError(null);
			setOauthWaitingCallback(false);
			setOauthExchanging(false);
			setOauthLoading(false);
			setActionLoading(true);
			try {
				await persistTunnelAuth(token, extra);
				try {
					await startClient({
						server_addr: serverAddr,
						transport,
						auth_token: token,
						listen_addr: listenAddr,
						fake_lan_broadcast: fakeLanBroadcast,
						profile_id: selectedProfileId || undefined,
						profile_name: profileName || undefined,
					});
				} catch (err) {
					setError(err instanceof Error ? err.message : String(err));
				}
				const st = await waitForClientConnected();
				fetchLogs();
				if (st?.state === "connected") {
					const session = await refreshSession();
					if (
						session?.authenticated &&
						(session.is_admin || session.role?.toLowerCase() === "admin")
					) {
						void navigate({ to: "/admin" });
					}
				}
				prismLink.setRemoteLinkInput("");
				setManualCallbackInput("");
			} finally {
				setActionLoading(false);
			}
		},
		[
			fakeLanBroadcast,
			fetchLogs,
			listenAddr,
			navigate,
			persistTunnelAuth,
			prismLink,
			profileName,
			refreshSession,
			selectedProfileId,
			serverAddr,
			transport,
			waitForClientConnected,
		],
	);

	useEffect(() => {
		if (!connection || !isTunnelAdminConnection(connection)) return;
		if (status?.state === "connected") {
			void refreshSession();
			return;
		}
		if (status && !status.running && !actionLoading) {
			suspendSession();
			setLoginAdminUnlocked(false);
			if (location.pathname === "/admin" || location.pathname.startsWith("/admin/")) {
				void navigate({ to: "/" });
			}
		}
	}, [
		actionLoading,
		authToken,
		connection,
		location.pathname,
		navigate,
		refreshSession,
		status?.running,
		status?.state,
		suspendSession,
	]);

	useEffect(() => {
		if (!configLoaded) return;
		if (autoConnectPanel && authToken) {
			saveConnection(tunnelAdminConnection(authToken));
		}
	}, [authToken, autoConnectPanel, configLoaded, saveConnection]);

	// Start GitHub OAuth with target management URL via browser + deep link
	const startGitHubAuthWithUrl = useCallback(
		async (targetAuthUrl: string, targetServerAddr?: string) => {
			setAuthError(null);
			setOauthLoading(true);
			try {
				const nextServer = targetServerAddr || serverAddr;
				if (targetServerAddr) {
					setServerAddr(nextServer);
				}
				if (typeof window !== "undefined") {
					window.localStorage.removeItem("prism_pending_auth_url");
					window.sessionStorage.removeItem("prism_pending_auth_url");
				}
				const res = await getGitHubLoginUrl(TUNNEL_ADMIN_CONNECTION);
				if (res.url) {
					setOauthWaitingCallback(true);
					await openExternalUrl(res.url);
				}
			} catch (err) {
				if (targetAuthUrl.trim()) {
					try {
						const norm = normalizeBaseUrl(targetAuthUrl);
						const res = await getGitHubLoginUrl({ baseUrl: norm, token: "" });
						if (res.url) {
							setOauthWaitingCallback(true);
							await openExternalUrl(res.url);
							return;
						}
					} catch (fallbackErr) {
						setAuthError(fallbackErr instanceof Error ? fallbackErr.message : String(fallbackErr));
						setOauthWaitingCallback(false);
						return;
					}
				}
				setAuthError(err instanceof Error ? err.message : String(err));
				setOauthWaitingCallback(false);
			} finally {
				setOauthLoading(false);
			}
		},
		[serverAddr, setServerAddr],
	);

	const handleRedetectProviders = useCallback(async (overrideUrl?: string) => {
		setCheckingProviders(true);
		setProvidersError(null);
		try {
			const target = overrideUrl?.trim()
				? { baseUrl: normalizeBaseUrl(overrideUrl), token: "" }
				: TUNNEL_ADMIN_CONNECTION;
			const providers = await getAuthProviders(target);
			setProvidersResult(providers);
			if (overrideUrl?.trim()) {
				setAuthServerUrl(normalizeBaseUrl(overrideUrl));
			}
		} catch (err) {
			setProvidersError(err instanceof Error ? err.message : m.client_probe_failed());
		} finally {
			setCheckingProviders(false);
		}
	}, []);

	// Auto-connect and reconnect: probe login methods once the tunnel is up
	// without a session token (handleConnectFromLink already probes on manual connect).
	useEffect(() => {
		if (status?.state !== "connected") {
			if (!status?.running) {
				setProvidersResult(null);
				setProvidersError(null);
			}
			return;
		}
		if (authToken) return;
		if (checkingProviders || oauthWaitingCallback || oauthExchanging) return;
		if (providersResult || providersError) return;
		void handleRedetectProviders();
	}, [
		authToken,
		checkingProviders,
		handleRedetectProviders,
		oauthExchanging,
		oauthWaitingCallback,
		providersError,
		providersResult,
		status?.running,
		status?.state,
	]);

	// Manual OAuth callback exchange handler (supports prism://, http(s)://, code=..., or bare code)
	const handleManualOAuthCallback = useCallback(
		async (input: string) => {
			const raw = input.trim();
			if (!raw) return;

			let code = "";
			let customOrigin: string | null = null;

			const deep = parseDeepLink(raw);
			if (deep.kind === "auth-code") {
				code = deep.code;
			} else if (deep.kind === "auth") {
				await finalizeLogin(deep.token, {
					user_id: deep.userId,
					username: deep.username,
					role: deep.role,
				});
				return;
			} else if (raw.includes("code=")) {
				const match = raw.match(/[?&]code=([a-zA-Z0-9_-]+)/);
				if (match) code = match[1];
			} else if (/^[a-zA-Z0-9_-]{16,64}$/.test(raw)) {
				code = raw;
			}

			if (raw.toLowerCase().startsWith("http://") || raw.toLowerCase().startsWith("https://")) {
				try {
					const u = new URL(raw);
					customOrigin = u.origin;
				} catch {
					// ignore
				}
			}

			if (!code) {
				setAuthError(m.client_invalid_github_callback());
				return;
			}

			setActionLoading(true);
			setOauthExchanging(true);
			setOauthWaitingCallback(false);
			setAuthError(null);
			setProvidersError(null);

			try {
				let res: {
					token: string;
					user: UserRecord;
					token_id: string;
					expires_at_unix_ms?: number | null;
				} | null = null;
				let lastErr: unknown = null;
				try {
					res = await exchangeGitHubCode(TUNNEL_ADMIN_CONNECTION, code, deviceId || undefined);
				} catch (err) {
					lastErr = err;
				}

				if (!res && customOrigin) {
					try {
						res = await exchangeGitHubCode(
							{ baseUrl: normalizeBaseUrl(customOrigin), token: "" },
							code,
							deviceId || undefined,
						);
					} catch (err) {
						lastErr = err;
					}
				}

				if (!res) {
					throw new Error(
						lastErr instanceof Error ? lastErr.message : m.client_github_exchange_failed(),
					);
				}

				await finalizeLogin(res.token, {
					token_id: res.token_id,
					user_id: res.user?.id,
					username: res.user?.username,
					display_name: res.user?.display_name,
					avatar_url: res.user?.avatar_url,
					role: res.user?.role,
					expires_at: res.expires_at_unix_ms,
				});
			} catch (err) {
				setAuthError(err instanceof Error ? err.message : String(err));
			} finally {
				setActionLoading(false);
				setCheckingProviders(false);
				setOauthExchanging(false);
			}
		},
		[deviceId, finalizeLogin],
	);

	// Connect from remote link: initiate tunnel client connection
	const handleConnectFromLink = useCallback(
		async (customLink?: string) => {
			let raw = (customLink ?? prismLink.remoteLinkInput).trim();
			if (!raw && serverAddr) {
				raw = `${prismLink.linkProtocol}${serverAddr}`;
			}

			// Handle direct OAuth code or Token callback links pasted by user first
			const deep = parseDeepLink(raw);
			if (deep.kind === "auth-code" || deep.kind === "auth") {
				await handleManualOAuthCallback(raw);
				return;
			}

			if (raw && !raw.includes("://")) {
				raw = `${prismLink.linkProtocol}${raw}`;
			}
			if (!raw) {
				setError(m.client_link_required());
				return;
			}

			const resolved = resolveRemoteConnection(raw);
			const targetServerAddr = resolved.serverAddr;
			const targetTransport = resolved.transport || "auto";
			setServerAddr(targetServerAddr);
			setTransport(targetTransport);
			const matched = SUPPORTED_LINK_PROTOCOLS.find((p) => p.transport === targetTransport);
			if (matched) prismLink.setLinkProtocol(matched.value);
			if (resolved.name) setProfileName(resolved.name);
			if (resolved.listenAddr) setListenAddr(resolved.listenAddr);

			setAuthError(null);
			setProvidersError(null);
			setProvidersResult(null);
			setCheckingProviders(true);
			setActionLoading(true);

			try {
				await startClient({
					server_addr: targetServerAddr,
					transport: targetTransport,
					auth_token: authToken || "",
					listen_addr: resolved.listenAddr || listenAddr,
					fake_lan_broadcast: fakeLanBroadcast,
					profile_id: selectedProfileId || undefined,
					profile_name: resolved.name || profileName || undefined,
				}).catch((err) => {
					console.warn("Tunnel client start attempt:", err);
				});

				const latestStatus = await waitForClientConnected();
				fetchLogs();

				if (authToken && latestStatus?.state === "connected") {
					const session = await refreshSession();
					if (session?.authenticated) {
						setProvidersResult(null);
						setProvidersError(null);
						return;
					}
				}

				let providers: AuthProvidersResponse | null = null;
				let probeErr: string | null = null;
				try {
					providers = await getAuthProviders(TUNNEL_ADMIN_CONNECTION);
				} catch (err) {
					probeErr = err instanceof Error ? err.message : m.client_probe_failed();
					if (resolved.managementUrl) {
						try {
							providers = await getAuthProviders(normalizeBaseUrl(resolved.managementUrl));
							setAuthServerUrl(normalizeBaseUrl(resolved.managementUrl));
							probeErr = null;
						} catch (fallbackErr) {
							probeErr = fallbackErr instanceof Error ? fallbackErr.message : probeErr;
						}
					}
				}
				const hasValidProviders = Boolean(
					providers &&
					(providers.github_enabled || (providers.providers && providers.providers.length > 0)),
				);
				if (hasValidProviders) {
					setProvidersResult(providers);
					setProvidersError(null);
				} else {
					setProvidersResult(providers);
					setProvidersError(probeErr);
				}
			} catch (err) {
				setError(err instanceof Error ? err.message : m.client_connection_failed());
				setProvidersResult(null);
			} finally {
				setCheckingProviders(false);
				setActionLoading(false);
			}
		},
		[
			authToken,
			fakeLanBroadcast,
			fetchLogs,
			listenAddr,
			prismLink,
			profileName,
			refreshSession,
			selectedProfileId,
			serverAddr,
			setListenAddr,
			setProfileName,
			setServerAddr,
			setTransport,
			waitForClientConnected,
		],
	);

	// Listen for Deep Link OAuth and Profile events
	useEffect(() => {
		const handleExchangeStart = () => {
			setOauthWaitingCallback(false);
			setOauthExchanging(true);
			setAuthError(null);
		};

		const handleExchangeError = (event: Event) => {
			const customEvent = event as CustomEvent<{ error?: string }>;
			setOauthWaitingCallback(false);
			setOauthExchanging(false);
			setOauthLoading(false);
			setAuthError(customEvent.detail?.error || m.client_authorization_failed());
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
			if (token) {
				void finalizeLogin(token, {
					token_id,
					user_id: userId,
					username,
					role,
					expires_at: expires_at_unix_ms,
				});
			}
		};

		const handleDeepLinkProfile = (event: Event) => {
			const customEvent = event as CustomEvent<Partial<ClientProfile>>;
			const p = customEvent.detail;
			if (p?.server_addr) {
				setServerAddr(p.server_addr);
				prismLink.setRemoteLinkInput(p.server_addr);
				if (p.transport) setTransport(p.transport);
				if (p.name) setProfileName(p.name);
				if (p.listen_addr) setListenAddr(p.listen_addr);
				if (p.auth_token) setAuthToken(p.auth_token);
				if (typeof p.fake_lan_broadcast === "boolean") {
					setFakeLanBroadcast(p.fake_lan_broadcast);
				}
			}
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
	}, [
		finalizeLogin,
		prismLink,
		setFakeLanBroadcast,
		setListenAddr,
		setProfileName,
		setServerAddr,
		setTransport,
	]);

	const value = useMemo<ClientContextValue>(
		() => ({
			// Status
			status: clientStatus.status,
			cumulativeStats: clientStatus.cumulativeStats,
			statsViewMode: clientStatus.statsViewMode,
			setStatsViewMode: clientStatus.setStatsViewMode,
			throughputSamples: clientStatus.throughputSamples,
			uptimeSeconds: clientStatus.uptimeSeconds,
			formatUptime: clientStatus.formatUptime,
			isRunning: clientStatus.isRunning,
			isConnected: clientStatus.isConnected,
			isConnecting: clientStatus.isConnecting,
			rawBytes: clientStatus.rawBytes,
			wireBytes: clientStatus.wireBytes,
			savedRatio: clientStatus.savedRatio,

			// Actions
			actionLoading,
			error,
			setError,
			copied: prismLink.copied,
			copyText: prismLink.copyText,
			handleConnect,
			handleDisconnect,
			handleToggleTunnel,
			handleResetStats,
			handleShareLink: prismLink.handleShareLink,

			// Profiles
			profiles: clientProfiles.profiles,
			selectedProfileId: clientProfiles.selectedProfileId,
			profileName: clientProfiles.profileName,
			setProfileName: clientProfiles.setProfileName,
			serverAddr: clientProfiles.serverAddr,
			setServerAddr: clientProfiles.setServerAddr,
			transport: clientProfiles.transport,
			setTransport: clientProfiles.setTransport,
			authToken: clientProfiles.authToken,
			setAuthToken: clientProfiles.setAuthToken,
			listenAddr: clientProfiles.listenAddr,
			setListenAddr: clientProfiles.setListenAddr,
			fakeLanBroadcast: clientProfiles.fakeLanBroadcast,
			setFakeLanBroadcast: clientProfiles.setFakeLanBroadcast,
			autoConnectPanel: clientProfiles.autoConnectPanel,
			setAutoConnectPanel: clientProfiles.setAutoConnectPanel,
			autoConnect: clientProfiles.autoConnect,
			setAutoConnect: clientProfiles.setAutoConnect,
			managementUrl: clientProfiles.managementUrl,
			handleSelectProfile: clientProfiles.handleSelectProfile,
			handleSaveProfile: clientProfiles.handleSaveProfile,
			handleDeleteProfile: clientProfiles.handleDeleteProfile,

			// Remote link
			remoteLinkInput: prismLink.remoteLinkInput,
			setRemoteLinkInput: prismLink.setRemoteLinkInput,
			linkProtocol: prismLink.linkProtocol,
			setLinkProtocol: prismLink.setLinkProtocol,
			handleSelectProtocol: prismLink.handleSelectProtocol,
			handleAddressChange: prismLink.handleAddressChange,
			handleAddressPaste: prismLink.handleAddressPaste,
			handleAddressCopy: prismLink.handleAddressCopy,
			handleConnectFromLink,

			// Logs
			logs: clientLogs.logs,
			filteredLogs: clientLogs.filteredLogs,
			logFilterLevel: clientLogs.logFilterLevel,
			setLogFilterLevel: clientLogs.setLogFilterLevel,
			logSearchQuery: clientLogs.logSearchQuery,
			setLogSearchQuery: clientLogs.setLogSearchQuery,
			autoScrollLogs: clientLogs.autoScrollLogs,
			setAutoScrollLogs: clientLogs.setAutoScrollLogs,
			isAtBottom: clientLogs.isAtBottom,
			logsContainerRef: clientLogs.logsContainerRef,
			handleLogsScroll: clientLogs.handleLogsScroll,
			scrollToBottom: clientLogs.scrollToBottom,
			handleClearLogs: clientLogs.handleClearLogs,
			handleCopyAllLogs: clientLogs.handleCopyAllLogs,

			// Login modal & auth
			loginModalOpen,
			setLoginModalOpen,
			checkingProviders,
			providersResult,
			setProvidersResult,
			providersError,
			setProvidersError,
			authServerUrl,
			setAuthServerUrl,
			authError,
			setAuthError,
			oauthLoading,
			oauthWaitingCallback,
			setOauthWaitingCallback,
			oauthExchanging,
			manualCallbackInput,
			setManualCallbackInput,
			handleManualOAuthCallback,
			startGitHubAuthWithUrl,
			handleRedetectProviders,
			loginAdminUnlocked,

			// Import modal
			importModalOpen: prismLink.importModalOpen,
			setImportModalOpen: prismLink.setImportModalOpen,
			importUrl: prismLink.importUrl,
			setImportUrl: prismLink.setImportUrl,
			importError: prismLink.importError,
			setImportError: prismLink.setImportError,
			handleImportLink: prismLink.handleImportLink,
		}),
		[
			clientStatus,
			clientProfiles,
			prismLink,
			clientLogs,
			actionLoading,
			error,
			loginModalOpen,
			checkingProviders,
			providersResult,
			setProvidersResult,
			providersError,
			authServerUrl,
			authError,
			oauthLoading,
			oauthWaitingCallback,
			oauthExchanging,
			manualCallbackInput,
			loginAdminUnlocked,
			handleConnect,
			handleDisconnect,
			handleToggleTunnel,
			handleResetStats,
			startGitHubAuthWithUrl,
			handleRedetectProviders,
			handleConnectFromLink,
			handleManualOAuthCallback,
		],
	);

	return <ClientContext.Provider value={value}>{children}</ClientContext.Provider>;
}

export function useClient() {
	const val = useContext(ClientContext);
	return val || DEFAULT_CLIENT_CONTEXT;
}
