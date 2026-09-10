import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";

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
import { deriveManagementUrl, normalizeBaseUrl } from "@/lib/panelConnection";
import { useAdminSession } from "@/lib/admin/adminSession";
import { SUPPORTED_LINK_PROTOCOLS, resolveRemoteConnection } from "@/lib/prismLink";
import type {
	AuthProvidersResponse,
	ClientContextValue,
	ClientProfile,
	UserRecord,
} from "@/types/client";
import { m } from "@/paraglide/messages";

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
	profileName: "Default Realm",
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
	const { saveConnection } = useAdminSession();

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
			fetchStatus();
			fetchLogs();
			fetchClientConfigData();
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
		fetchStatus,
		listenAddr,
		profileName,
		selectedProfileId,
		serverAddr,
		transport,
	]);

	const handleDisconnect = useCallback(async () => {
		setActionLoading(true);
		setError(null);
		try {
			await stopClient();
			fetchStatus();
			fetchLogs();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setActionLoading(false);
		}
	}, [fetchLogs, fetchStatus]);

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

	// Start GitHub OAuth with target management URL via browser + deep link
	const startGitHubAuthWithUrl = useCallback(
		async (targetAuthUrl: string, targetServerAddr?: string) => {
			setAuthError(null);
			setOauthLoading(true);
			try {
				const norm = normalizeBaseUrl(targetAuthUrl);
				const nextServer = targetServerAddr || serverAddr;
				if (targetServerAddr) {
					setServerAddr(nextServer);
				}
				if (typeof window !== "undefined") {
					window.localStorage.setItem("prism_pending_auth_url", norm);
					window.sessionStorage.setItem("prism_pending_auth_url", norm);
				}
				const res = await getGitHubLoginUrl({ baseUrl: norm, token: "" });
				if (res.url) {
					setOauthWaitingCallback(true);
					await openExternalUrl(res.url);
				}
			} catch (err) {
				setAuthError(err instanceof Error ? err.message : String(err));
				setOauthWaitingCallback(false);
			} finally {
				setOauthLoading(false);
			}
		},
		[serverAddr, setServerAddr],
	);

	const handleRedetectProviders = useCallback(
		async (overrideUrl?: string) => {
			const target = (overrideUrl ?? authServerUrl).trim() || status?.admin_url;
			if (!target) return;
			setCheckingProviders(true);
			setProvidersError(null);
			try {
				const norm = normalizeBaseUrl(target);
				const providers = await getAuthProviders(norm);
				setProvidersResult(providers);
				setAuthServerUrl(norm);
			} catch (err) {
				setProvidersError(err instanceof Error ? err.message : m.client_probe_failed());
			} finally {
				setCheckingProviders(false);
			}
		},
		[authServerUrl, status?.admin_url],
	);

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
				setAuthToken(deep.token);
				if (deep.role?.toLowerCase() === "admin") {
					setLoginAdminUnlocked(true);
				}
				saveClientConfig({
					active_profile_id: selectedProfileId || null,
					active_config: {
						server_addr: serverAddr,
						transport,
						auth_token: deep.token,
						listen_addr: listenAddr,
						fake_lan_broadcast: fakeLanBroadcast,
						auto_connect_panel: autoConnectPanel,
					},
				}).catch(() => {});
				setOauthWaitingCallback(false);
				setManualCallbackInput("");
				void startClient({
					server_addr: serverAddr,
					transport,
					auth_token: deep.token,
					listen_addr: listenAddr,
					fake_lan_broadcast: fakeLanBroadcast,
					profile_id: selectedProfileId || undefined,
					profile_name: profileName || undefined,
				}).then(() => {
					fetchStatus();
					fetchLogs();
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
				const candidateUrls: string[] = [];
				if (customOrigin && !candidateUrls.includes(customOrigin)) {
					candidateUrls.push(customOrigin);
				}
				if (typeof window !== "undefined") {
					const fromLocal = window.localStorage.getItem("prism_pending_auth_url");
					const fromSession = window.sessionStorage.getItem("prism_pending_auth_url");
					if (fromLocal && !candidateUrls.includes(fromLocal)) candidateUrls.push(fromLocal);
					if (fromSession && !candidateUrls.includes(fromSession)) candidateUrls.push(fromSession);
				}
				if (status?.admin_url && !candidateUrls.includes(status.admin_url)) {
					candidateUrls.push(status.admin_url);
				}
				if (authServerUrl && !candidateUrls.includes(authServerUrl)) {
					candidateUrls.push(authServerUrl);
				}
				if (serverAddr) {
					const derived = deriveManagementUrl(serverAddr);
					if (derived && !candidateUrls.includes(derived)) {
						candidateUrls.push(derived);
					}
				}
				if (!candidateUrls.includes("http://127.0.0.1:18080")) {
					candidateUrls.push("http://127.0.0.1:18080");
				}

				let res: { token: string; user: UserRecord; token_id: string } | null = null;
				let activeUrl = candidateUrls[0];
				let lastErr: unknown = null;
				for (const u of candidateUrls) {
					try {
						res = await exchangeGitHubCode({ baseUrl: normalizeBaseUrl(u), token: "" }, code);
						activeUrl = u;
						break;
					} catch (err) {
						lastErr = err;
					}
				}

				if (!res) {
					throw new Error(
						lastErr instanceof Error
							? lastErr.message
							: m.client_github_exchange_failed(),
					);
				}

				setAuthToken(res.token);
				if (res.user?.role?.toLowerCase() === "admin") {
					setLoginAdminUnlocked(true);
				}
				saveClientConfig({
					active_profile_id: selectedProfileId || null,
					active_config: {
						server_addr: serverAddr,
						transport,
						auth_token: res.token,
						listen_addr: listenAddr,
						fake_lan_broadcast: fakeLanBroadcast,
						auto_connect_panel: autoConnectPanel,
					},
				}).catch(() => {});
				if (autoConnectPanel && activeUrl) {
					saveConnection({ baseUrl: normalizeBaseUrl(activeUrl), token: res.token });
				}
				setLoginModalOpen(false);
				setOauthWaitingCallback(false);
				setOauthExchanging(false);
				setAuthError(null);
				void startClient({
					server_addr: serverAddr,
					transport,
					auth_token: res.token,
					listen_addr: listenAddr,
					fake_lan_broadcast: fakeLanBroadcast,
					profile_id: selectedProfileId || undefined,
					profile_name: profileName || undefined,
				}).then(() => {
					fetchStatus();
					fetchLogs();
				});
				prismLink.setRemoteLinkInput("");
				setManualCallbackInput("");
			} catch (err) {
				setAuthError(err instanceof Error ? err.message : String(err));
			} finally {
				setActionLoading(false);
				setCheckingProviders(false);
				setOauthExchanging(false);
			}
		},
		[
			authServerUrl,
			autoConnectPanel,
			fakeLanBroadcast,
			fetchLogs,
			fetchStatus,
			listenAddr,
			prismLink,
			profileName,
			saveConnection,
			selectedProfileId,
			serverAddr,
			setAuthToken,
			status?.admin_url,
			transport,
		],
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

				let bridgeAdminUrl = status?.admin_url || null;
				let latestStatus = null;
				for (let i = 0; i < 15; i++) {
					const st = await getClientStatus().catch(() => null);
					if (st) {
						latestStatus = st;
						if (st.admin_url) {
							bridgeAdminUrl = st.admin_url;
							setStatus(st);
							break;
						}
						if (st.state === "connected" && st.admin_url) {
							bridgeAdminUrl = st.admin_url;
							setStatus(st);
							break;
						}
					}
					await new Promise((resolve) => setTimeout(resolve, 200));
				}

				if (latestStatus) {
					setStatus(latestStatus);
				}
				fetchLogs();

				const candidateUrls: string[] = [];
				if (bridgeAdminUrl) {
					candidateUrls.push(normalizeBaseUrl(bridgeAdminUrl));
				}
				if (resolved.managementUrl) {
					const normManagement = normalizeBaseUrl(resolved.managementUrl);
					if (!candidateUrls.includes(normManagement)) {
						candidateUrls.push(normManagement);
					}
				}

				let providers: AuthProvidersResponse | null = null;
				let successfulUrl = candidateUrls[0] || resolved.managementUrl;

				for (const url of candidateUrls) {
					try {
						const res = await getAuthProviders(url);
						if (res.providers && res.providers.length > 0) {
							providers = res;
							successfulUrl = url;
							break;
						}
						if (res.github_enabled) {
							providers = res;
							successfulUrl = url;
							break;
						}
					} catch {
						// try next candidate
					}
				}

				if (!providers && candidateUrls.length > 0) {
					try {
						providers = await getAuthProviders(candidateUrls[0]);
						successfulUrl = candidateUrls[0];
					} catch {
						// Remote node may not have management auth service; keep silent
					}
				}

				setAuthServerUrl(successfulUrl);
				const hasValidProviders = Boolean(
					providers &&
					(providers.github_enabled || (providers.providers && providers.providers.length > 0)),
				);
				if (hasValidProviders) {
					setProvidersResult(providers);
				} else {
					setProvidersResult(null);
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
			authServerUrl,
			authToken,
			autoConnectPanel,
			fakeLanBroadcast,
			fetchLogs,
			fetchStatus,
			listenAddr,
			prismLink,
			profileName,
			saveConnection,
			selectedProfileId,
			serverAddr,
			setStatus,
			setAuthToken,
			setListenAddr,
			setProfileName,
			setServerAddr,
			setTransport,
			status?.admin_url,
			transport,
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
			}>;
			const { token, role } = customEvent.detail;
			setOauthWaitingCallback(false);
			setOauthExchanging(false);
			setOauthLoading(false);
			if (token) {
				setAuthToken(token);
				if (role?.toLowerCase() === "admin") {
					setLoginAdminUnlocked(true);
				}
				saveClientConfig({
					active_profile_id: selectedProfileId || null,
					active_config: {
						server_addr: serverAddr,
						transport,
						auth_token: token,
						listen_addr: listenAddr,
						fake_lan_broadcast: fakeLanBroadcast,
						auto_connect_panel: autoConnectPanel,
					},
				}).catch(() => {});
				if (autoConnectPanel && authServerUrl) {
					saveConnection({ baseUrl: normalizeBaseUrl(authServerUrl), token });
				}
				setLoginModalOpen(false);
				setAuthError(null);
				void startClient({
					server_addr: serverAddr,
					transport,
					auth_token: token,
					listen_addr: listenAddr,
					fake_lan_broadcast: fakeLanBroadcast,
					profile_id: selectedProfileId || undefined,
					profile_name: profileName || undefined,
				}).then(() => {
					fetchStatus();
					fetchLogs();
				});
				prismLink.setRemoteLinkInput("");
				setManualCallbackInput("");
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
		authServerUrl,
		autoConnectPanel,
		fakeLanBroadcast,
		fetchLogs,
		fetchStatus,
		listenAddr,
		prismLink,
		profileName,
		saveConnection,
		selectedProfileId,
		serverAddr,
		setAuthToken,
		setFakeLanBroadcast,
		setListenAddr,
		setProfileName,
		setServerAddr,
		setTransport,
		transport,
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
