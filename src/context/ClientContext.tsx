import {
	createContext,
	useCallback,
	useContext,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";

import { openExternalUrl } from "@/lib/desktopWindow";
import {
	type AuthProvidersResponse,
	type ClientLogEntry,
	type ClientProfile,
	type ClientStatusResponse,
	type CumulativeStats,
	type UserRecord,
	clearClientLogs,
	exchangeGitHubCode,
	getAuthProviders,
	getClientConfig,
	getClientLogs,
	getClientProfiles,
	getClientStatus,
	getGitHubLoginUrl,
	getHealth,
	resetClientStats,
	saveClientConfig,
	saveClientProfiles,
	startClient,
	stopClient,
} from "@/lib/managementApi";
import { parseDeepLink } from "@/lib/deepLink";
import { deriveManagementUrl, normalizeBaseUrl } from "@/lib/panelConnection";
import { usePanelSession } from "@/lib/panelSession";
import {
	SUPPORTED_LINK_PROTOCOLS,
	encodePrismLink,
	extractProtocolAndAddress,
	parsePrismLink,
	resolveRemoteConnection,
} from "@/lib/prismLink";
import { usePolling } from "@/lib/usePolling";

export interface ClientContextValue {
	// Status & Metrics
	status: ClientStatusResponse | null;
	cumulativeStats: CumulativeStats | null;
	statsViewMode: "session" | "lifetime";
	setStatsViewMode: (mode: "session" | "lifetime") => void;
	throughputSamples: number[];
	uptimeSeconds: number;
	formatUptime: (seconds: number) => string;
	isRunning: boolean;
	isConnected: boolean;
	isConnecting: boolean;
	rawBytes: number;
	wireBytes: number;
	savedRatio: number;

	// Actions & State
	actionLoading: boolean;
	error: string | null;
	setError: (err: string | null) => void;
	copied: string | null;
	copyText: (text: string, id: string) => void;
	handleConnect: () => Promise<void>;
	handleDisconnect: () => Promise<void>;
	handleToggleTunnel: () => void;
	handleResetStats: () => Promise<void>;
	handleShareLink: () => void;

	// Profiles & Active Form
	profiles: ClientProfile[];
	selectedProfileId: string;
	profileName: string;
	setProfileName: (val: string) => void;
	serverAddr: string;
	setServerAddr: (val: string) => void;
	transport: string;
	setTransport: (val: string) => void;
	authToken: string;
	setAuthToken: (val: string) => void;
	listenAddr: string;
	setListenAddr: (val: string) => void;
	fakeLanBroadcast: boolean;
	setFakeLanBroadcast: (val: boolean) => void;
	autoConnectPanel: boolean;
	setAutoConnectPanel: (val: boolean) => void;
	managementUrl: string;
	handleSelectProfile: (id: string) => void;
	handleSaveProfile: () => Promise<void>;
	handleDeleteProfile: (id: string) => Promise<void>;

	// Remote Link Input
	remoteLinkInput: string;
	setRemoteLinkInput: (val: string) => void;
	linkProtocol: string;
	setLinkProtocol: (proto: string) => void;
	handleSelectProtocol: (proto: string) => void;
	handleAddressChange: (val: string) => void;
	handleAddressPaste: (e: React.ClipboardEvent<HTMLInputElement>) => void;
	handleAddressCopy: (e: React.ClipboardEvent<HTMLInputElement>) => void;
	handleConnectFromLink: (customLink?: string) => Promise<void>;

	// Logs
	logs: ClientLogEntry[];
	filteredLogs: ClientLogEntry[];
	logFilterLevel: string;
	setLogFilterLevel: (level: string) => void;
	logSearchQuery: string;
	setLogSearchQuery: (q: string) => void;
	autoScrollLogs: boolean;
	setAutoScrollLogs: (enabled: boolean) => void;
	isAtBottom: boolean;
	logsContainerRef: React.RefObject<HTMLDivElement | null>;
	handleLogsScroll: () => void;
	scrollToBottom: (smooth?: boolean) => void;
	handleClearLogs: () => Promise<void>;
	handleCopyAllLogs: () => void;

	// Modals & OAuth
	loginModalOpen: boolean;
	setLoginModalOpen: (open: boolean) => void;
	checkingProviders: boolean;
	providersResult: AuthProvidersResponse | null;
	providersError: string | null;
	setProvidersError: (err: string | null) => void;
	authServerUrl: string;
	setAuthServerUrl: (url: string) => void;
	authError: string | null;
	setAuthError: (err: string | null) => void;
	oauthLoading: boolean;
	oauthWaitingCallback: boolean;
	setOauthWaitingCallback: (waiting: boolean) => void;
	oauthExchanging: boolean;
	manualCallbackInput: string;
	setManualCallbackInput: (val: string) => void;
	startGitHubAuthWithUrl: (targetAuthUrl: string, targetServerAddr?: string) => Promise<void>;
	handleRedetectProviders: (overrideUrl?: string) => Promise<void>;
	loginAdminUnlocked: boolean;

	importModalOpen: boolean;
	setImportModalOpen: (open: boolean) => void;
	importUrl: string;
	setImportUrl: (url: string) => void;
	importError: string | null;
	setImportError: (err: string | null) => void;
	handleImportLink: () => void;
}

const ClientContext = createContext<ClientContextValue | null>(null);

export function ClientProvider({ children }: { children: React.ReactNode }) {
	const { connection, saveConnection } = usePanelSession();

	const [status, setStatus] = useState<ClientStatusResponse | null>(null);
	const [profiles, setProfiles] = useState<ClientProfile[]>([]);
	const [selectedProfileId, setSelectedProfileId] = useState<string>("");
	const [actionLoading, setActionLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [copied, setCopied] = useState<string | null>(null);

	// Remote link input for one-click device flow
	const [remoteLinkInput, setRemoteLinkInput] = useState("");
	const [linkProtocol, setLinkProtocol] = useState<string>("quic://");
	const [loginAdminUnlocked, setLoginAdminUnlocked] = useState(false);

	// Throughput sparkline history
	const [throughputSamples, setThroughputSamples] = useState<number[]>([
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
	]);
	const prevWireRef = useRef(0);

	// Cumulative lifetime stats state
	const [cumulativeStats, setCumulativeStats] = useState<CumulativeStats | null>(null);
	const [statsViewMode, setStatsViewMode] = useState<"session" | "lifetime">("session");
	const configLoadedRef = useRef(false);
	const autoSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

	// Form / profile config state
	const [serverAddr, setServerAddr] = useState("127.0.0.1:7000");
	const [transport, setTransport] = useState("quic");
	const [authToken, setAuthToken] = useState("");
	const [listenAddr, setListenAddr] = useState("127.0.0.1:25565");
	const [fakeLanBroadcast, setFakeLanBroadcast] = useState(true);
	const [profileName, setProfileName] = useState("Default Realm");

	// Auto-connect management panel state
	const [autoConnectPanel, setAutoConnectPanel] = useState(true);
	const managementUrl = useMemo(
		() => deriveManagementUrl(serverAddr) || "http://127.0.0.1:8080",
		[serverAddr],
	);

	// Uptime timer state
	const [uptimeSeconds, setUptimeSeconds] = useState(0);

	// Logs state
	const [logs, setLogs] = useState<ClientLogEntry[]>([]);
	const [logFilterLevel, setLogFilterLevel] = useState<string>("ALL");
	const [logSearchQuery, setLogSearchQuery] = useState("");
	const [autoScrollLogs, setAutoScrollLogs] = useState(true);
	const logsContainerRef = useRef<HTMLDivElement | null>(null);
	const [isAtBottom, setIsAtBottom] = useState(true);
	const isAtBottomRef = useRef(true);

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

	// Import modal state
	const [importModalOpen, setImportModalOpen] = useState(false);
	const [importUrl, setImportUrl] = useState("");
	const [importError, setImportError] = useState<string | null>(null);

	// Fetch status
	const fetchStatus = useCallback(() => {
		getClientStatus()
			.then((resp) => {
				setStatus(resp);
				if (resp.cumulative_stats) {
					setCumulativeStats(resp.cumulative_stats);
				}
			})
			.catch((err) => {
				console.debug("Failed to fetch client status:", err);
			});
	}, []);

	// Fetch full client configuration from persistent redb on mount or after updates
	const fetchClientConfigData = useCallback(() => {
		getClientConfig()
			.then((resp) => {
				setProfiles(resp.profiles);
				if (resp.cumulative_stats) {
					setCumulativeStats(resp.cumulative_stats);
				}

				if (!configLoadedRef.current) {
					configLoadedRef.current = true;
					if (resp.active_config) {
						setProfileName(resp.active_config.profile_name || "Default Realm");
						const sAddr = resp.active_config.server_addr || "127.0.0.1:7000";
						setServerAddr(sAddr);
						setRemoteLinkInput((prev) => prev || sAddr);
						setTransport(resp.active_config.transport || "quic");
						setAuthToken(resp.active_config.auth_token || "");
						setListenAddr(resp.active_config.listen_addr || "127.0.0.1:25565");
						setFakeLanBroadcast(resp.active_config.fake_lan_broadcast ?? true);
						setAutoConnectPanel(resp.active_config.auto_connect_panel ?? true);
					}
					if (resp.active_profile_id) {
						setSelectedProfileId(resp.active_profile_id);
					} else if (resp.profiles.length > 0) {
						setSelectedProfileId(resp.profiles[0].id);
					}
				}
			})
			.catch(() => {
				getClientProfiles()
					.then((list) => {
						setProfiles(list);
						if (list.length > 0 && !selectedProfileId && !configLoadedRef.current) {
							configLoadedRef.current = true;
							const first = list[0];
							setSelectedProfileId(first.id);
							setProfileName(first.name);
							setServerAddr(first.server_addr);
							setTransport(first.transport);
							setAuthToken(first.auth_token);
							setListenAddr(first.listen_addr);
							setFakeLanBroadcast(first.fake_lan_broadcast);
						}
					})
					.catch(() => {});
			});
	}, [selectedProfileId]);

	// Fetch logs with deduplication to avoid unnecessary re-renders
	const fetchLogs = useCallback(() => {
		getClientLogs(300)
			.then((entries) => {
				setLogs((prev) => {
					if (prev.length === entries.length) {
						const prevLast = prev[prev.length - 1];
						const newLast = entries[entries.length - 1];
						if (
							(!prevLast && !newLast) ||
							(prevLast &&
								newLast &&
								prevLast.timestamp === newLast.timestamp &&
								prevLast.message === newLast.message)
						) {
							return prev;
						}
					}
					return entries;
				});
			})
			.catch(() => {});
	}, []);

	useEffect(() => {
		fetchStatus();
		fetchClientConfigData();
		fetchLogs();
	}, [fetchStatus, fetchClientConfigData, fetchLogs]);

	// Debounced auto-save of active configuration to KV storage
	useEffect(() => {
		if (!configLoadedRef.current) return;
		if (autoSaveTimerRef.current) {
			clearTimeout(autoSaveTimerRef.current);
		}
		autoSaveTimerRef.current = setTimeout(() => {
			saveClientConfig({
				active_profile_id: selectedProfileId || null,
				active_config: {
					server_addr: serverAddr,
					transport,
					auth_token: authToken,
					listen_addr: listenAddr,
					fake_lan_broadcast: fakeLanBroadcast,
					auto_connect_panel: autoConnectPanel,
				},
			}).catch(() => {});
		}, 500);

		return () => {
			if (autoSaveTimerRef.current) {
				clearTimeout(autoSaveTimerRef.current);
			}
		};
	}, [
		selectedProfileId,
		serverAddr,
		transport,
		authToken,
		listenAddr,
		fakeLanBroadcast,
		autoConnectPanel,
	]);

	// Poll status frequently
	usePolling(fetchStatus, 1500, true);

	// Poll logs frequently
	usePolling(fetchLogs, 1500, true);

	// Filter logs
	const filteredLogs = useMemo(() => {
		return logs.filter((l) => {
			if (logFilterLevel !== "ALL" && l.level.toUpperCase() !== logFilterLevel) {
				return false;
			}
			if (logSearchQuery.trim()) {
				const q = logSearchQuery.toLowerCase();
				return (
					l.message.toLowerCase().includes(q) ||
					l.target.toLowerCase().includes(q) ||
					l.level.toLowerCase().includes(q)
				);
			}
			return true;
		});
	}, [logs, logFilterLevel, logSearchQuery]);

	// Scroll management for logs container
	const handleLogsScroll = useCallback(() => {
		const container = logsContainerRef.current;
		if (!container) return;
		const distanceFromBottom =
			container.scrollHeight - container.scrollTop - container.clientHeight;
		const atBottom = distanceFromBottom <= 24;
		setIsAtBottom(atBottom);
		isAtBottomRef.current = atBottom;
	}, []);

	const scrollToBottom = useCallback((smooth = false) => {
		const container = logsContainerRef.current;
		if (!container) return;
		if (smooth) {
			container.scrollTo({ top: container.scrollHeight, behavior: "smooth" });
		} else {
			container.scrollTop = container.scrollHeight;
		}
		setIsAtBottom(true);
		isAtBottomRef.current = true;
	}, []);

	// Auto-scroll logs only when user is already at the bottom and auto-scroll is enabled
	useEffect(() => {
		if (autoScrollLogs && isAtBottomRef.current) {
			scrollToBottom(false);
		}
	}, [filteredLogs, autoScrollLogs, scrollToBottom]);

	// Connection duration timer
	useEffect(() => {
		let interval: ReturnType<typeof setInterval> | null = null;
		if (status?.state === "connected") {
			interval = setInterval(() => {
				setUptimeSeconds((prev) => prev + 1);
			}, 1000);
		} else {
			setUptimeSeconds(0);
		}
		return () => {
			if (interval) clearInterval(interval);
		};
	}, [status?.state]);

	// Throughput sample tracking for live waveform sparkline
	useEffect(() => {
		if (status?.running) {
			const currentWire = status.stats.wire_bytes;
			const delta = prevWireRef.current > 0 ? Math.max(0, currentWire - prevWireRef.current) : 0;
			prevWireRef.current = currentWire;
			setThroughputSamples((prev) => [...prev.slice(1), delta]);
		} else {
			prevWireRef.current = 0;
			setThroughputSamples([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
		}
	}, [status?.running, status?.stats.wire_bytes]);

	// Auto-connect management panel session when connected
	useEffect(() => {
		if (!autoConnectPanel) return;
		if (!status?.running || status.state !== "connected") return;

		const targetUrl =
			status?.admin_url?.trim() ||
			managementUrl.trim() ||
			deriveManagementUrl(serverAddr || status.server_addr);
		if (!targetUrl) return;

		if (connection?.baseUrl === targetUrl && connection?.token === authToken.trim()) {
			return;
		}

		getHealth({ baseUrl: targetUrl, token: authToken.trim() })
			.then(() => {
				saveConnection({ baseUrl: targetUrl, token: authToken.trim() });
			})
			.catch(() => {
				if (
					typeof window !== "undefined" &&
					window.location.origin &&
					connection?.baseUrl !== window.location.origin
				) {
					saveConnection({ baseUrl: window.location.origin, token: authToken.trim() });
				} else if (!connection) {
					saveConnection({ baseUrl: targetUrl, token: authToken.trim() });
				}
			});
	}, [
		autoConnectPanel,
		status?.running,
		status?.state,
		status?.server_addr,
		status?.admin_url,
		managementUrl,
		serverAddr,
		authToken,
		connection,
		saveConnection,
	]);

	// Start GitHub OAuth with target management URL via browser + deep link
	const startGitHubAuthWithUrl = async (targetAuthUrl: string, targetServerAddr?: string) => {
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
	};

	// Handle protocol dropdown selection
	const handleSelectProtocol = (newProtocol: string) => {
		setLinkProtocol(newProtocol);
		const matched = SUPPORTED_LINK_PROTOCOLS.find((p) => p.value === newProtocol);
		if (matched?.transport) {
			setTransport(matched.transport);
		}
	};

	// Handle address input change (auto detect and select protocol if present)
	const handleAddressChange = (val: string) => {
		const { protocol, address } = extractProtocolAndAddress(val);
		if (protocol) {
			const matched = SUPPORTED_LINK_PROTOCOLS.find(
				(p) => p.value.toLowerCase() === protocol.toLowerCase(),
			);
			if (matched) {
				setLinkProtocol(matched.value);
				if (matched.transport) {
					setTransport(matched.transport);
				}
			} else {
				setLinkProtocol(protocol);
			}
			setRemoteLinkInput(address);
		} else {
			setRemoteLinkInput(val);
		}
	};

	// Handle paste event (auto detect protocol or auth callback when pasting link)
	const handleAddressPaste = (e: React.ClipboardEvent<HTMLInputElement>) => {
		const text = e.clipboardData.getData("text");
		if (!text) return;
		const trimmedText = text.trim();
		if (
			trimmedText.toLowerCase().startsWith("prism://") &&
			(trimmedText.includes("code=") || trimmedText.includes("token="))
		) {
			const deep = parseDeepLink(trimmedText);
			if (deep.kind === "auth-code" || deep.kind === "auth") {
				e.preventDefault();
				setRemoteLinkInput(trimmedText);
				void handleConnectFromLink(trimmedText);
				return;
			}
		}

		const { protocol, address } = extractProtocolAndAddress(text);
		if (protocol) {
			e.preventDefault();
			const matched = SUPPORTED_LINK_PROTOCOLS.find(
				(p) => p.value.toLowerCase() === protocol.toLowerCase(),
			);
			if (matched) {
				setLinkProtocol(matched.value);
				if (matched.transport) {
					setTransport(matched.transport);
				}
			} else {
				setLinkProtocol(protocol);
			}
			setRemoteLinkInput(address);
		}
	};

	// Handle copy event on address input (ensure full protocol link is copied)
	const handleAddressCopy = (e: React.ClipboardEvent<HTMLInputElement>) => {
		const sel = window.getSelection()?.toString();
		if (sel && sel.trim() === remoteLinkInput.trim() && !remoteLinkInput.includes("://")) {
			e.preventDefault();
			e.clipboardData.setData("text/plain", `${linkProtocol}${remoteLinkInput}`);
		}
	};

	// Re-detect providers from a given or current URL
	const handleRedetectProviders = async (overrideUrl?: string) => {
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
			setProvidersError(err instanceof Error ? err.message : "探测失败，无法连接到远端服务");
		} finally {
			setCheckingProviders(false);
		}
	};

	// Connect from remote link: initiate tunnel client connection, query providers via in-band bridge or direct URL
	const handleConnectFromLink = async (customLink?: string) => {
		let raw = (customLink ?? remoteLinkInput).trim();
		if (!raw && serverAddr) {
			raw = `${linkProtocol}${serverAddr}`;
		} else if (raw && !raw.includes("://")) {
			raw = `${linkProtocol}${raw}`;
		}
		if (!raw) {
			setError("请输入远端链接或服务器地址");
			return;
		}

		// Handle direct OAuth code or Token callback links pasted by user
		const deep = parseDeepLink(raw);
		if (deep.kind === "auth-code") {
			setActionLoading(true);
			setOauthExchanging(true);
			setOauthWaitingCallback(false);
			setLoginModalOpen(true);
			setAuthError(null);
			setProvidersError(null);
			try {
				const candidateUrls: string[] = [];
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
						res = await exchangeGitHubCode({ baseUrl: normalizeBaseUrl(u), token: "" }, deep.code);
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
							: "GitHub 授权码兑换凭证失败，验证码可能已失效，请重新发起登录",
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
				setRemoteLinkInput("");
				setManualCallbackInput("");
			} catch (err) {
				setAuthError(err instanceof Error ? err.message : String(err));
			} finally {
				setActionLoading(false);
				setCheckingProviders(false);
				setOauthExchanging(false);
			}
			return;
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
			setLoginModalOpen(false);
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
			setRemoteLinkInput("");
			return;
		}

		const resolved = resolveRemoteConnection(raw);
		const targetServerAddr = resolved.serverAddr;
		const targetTransport = resolved.transport || "quic";
		setServerAddr(targetServerAddr);
		setTransport(targetTransport);
		const matched = SUPPORTED_LINK_PROTOCOLS.find((p) => p.transport === targetTransport);
		if (matched) setLinkProtocol(matched.value);
		if (resolved.name) setProfileName(resolved.name);
		if (resolved.listenAddr) setListenAddr(resolved.listenAddr);

		setAuthError(null);
		setProvidersError(null);
		setLoginModalOpen(true);
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
			let latestStatus: ClientStatusResponse | null = null;
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
				} catch (err) {
					setProvidersError(
						err instanceof Error ? err.message : "无法获取远端登录方式，请检查网络或服务端配置",
					);
				}
			}

			setAuthServerUrl(successfulUrl);
			if (providers) {
				setProvidersResult(providers);
			}
		} catch (err) {
			setProvidersError(
				err instanceof Error ? err.message : "连接远端或获取登录方式失败，请检查网络",
			);
		} finally {
			setCheckingProviders(false);
			setActionLoading(false);
		}
	};

	// Listen for Deep Link OAuth and Profile events
	useEffect(() => {
		const handleExchangeStart = () => {
			setOauthWaitingCallback(false);
			setOauthExchanging(true);
			setAuthError(null);
			setLoginModalOpen(true);
		};

		const handleExchangeError = (event: Event) => {
			const customEvent = event as CustomEvent<{ error?: string }>;
			setOauthWaitingCallback(false);
			setOauthExchanging(false);
			setOauthLoading(false);
			setAuthError(customEvent.detail?.error || "授权验证失败，请重试");
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
				setRemoteLinkInput("");
				setManualCallbackInput("");
			}
		};

		const handleDeepLinkProfile = (event: Event) => {
			const customEvent = event as CustomEvent<Partial<ClientProfile>>;
			const p = customEvent.detail;
			if (p?.server_addr) {
				setServerAddr(p.server_addr);
				setRemoteLinkInput(p.server_addr);
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
		listenAddr,
		profileName,
		saveConnection,
		selectedProfileId,
		serverAddr,
		transport,
	]);

	// Select Profile
	const handleSelectProfile = (id: string) => {
		setSelectedProfileId(id);
		const p = profiles.find((item) => item.id === id);
		if (p) {
			setProfileName(p.name);
			setServerAddr(p.server_addr);
			setTransport(p.transport);
			setAuthToken(p.auth_token);
			setListenAddr(p.listen_addr);
			setFakeLanBroadcast(p.fake_lan_broadcast);
			saveClientConfig({
				active_profile_id: id,
				active_config: {
					server_addr: p.server_addr,
					transport: p.transport,
					auth_token: p.auth_token,
					listen_addr: p.listen_addr,
					fake_lan_broadcast: p.fake_lan_broadcast,
					auto_connect_panel: autoConnectPanel,
				},
			}).catch(() => {});
		}
	};

	// Save Profile
	const handleSaveProfile = async () => {
		const existingIndex = profiles.findIndex(
			(p) => p.id === selectedProfileId || p.server_addr === serverAddr,
		);
		const id = selectedProfileId || `profile-${Date.now()}`;
		const newProfile: ClientProfile = {
			id,
			name: profileName || serverAddr,
			server_addr: serverAddr,
			transport,
			auth_token: authToken,
			listen_addr: listenAddr,
			fake_lan_broadcast: fakeLanBroadcast,
		};

		let updated: ClientProfile[];
		if (existingIndex >= 0) {
			updated = [...profiles];
			updated[existingIndex] = newProfile;
		} else {
			updated = [...profiles, newProfile];
		}

		setProfiles(updated);
		setSelectedProfileId(id);
		await saveClientProfiles(updated).catch(() => {});
		await saveClientConfig({
			active_profile_id: id,
			active_config: {
				server_addr: serverAddr,
				transport,
				auth_token: authToken,
				listen_addr: listenAddr,
				fake_lan_broadcast: fakeLanBroadcast,
				auto_connect_panel: autoConnectPanel,
			},
		}).catch(() => {});
	};

	// Delete Profile
	const handleDeleteProfile = async (id: string) => {
		const updated = profiles.filter((p) => p.id !== id);
		setProfiles(updated);
		const nextActiveId = selectedProfileId === id ? updated[0]?.id || "" : selectedProfileId;
		if (selectedProfileId === id) {
			setSelectedProfileId(nextActiveId);
			if (updated[0]) {
				const first = updated[0];
				setProfileName(first.name);
				setServerAddr(first.server_addr);
				setTransport(first.transport);
				setAuthToken(first.auth_token);
				setListenAddr(first.listen_addr);
				setFakeLanBroadcast(first.fake_lan_broadcast);
			}
		}
		await saveClientProfiles(updated).catch(() => {});
		await saveClientConfig({
			active_profile_id: nextActiveId || null,
		}).catch(() => {});
	};

	// Connect / Disconnect Handlers
	const handleConnect = async () => {
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
	};

	const handleDisconnect = async () => {
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
	};

	const handleResetStats = async () => {
		try {
			await resetClientStats();
			fetchStatus();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		}
	};

	// Toggle Connect Switch
	const handleToggleTunnel = () => {
		if (status?.running) {
			void handleDisconnect();
		} else {
			void handleConnect();
		}
	};

	// Clear Logs Handler
	const handleClearLogs = async () => {
		try {
			await clearClientLogs();
			setLogs([]);
		} catch (err) {
			console.error("Failed to clear logs:", err);
		}
	};

	// Copy all visible logs
	const handleCopyAllLogs = () => {
		const text = filteredLogs
			.map((l) => `[${l.timestamp}] [${l.level}] [${l.target}] ${l.message}`)
			.join("\n");
		navigator.clipboard.writeText(text);
		setCopied("all-logs");
		setTimeout(() => setCopied(null), 2000);
	};

	// Import Link
	const handleImportLink = () => {
		setImportError(null);
		const parsed = parsePrismLink(importUrl);
		if (!parsed || !parsed.server_addr) {
			setImportError("Invalid Prism link or server address format.");
			return;
		}

		if (parsed.name) setProfileName(parsed.name);
		setServerAddr(parsed.server_addr);
		if (parsed.transport) setTransport(parsed.transport);
		if (parsed.listen_addr) setListenAddr(parsed.listen_addr);
		if (parsed.fake_lan_broadcast !== undefined) {
			setFakeLanBroadcast(parsed.fake_lan_broadcast);
		}

		setImportModalOpen(false);
		setImportUrl("");
	};

	// Share Link
	const handleShareLink = () => {
		const link = encodePrismLink({
			name: profileName,
			server_addr: serverAddr,
			transport,
			auth_token: authToken,
			listen_addr: listenAddr,
			fake_lan_broadcast: fakeLanBroadcast,
		});

		navigator.clipboard.writeText(link);
		setCopied("share");
		setTimeout(() => setCopied(null), 2000);
	};

	const copyText = (text: string, id: string) => {
		navigator.clipboard.writeText(text);
		setCopied(id);
		setTimeout(() => setCopied(null), 2000);
	};

	const isRunning = status?.running ?? false;
	const isConnected = status?.state === "connected";
	const isConnecting = status?.state === "connecting";

	const rawBytes = status?.stats.raw_bytes ?? 0;
	const wireBytes = status?.stats.wire_bytes ?? 0;
	const savedRatio = (status?.stats.saved_ratio ?? 0) * 100;

	const formatUptime = (seconds: number) => {
		const hrs = Math.floor(seconds / 3600);
		const mins = Math.floor((seconds % 3600) / 60);
		const secs = seconds % 60;
		return `${hrs.toString().padStart(2, "0")}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
	};

	const value = useMemo<ClientContextValue>(
		() => ({
			status,
			cumulativeStats,
			statsViewMode,
			setStatsViewMode,
			throughputSamples,
			uptimeSeconds,
			formatUptime,
			isRunning,
			isConnected,
			isConnecting,
			rawBytes,
			wireBytes,
			savedRatio,

			actionLoading,
			error,
			setError,
			copied,
			copyText,
			handleConnect,
			handleDisconnect,
			handleToggleTunnel,
			handleResetStats,
			handleShareLink,

			profiles,
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
			setAutoConnectPanel,
			managementUrl,
			handleSelectProfile,
			handleSaveProfile,
			handleDeleteProfile,

			remoteLinkInput,
			setRemoteLinkInput,
			linkProtocol,
			setLinkProtocol,
			handleSelectProtocol,
			handleAddressChange,
			handleAddressPaste,
			handleAddressCopy,
			handleConnectFromLink,

			logs,
			filteredLogs,
			logFilterLevel,
			setLogFilterLevel,
			logSearchQuery,
			setLogSearchQuery,
			autoScrollLogs,
			setAutoScrollLogs,
			isAtBottom,
			logsContainerRef,
			handleLogsScroll,
			scrollToBottom,
			handleClearLogs,
			handleCopyAllLogs,

			loginModalOpen,
			setLoginModalOpen,
			checkingProviders,
			providersResult,
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
			startGitHubAuthWithUrl,
			handleRedetectProviders,
			loginAdminUnlocked,

			importModalOpen,
			setImportModalOpen,
			importUrl,
			setImportUrl,
			importError,
			setImportError,
			handleImportLink,
		}),
		[
			status,
			cumulativeStats,
			statsViewMode,
			throughputSamples,
			uptimeSeconds,
			isRunning,
			isConnected,
			isConnecting,
			rawBytes,
			wireBytes,
			savedRatio,
			actionLoading,
			error,
			copied,
			profiles,
			selectedProfileId,
			profileName,
			serverAddr,
			transport,
			authToken,
			listenAddr,
			fakeLanBroadcast,
			autoConnectPanel,
			managementUrl,
			remoteLinkInput,
			linkProtocol,
			logs,
			filteredLogs,
			logFilterLevel,
			logSearchQuery,
			autoScrollLogs,
			isAtBottom,
			loginModalOpen,
			checkingProviders,
			providersResult,
			providersError,
			authServerUrl,
			authError,
			oauthLoading,
			oauthWaitingCallback,
			oauthExchanging,
			manualCallbackInput,
			loginAdminUnlocked,
			importModalOpen,
			importUrl,
			importError,
			handleConnect,
			handleDisconnect,
			handleToggleTunnel,
			handleResetStats,
			handleShareLink,
			handleSelectProfile,
			handleSaveProfile,
			handleDeleteProfile,
			handleSelectProtocol,
			handleAddressChange,
			handleAddressPaste,
			handleAddressCopy,
			handleConnectFromLink,
			handleLogsScroll,
			scrollToBottom,
			handleClearLogs,
			handleCopyAllLogs,
			startGitHubAuthWithUrl,
			handleRedetectProviders,
			handleImportLink,
		],
	);

	return <ClientContext.Provider value={value}>{children}</ClientContext.Provider>;
}

export function useClient() {
	const val = useContext(ClientContext);
	if (!val) {
		throw new Error("useClient must be used within a ClientProvider");
	}
	return val;
}
