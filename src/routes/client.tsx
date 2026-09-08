import { createFileRoute, Link, useLocation, useNavigate } from "@tanstack/react-router";
import {
	Activity,
	ArrowDown,
	Check,
	CheckCircle2,
	Copy,
	Download,
	Gamepad2,
	Plug,
	Plus,
	Power,
	Radio,
	RotateCcw,
	Search,
	Settings2,
	Share2,
	Terminal,
	Trash2,
	WifiOff,
	X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Github } from "@/components/icons/Github";

import { isDesktopApp, openExternalUrl } from "@/lib/desktopWindow";
import { formatBytes } from "@/lib/format";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardFooter,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
	type AuthProvidersResponse,
	type ClientLogEntry,
	type ClientProfile,
	type ClientStatusResponse,
	type CumulativeStats,
	clearClientLogs,
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
import { type PanelConnection, deriveManagementUrl, normalizeBaseUrl } from "@/lib/panelConnection";
import { usePanelSession } from "@/lib/panelSession";
import {
	SUPPORTED_LINK_PROTOCOLS,
	encodePrismLink,
	extractProtocolAndAddress,
	parsePrismLink,
	resolveRemoteConnection,
} from "@/lib/prismLink";
import { usePolling } from "@/lib/usePolling";
import { cn } from "@/lib/utils";

export const Route = createFileRoute("/client")({
	component: ClientDashboardPage,
});

function parsePort(listenAddr: string): string {
	const trimmed = listenAddr.trim();
	if (!trimmed) return "25565";
	if (trimmed.startsWith("[")) {
		const end = trimmed.indexOf("]");
		if (end !== -1) {
			const rest = trimmed.slice(end + 1);
			return rest.startsWith(":") ? rest.slice(1) || "25565" : "25565";
		}
	}
	const lastColon = trimmed.lastIndexOf(":");
	if (lastColon !== -1 && trimmed.indexOf(":") === lastColon) {
		return trimmed.slice(lastColon + 1) || "25565";
	}
	return "25565";
}

function getLoopbackTargetForService(idx: number, port: string): string {
	let ip = "127.0.0.1";
	if (idx <= 254) {
		ip = `127.0.0.${idx + 1}`;
	} else {
		const offset = idx - 255;
		const b = 1 + Math.floor(offset / 65536);
		if (b <= 7) {
			const rem = offset % 65536;
			const c = Math.floor(rem / 256);
			const d = rem % 256;
			ip = `127.${b}.${c}.${d}`;
		}
	}
	return port === "25565" ? ip : `${ip}:${port}`;
}

function ClientDashboardPage() {
	const location = useLocation();
	const navigate = useNavigate();
	const { connection, authSession, isAdmin, saveConnection, clearConnection } = usePanelSession();
	const isDesktop = useMemo(() => isDesktopApp(), []);

	const clientConnection = useMemo<PanelConnection>(
		() => ({
			baseUrl: isDesktop ? "http://127.0.0.1:8080" : "",
			token: "",
		}),
		[isDesktop],
	);

	const [status, setStatus] = useState<ClientStatusResponse | null>(null);
	const [profiles, setProfiles] = useState<ClientProfile[]>([]);
	const [selectedProfileId, setSelectedProfileId] = useState<string>("");
	const [actionLoading, setActionLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [copied, setCopied] = useState<string | null>(null);

	// Synchronize tab with URL search parameter ?tab=...
	const searchTab = useMemo(() => {
		return new URLSearchParams(location.search).get("tab");
	}, [location.search]);

	const currentTab = useMemo(() => {
		if (searchTab === "logs") return "logs";
		if (searchTab === "settings" || searchTab === "profiles") return "settings";
		return "overview";
	}, [searchTab]);

	const handleNavigateTab = useCallback(
		(tab: "overview" | "logs" | "settings") => {
			void navigate({
				to: "/client",
				search: tab === "overview" ? undefined : { tab },
			});
		},
		[navigate],
	);

	// Remote link input for one-click device flow
	const [remoteLinkInput, setRemoteLinkInput] = useState("");
	const [linkProtocol, setLinkProtocol] = useState<string>("quic://");
	const [copiedLink, setCopiedLink] = useState(false);
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

	// Login modal & auth state
	const [loginModalOpen, setLoginModalOpen] = useState(false);
	const [checkingProviders, setCheckingProviders] = useState(false);
	const [providersResult, setProvidersResult] = useState<AuthProvidersResponse | null>(null);
	const [providersError, setProvidersError] = useState<string | null>(null);
	const [authServerUrl, setAuthServerUrl] = useState("http://127.0.0.1:8080");
	const [authError, setAuthError] = useState<string | null>(null);
	const [oauthLoading, setOauthLoading] = useState(false);

	// Import modal state
	const [importModalOpen, setImportModalOpen] = useState(false);
	const [importUrl, setImportUrl] = useState("");
	const [importError, setImportError] = useState<string | null>(null);

	// Fetch status
	const fetchStatus = useCallback(() => {
		getClientStatus(clientConnection)
			.then((resp) => {
				setStatus(resp);
				if (resp.cumulative_stats) {
					setCumulativeStats(resp.cumulative_stats);
				}
			})
			.catch((err) => {
				console.debug("Failed to fetch client status:", err);
			});
	}, [clientConnection]);

	// Fetch full client configuration from persistent redb on mount or after updates
	const fetchClientConfigData = useCallback(() => {
		getClientConfig(clientConnection)
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
				getClientProfiles(clientConnection)
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
	}, [clientConnection, selectedProfileId]);

	// Fetch logs with deduplication to avoid unnecessary re-renders
	const fetchLogs = useCallback(() => {
		getClientLogs(clientConnection, 300)
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
	}, [clientConnection]);

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
			saveClientConfig(clientConnection, {
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
		clientConnection,
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

	// Poll logs frequently while on logs tab or when running
	usePolling(fetchLogs, 1500, currentTab === "logs" || status?.running === true);

	// Scroll management for logs container
	const handleLogsScroll = useCallback(() => {
		const container = logsContainerRef.current;
		if (!container) return;
		// Distance from bottom threshold (24px)
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
		if (currentTab !== "logs") return;
		if (autoScrollLogs && isAtBottomRef.current) {
			scrollToBottom(false);
		}
	}, [filteredLogs, autoScrollLogs, currentTab, scrollToBottom]);

	// When user opens/switches to logs tab, scroll to bottom if auto-scroll is enabled
	useEffect(() => {
		if (currentTab === "logs" && autoScrollLogs && isAtBottomRef.current) {
			const frame = requestAnimationFrame(() => {
				scrollToBottom(false);
			});
			return () => cancelAnimationFrame(frame);
		}
	}, [currentTab, autoScrollLogs, scrollToBottom]);

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
				window.sessionStorage.setItem("prism_pending_auth_url", norm);
			}
			const res = await getGitHubLoginUrl({ baseUrl: norm, token: "" });
			if (res.url) {
				await openExternalUrl(res.url);
			}
		} catch (err) {
			setAuthError(err instanceof Error ? err.message : String(err));
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

	// Handle paste event (auto detect protocol when pasting link)
	const handleAddressPaste = (e: React.ClipboardEvent<HTMLInputElement>) => {
		const text = e.clipboardData.getData("text");
		if (!text) return;
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

	// Connect from remote link: query providers and open select login method dialog
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

		const resolved = resolveRemoteConnection(raw);
		setServerAddr(resolved.serverAddr);
		if (resolved.transport) {
			setTransport(resolved.transport);
			const matched = SUPPORTED_LINK_PROTOCOLS.find((p) => p.transport === resolved.transport);
			if (matched) setLinkProtocol(matched.value);
		}
		if (resolved.name) setProfileName(resolved.name);
		if (resolved.listenAddr) setListenAddr(resolved.listenAddr);

		const targetAuthUrl = resolved.managementUrl;
		setAuthServerUrl(targetAuthUrl);
		setAuthError(null);
		setProvidersError(null);
		setLoginModalOpen(true);
		setCheckingProviders(true);

		try {
			const norm = normalizeBaseUrl(targetAuthUrl);
			const providers = await getAuthProviders(norm);
			setProvidersResult(providers);
		} catch (err) {
			setProvidersError(
				err instanceof Error ? err.message : "无法获取远端登录方式，请检查网络或服务端配置",
			);
			setProvidersResult({
				github_enabled: false,
				github_client_id: null,
				mode: "token",
				providers: [],
			});
		} finally {
			setCheckingProviders(false);
		}
	};

	// Listen for Deep Link OAuth and Profile events
	useEffect(() => {
		const handleDeepLinkAuth = (event: Event) => {
			const customEvent = event as CustomEvent<{
				token: string;
				userId?: string;
				username?: string;
				role?: string;
			}>;
			const { token, role } = customEvent.detail;
			if (token) {
				setAuthToken(token);
				if (role?.toLowerCase() === "admin") {
					setLoginAdminUnlocked(true);
				}
				saveClientConfig(clientConnection, {
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

		window.addEventListener("prism:deep-link-auth", handleDeepLinkAuth);
		window.addEventListener("prism:deep-link-profile", handleDeepLinkProfile);
		return () => {
			window.removeEventListener("prism:deep-link-auth", handleDeepLinkAuth);
			window.removeEventListener("prism:deep-link-profile", handleDeepLinkProfile);
		};
	}, [
		autoConnectPanel,
		clientConnection,
		fakeLanBroadcast,
		listenAddr,
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
			saveClientConfig(clientConnection, {
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
		await saveClientProfiles(clientConnection, updated).catch(() => {});
		await saveClientConfig(clientConnection, {
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
		await saveClientProfiles(clientConnection, updated).catch(() => {});
		await saveClientConfig(clientConnection, {
			active_profile_id: nextActiveId || null,
		}).catch(() => {});
	};

	// Connect / Disconnect Handlers
	const handleConnect = async () => {
		setActionLoading(true);
		setError(null);
		try {
			await startClient(clientConnection, {
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
			await stopClient(clientConnection);
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
			await resetClientStats(clientConnection);
			fetchStatus();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		}
	};

	// Toggle Connect Switch
	const handleToggleTunnel = () => {
		if (status?.running) {
			handleDisconnect();
		} else {
			handleConnect();
		}
	};

	// Clear Logs Handler
	const handleClearLogs = async () => {
		try {
			await clearClientLogs(clientConnection);
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

	// Format uptime
	const formatUptime = (seconds: number) => {
		const hrs = Math.floor(seconds / 3600);
		const mins = Math.floor((seconds % 3600) / 60);
		const secs = seconds % 60;
		return `${hrs.toString().padStart(2, "0")}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
	};

	return (
		<div className="flex h-full w-full flex-1 min-h-0 flex-col overflow-hidden bg-background text-foreground">
			{/* 1. Overview Page: 连接 (Connection) */}
			{currentTab === "overview" ? (
				<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-y-auto">
					{/* Page Header Bar */}
					<div className="flex flex-none select-none items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
						<div className="flex items-center gap-2 min-w-0">
							<Gamepad2 className="h-4 w-4 text-primary flex-none" />
							<div className="min-w-0">
								<h1 className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground">
									连接
								</h1>
								<p className="truncate text-[10px] text-muted-foreground hidden sm:block">
									远端节点连接、隧道运行状态与服务发现
								</p>
							</div>
							{isConnected ? (
								<span className="flex items-center gap-1 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-500 ring-1 ring-emerald-500/30">
									<span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-500" />
									ONLINE
								</span>
							) : isConnecting ? (
								<span className="flex items-center gap-1 rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] font-semibold text-amber-500 ring-1 ring-amber-500/30">
									<span className="h-1.5 w-1.5 animate-ping rounded-full bg-amber-500" />
									CONNECTING
								</span>
							) : (
								<span className="flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
									<span className="h-1.5 w-1.5 rounded-full bg-muted-foreground/50" />
									OFFLINE
								</span>
							)}
						</div>

						{/* Quick Controls */}
						<div className="flex items-center gap-1.5 flex-none">
							{profiles.length > 0 ? (
								<select
									aria-label="Select Profile"
									value={selectedProfileId}
									onChange={(e) => handleSelectProfile(e.target.value)}
									className="h-7 max-w-[120px] sm:max-w-[170px] truncate rounded-md border border-input bg-background px-2 text-xs font-medium text-foreground outline-none focus:ring-1 focus:ring-ring"
								>
									{profiles.map((p) => (
										<option key={p.id} value={p.id}>
											{p.name}
										</option>
									))}
								</select>
							) : null}

							{authSession?.authenticated ? (
								<Button
									size="sm"
									variant={isRunning ? "destructive" : "default"}
									disabled={actionLoading}
									onClick={handleToggleTunnel}
									className={cn(
										"h-7 px-3 text-xs font-bold gap-1 rounded-md flex-none shadow-xs",
										isRunning
											? "bg-emerald-600 hover:bg-emerald-700 text-white"
											: "bg-primary text-primary-foreground hover:bg-primary/90",
									)}
								>
									{actionLoading ? (
										<RotateCcw className="h-3.5 w-3.5 animate-spin" />
									) : (
										<Power className="h-3.5 w-3.5" />
									)}
									<span>{isRunning ? "已连接" : "启动连接"}</span>
								</Button>
							) : (
								<Button
									size="sm"
									variant="default"
									disabled={actionLoading || oauthLoading || checkingProviders}
									onClick={() => void handleConnectFromLink()}
									className="h-7 px-3 text-xs font-bold gap-1 rounded-md flex-none shadow-xs bg-primary text-primary-foreground hover:bg-primary/90 cursor-pointer"
								>
									{checkingProviders ? (
										<RotateCcw className="h-3.5 w-3.5 animate-spin" />
									) : (
										<Plug className="h-3.5 w-3.5" />
									)}
									<span>连接</span>
								</Button>
							)}

							<Button
								variant="outline"
								size="icon-xs"
								onClick={() => handleNavigateTab("settings")}
								title="前往隧道配置"
								className="h-7 w-7 text-xs flex-none cursor-pointer"
							>
								<Settings2 className="h-3.5 w-3.5 text-muted-foreground hover:text-foreground" />
							</Button>
						</div>
					</div>

					{error ? (
						<div className="flex flex-none items-center justify-between gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
							<span className="truncate">{error}</span>
							<Button
								variant="outline"
								size="xs"
								onClick={fetchStatus}
								className="h-5 text-[10px] px-1.5 flex-none"
							>
								Retry
							</Button>
						</div>
					) : null}

					{/* 极简连接远端卡片 (Connect to Remote Hero) */}
					{authSession?.authenticated ? (
						<div className="flex-none rounded-lg border border-border bg-card p-3 shadow-xs flex items-center justify-between gap-3">
							<div className="flex items-center gap-2.5 min-w-0">
								{authSession.avatar_url ? (
									<img
										src={authSession.avatar_url}
										alt={authSession.username || "User"}
										className="h-8 w-8 rounded-full object-cover ring-1 ring-border flex-none"
									/>
								) : (
									<div className="flex h-8 w-8 items-center justify-center rounded-full bg-primary/10 text-primary flex-none">
										<Github className="h-4 w-4" />
									</div>
								)}
								<div className="min-w-0">
									<div className="flex items-center gap-1.5">
										<span className="text-xs font-bold text-foreground truncate">
											{authSession.display_name || authSession.username || "已登录"}
										</span>
										{isAdmin ? (
											<Badge className="bg-primary/20 text-primary border-primary/30 text-[9px] px-1.5 py-0 h-4">
												管理员
											</Badge>
										) : (
											<Badge variant="secondary" className="text-[9px] px-1.5 py-0 h-4">
												成员
											</Badge>
										)}
									</div>
									<p className="text-[10px] font-mono text-muted-foreground truncate">
										远端节点: {serverAddr || "默认节点"}
									</p>
								</div>
							</div>

							<div className="flex items-center gap-2 flex-none">
								{isAdmin || loginAdminUnlocked ? (
									<Link to="/" className="text-[11px] font-bold text-primary hover:underline">
										进入控制台 &rarr;
									</Link>
								) : null}
								<Button
									variant="outline"
									size="xs"
									onClick={() => {
										clearConnection();
										setAuthToken("");
										if (status?.running) {
											void handleDisconnect();
										}
									}}
									className="h-7 text-xs px-2.5 text-muted-foreground hover:text-destructive cursor-pointer"
								>
									<span>退出登录</span>
								</Button>
							</div>
						</div>
					) : (
						<div className="flex-none rounded-lg border border-border bg-card p-3 shadow-xs space-y-2.5">
							<div className="flex items-center justify-between">
								<div className="flex items-center gap-1.5">
									<Radio className="h-4 w-4 text-primary" />
									<span className="text-xs font-bold text-foreground">连接远端节点与登录</span>
								</div>
								<span className="text-[11px] text-muted-foreground">
									服务端默认允许所有人连接；登录前仅限进行登录操作
								</span>
							</div>

							<div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
								<div className="relative flex-1 min-w-0 flex items-stretch">
									<select
										aria-label="选择连接协议"
										value={linkProtocol}
										onChange={(e) => handleSelectProtocol(e.target.value)}
										className="h-8 rounded-l-md rounded-r-none border border-r-0 border-input bg-muted/60 px-2 text-xs font-mono font-semibold text-foreground outline-none focus:ring-1 focus:ring-ring shrink-0 cursor-pointer hover:bg-muted transition-colors"
									>
										{SUPPORTED_LINK_PROTOCOLS.map((p) => (
											<option key={p.value} value={p.value}>
												{p.label}
											</option>
										))}
									</select>
									<div className="relative flex-1 min-w-0">
										<Input
											value={remoteLinkInput}
											onChange={(e) => handleAddressChange(e.target.value)}
											onPaste={handleAddressPaste}
											onCopy={handleAddressCopy}
											placeholder="play.example.com:7000 或 relay.example.com"
											className="h-8 text-xs font-mono rounded-l-none pr-14"
										/>
										{remoteLinkInput ? (
											<div className="absolute top-1/2 right-2 -translate-y-1/2 flex items-center gap-1">
												<button
													type="button"
													onClick={() => {
														const full = remoteLinkInput.includes("://")
															? remoteLinkInput
															: `${linkProtocol}${remoteLinkInput}`;
														navigator.clipboard.writeText(full);
														setCopiedLink(true);
														setTimeout(() => setCopiedLink(false), 1500);
													}}
													className="text-muted-foreground hover:text-foreground p-0.5 cursor-pointer"
													title="复制完整连接"
												>
													{copiedLink ? (
														<Check className="h-3 w-3 text-emerald-500" />
													) : (
														<Copy className="h-3 w-3" />
													)}
												</button>
												<button
													type="button"
													onClick={() => setRemoteLinkInput("")}
													className="text-muted-foreground hover:text-foreground p-0.5 cursor-pointer"
													title="清空"
												>
													<X className="h-3 w-3" />
												</button>
											</div>
										) : null}
									</div>
								</div>

								<Button
									onClick={() => void handleConnectFromLink()}
									disabled={actionLoading || oauthLoading || checkingProviders}
									className="h-8 px-3.5 text-xs font-semibold gap-1.5 bg-primary text-primary-foreground hover:bg-primary/90 shadow-xs flex-none cursor-pointer"
								>
									{checkingProviders ? (
										<>
											<RotateCcw className="h-3.5 w-3.5 animate-spin" />
											<span>正在连接...</span>
										</>
									) : (
										<>
											<Plug className="h-3.5 w-3.5" />
											<span>连接</span>
										</>
									)}
								</Button>
							</div>
						</div>
					)}

					{/* 隧道连接与实时状态卡片 (Active Tunnel Status) */}
					<div className="flex-none rounded-lg border border-border bg-card p-3 shadow-xs space-y-2.5">
						<div className="flex items-center justify-between gap-2">
							<div className="flex items-center gap-2 min-w-0 flex-1">
								<div
									className={cn(
										"flex h-8 w-8 flex-none items-center justify-center rounded-md text-white transition-colors",
										isConnected
											? "bg-emerald-600"
											: isConnecting
												? "bg-amber-600 animate-pulse"
												: "bg-muted text-muted-foreground",
									)}
								>
									{isConnecting ? (
										<RotateCcw className="h-4 w-4 animate-spin" />
									) : (
										<Radio className="h-4 w-4" />
									)}
								</div>
								<div className="min-w-0 flex-1">
									<div className="flex items-center gap-1.5">
										<span className="font-bold text-xs sm:text-sm text-foreground truncate">
											{profileName || "Default Profile"}
										</span>
										<Badge
											variant="secondary"
											className="font-mono text-[9px] uppercase px-1 py-0 h-4 flex-none"
										>
											{status?.transport || transport}
										</Badge>
									</div>
									<div className="flex items-center gap-1 text-[11px] font-mono text-muted-foreground truncate">
										<span className="truncate">
											{serverAddr || status?.server_addr || "No Server"}
										</span>
										<span>&rarr;</span>
										<span className="truncate">{listenAddr || status?.listen_addr}</span>
									</div>
								</div>
							</div>

							{isRunning ? (
								<Button
									size="sm"
									variant="outline"
									disabled={actionLoading}
									onClick={handleDisconnect}
									className="h-7 px-2.5 text-xs font-semibold gap-1 rounded-md flex-none shadow-xs text-destructive hover:bg-destructive/10 cursor-pointer"
								>
									{actionLoading ? (
										<RotateCcw className="h-3.5 w-3.5 animate-spin" />
									) : (
										<Power className="h-3.5 w-3.5" />
									)}
									<span>断开隧道</span>
								</Button>
							) : authSession?.authenticated ? (
								<Button
									size="sm"
									variant="default"
									disabled={actionLoading}
									onClick={handleConnect}
									className="h-7 px-3 text-xs font-bold gap-1 rounded-md flex-none shadow-xs bg-primary text-primary-foreground hover:bg-primary/90 cursor-pointer"
								>
									{actionLoading ? (
										<RotateCcw className="h-3.5 w-3.5 animate-spin" />
									) : (
										<Power className="h-3.5 w-3.5" />
									)}
									<span>启动连接</span>
								</Button>
							) : (
								<span className="text-[11px] font-medium text-amber-500 bg-amber-500/10 border border-amber-500/20 px-2 py-0.5 rounded">
									等待登录
								</span>
							)}
						</div>

						{/* Connected Metrics Strip */}
						{isConnected ? (
							<div className="border-t border-border/60 pt-2 space-y-2">
								{/* Mode Switcher */}
								<div className="flex items-center justify-between gap-1 text-[10px] text-muted-foreground">
									<span className="font-semibold uppercase tracking-wider text-[9px]">
										{statsViewMode === "session" ? "Current Session" : "Cumulative Lifetime"}
									</span>
									<div className="flex items-center rounded border border-input p-0.5 text-[9px]">
										<button
											type="button"
											onClick={() => setStatsViewMode("session")}
											className={cn(
												"rounded px-1.5 py-0.2 font-medium transition cursor-pointer",
												statsViewMode === "session"
													? "bg-primary text-primary-foreground font-semibold"
													: "text-muted-foreground hover:text-foreground",
											)}
										>
											Session
										</button>
										<button
											type="button"
											onClick={() => setStatsViewMode("lifetime")}
											className={cn(
												"rounded px-1.5 py-0.2 font-medium transition cursor-pointer",
												statsViewMode === "lifetime"
													? "bg-primary text-primary-foreground font-semibold"
													: "text-muted-foreground hover:text-foreground",
											)}
										>
											Lifetime
										</button>
									</div>
								</div>

								{/* 4-col compact stats */}
								<div className="grid grid-cols-4 gap-1.5 text-center font-mono">
									<div className="rounded bg-muted/40 px-1.5 py-1">
										<div className="text-[9px] uppercase text-muted-foreground">
											{statsViewMode === "session" ? "Uptime" : "Sessions"}
										</div>
										<div className="text-xs font-bold text-foreground truncate">
											{statsViewMode === "session"
												? formatUptime(uptimeSeconds)
												: (cumulativeStats?.sessions_count ?? 0) + 1}
										</div>
									</div>
									<div className="rounded bg-muted/40 px-1.5 py-1">
										<div className="text-[9px] uppercase text-muted-foreground">Raw</div>
										<div className="text-xs font-bold text-foreground truncate">
											{formatBytes(
												statsViewMode === "session"
													? rawBytes
													: (cumulativeStats?.raw_bytes ?? 0) + rawBytes,
											)}
										</div>
									</div>
									<div className="rounded bg-muted/40 px-1.5 py-1">
										<div className="text-[9px] uppercase text-muted-foreground">Wire</div>
										<div className="text-xs font-bold text-foreground truncate">
											{formatBytes(
												statsViewMode === "session"
													? wireBytes
													: (cumulativeStats?.wire_bytes ?? 0) + wireBytes,
											)}
										</div>
									</div>
									<div className="rounded bg-muted/40 px-1.5 py-1">
										<div className="text-[9px] uppercase text-emerald-500">Saved</div>
										<div className="text-xs font-bold text-emerald-500 truncate">
											{statsViewMode === "session"
												? `${savedRatio.toFixed(1)}%`
												: (() => {
														const r = (cumulativeStats?.raw_bytes ?? 0) + rawBytes;
														const w = (cumulativeStats?.wire_bytes ?? 0) + wireBytes;
														const ratio = r > 0 && w <= r ? ((r - w) / r) * 100 : 0;
														return `${ratio.toFixed(1)}%`;
													})()}
										</div>
									</div>
								</div>

								{/* Optimizer Directional & Latency Breakdown */}
								<div className="grid grid-cols-3 gap-1 text-center font-mono text-[10px]">
									<div className="rounded bg-muted/25 px-2 py-1 border border-border/40 flex items-center justify-between">
										<span className="text-[9px] font-sans font-medium text-muted-foreground flex items-center gap-0.5">
											<span className="text-primary font-bold">↑</span> Up
										</span>
										<span className="font-semibold text-foreground truncate ml-1">
											{formatBytes(status?.stats.uplink?.wire_bytes ?? 0)}
											<span className="text-muted-foreground font-normal text-[9px] ml-1">
												({((status?.stats.uplink?.saved_ratio ?? 0) * 100).toFixed(0)}%)
											</span>
										</span>
									</div>
									<div className="rounded bg-muted/25 px-2 py-1 border border-border/40 flex items-center justify-between">
										<span className="text-[9px] font-sans font-medium text-muted-foreground flex items-center gap-0.5">
											<span className="text-primary font-bold">↓</span> Down
										</span>
										<span className="font-semibold text-foreground truncate ml-1">
											{formatBytes(status?.stats.downlink?.wire_bytes ?? 0)}
											<span className="text-muted-foreground font-normal text-[9px] ml-1">
												({((status?.stats.downlink?.saved_ratio ?? 0) * 100).toFixed(0)}%)
											</span>
										</span>
									</div>
									<div
										className="rounded bg-muted/25 px-2 py-1 border border-border/40 flex items-center justify-between"
										title={`Est. transfer saved: -${(status?.stats.est_transfer_time_saved_ms ?? 0).toFixed(1)}ms, CPU processing: +${(status?.stats.est_processing_time_ms ?? 0).toFixed(1)}ms`}
									>
										<span className="text-[9px] font-sans font-medium text-muted-foreground">
											Latency
										</span>
										<span
											className={cn(
												"font-semibold text-[10px]",
												(status?.stats.net_latency_saved_ms ?? 0) > 0
													? "text-emerald-500"
													: (status?.stats.net_latency_saved_ms ?? 0) < 0
														? "text-amber-500"
														: "text-muted-foreground",
											)}
										>
											{(status?.stats.net_latency_saved_ms ?? 0) > 0
												? `-${(status?.stats.net_latency_saved_ms ?? 0).toFixed(1)}ms`
												: (status?.stats.net_latency_saved_ms ?? 0) < 0
													? `+${Math.abs(status?.stats.net_latency_saved_ms ?? 0).toFixed(1)}ms`
													: "0.0ms"}
										</span>
									</div>
								</div>

								{/* Throughput and LAN bar */}
								<div className="flex items-center justify-between gap-2 px-0.5 text-[10px] text-muted-foreground">
									<div className="flex items-center gap-1.5 flex-1 min-w-0">
										<Activity className="h-3 w-3 text-emerald-500 flex-none" />
										<span className="font-mono text-emerald-500 font-semibold flex-none text-[10px]">
											{formatBytes(throughputSamples[throughputSamples.length - 1] || 0)}/s
										</span>
										<div className="w-24 h-4 flex-none overflow-hidden">
											<ThroughputSparkline samples={throughputSamples} />
										</div>
									</div>

									{fakeLanBroadcast ? (
										<div className="flex items-center gap-1 flex-none bg-primary/10 text-primary px-1.5 py-0.5 rounded text-[10px] font-medium">
											<Gamepad2 className="h-3 w-3" />
											<span>LAN Active</span>
											<button
												type="button"
												onClick={() => copyText(status?.listen_addr || listenAddr, "lan-btn")}
												className="ml-0.5 hover:opacity-80 cursor-pointer"
												title="Copy LAN address"
											>
												{copied === "lan-btn" ? (
													<Check className="h-2.5 w-2.5 text-emerald-500" />
												) : (
													<Copy className="h-2.5 w-2.5" />
												)}
											</button>
										</div>
									) : null}
								</div>
							</div>
						) : null}

						{!isConnected && cumulativeStats && cumulativeStats.raw_bytes > 0 ? (
							<div className="border-t border-border/60 pt-1.5 flex items-center justify-between gap-2 text-[10px] text-muted-foreground">
								<div className="flex items-center gap-1.5 truncate">
									<span className="font-semibold uppercase tracking-wider text-[9px] text-primary">
										Lifetime Persisted
									</span>
									<span className="font-mono truncate">
										{formatBytes(cumulativeStats.raw_bytes)} raw &bull;{" "}
										{formatBytes(cumulativeStats.wire_bytes)} wire &bull;{" "}
										<span className="text-emerald-500 font-bold">
											{((cumulativeStats.saved_ratio || 0) * 100).toFixed(1)}% saved
										</span>
										<span className="text-muted-foreground/70 ml-1">
											({cumulativeStats.sessions_count} sessions)
										</span>
									</span>
								</div>
								<Button
									variant="ghost"
									size="xs"
									onClick={handleResetStats}
									className="h-5 px-1.5 text-[9px] text-muted-foreground hover:text-destructive cursor-pointer"
									title="Reset cumulative statistics"
								>
									Reset Stats
								</Button>
							</div>
						) : null}
					</div>

					{/* 已发现远端服务卡片 (Discovered Remote Services) */}
					<div className="flex-1 min-h-0 flex flex-col rounded-lg border border-border bg-card p-3 shadow-xs">
						<div className="flex items-center justify-between pb-2 border-b border-border/50 flex-none">
							<span className="text-xs font-semibold text-foreground">已发现远端服务</span>
							<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4">
								{status?.known_services.length || 0} active
							</Badge>
						</div>

						<div className="flex-1 min-h-0 overflow-y-auto divide-y divide-border/40 pt-1">
							{!authSession?.authenticated && !authToken ? (
								<div className="flex h-full flex-col items-center justify-center py-8 text-center text-muted-foreground">
									<WifiOff className="mb-2 h-7 w-7 text-muted-foreground/50" />
									<p className="text-xs font-medium text-foreground">未完成身份认证</p>
									<p className="text-[11px] text-muted-foreground max-w-sm mt-1">
										服务端默认允许所有人连接，但登录前仅限进行登录操作。请先在上方连接并完成登录以获取服务访问权限。
									</p>
								</div>
							) : status?.known_services && status.known_services.length > 0 ? (
								status.known_services.map((svc, idx) => {
									const rawListen = status?.listen_addr || listenAddr || "127.0.0.1:25565";
									const port = parsePort(rawListen);
									const copyTarget = getLoopbackTargetForService(idx, port);

									return (
										<div key={svc.name} className="flex items-center justify-between gap-2 py-2">
											<div className="flex items-center gap-2 min-w-0 flex-1">
												<div className="flex h-7 w-7 flex-none items-center justify-center rounded bg-primary/10 text-primary">
													<Radio className="h-3.5 w-3.5" />
												</div>
												<div className="min-w-0 flex-1">
													<div className="flex items-center gap-1.5">
														<span className="font-semibold text-foreground truncate text-xs">
															{svc.name}
														</span>
														<Badge
															variant="secondary"
															className="font-mono text-[9px] uppercase px-1 py-0 h-3.5 flex-none"
														>
															{svc.proto}
														</Badge>
													</div>
													<div className="flex items-center gap-1.5 font-mono text-[10px] text-muted-foreground truncate">
														<span className="text-primary font-medium">{copyTarget}</span>
														{svc.masquerade_host ? (
															<span className="text-muted-foreground/70">
																({svc.masquerade_host})
															</span>
														) : null}
													</div>
												</div>
											</div>

											<Button
												variant="outline"
												size="xs"
												onClick={() => copyText(copyTarget, svc.name)}
												className="gap-1 text-[10px] h-6 px-2 flex-none cursor-pointer"
											>
												{copied === svc.name ? (
													<Check className="h-3 w-3 text-emerald-500" />
												) : (
													<Copy className="h-3 w-3" />
												)}
												<span>{copied === svc.name ? "Copied" : "Copy"}</span>
											</Button>
										</div>
									);
								})
							) : (
								<div className="flex h-full flex-col items-center justify-center py-8 text-center text-muted-foreground">
									<WifiOff className="mb-2 h-7 w-7 text-muted-foreground/50" />
									<p className="text-xs">
										{isConnected
											? "等待 Connector 发布远端服务..."
											: "连接到 Prism 服务器以查看发布的服务。"}
									</p>
								</div>
							)}
						</div>
					</div>
				</div>
			) : null}

			{/* 2. Logs Page: 运行日志 (Full-Page Terminal Log Console) */}
			{currentTab === "logs" ? (
				<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-hidden">
					{/* Header Bar */}
					<div className="flex flex-none select-none items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
						<div className="flex items-center gap-2 min-w-0">
							<Terminal className="h-4 w-4 text-primary flex-none" />
							<div className="min-w-0">
								<h1 className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground">
									运行日志
								</h1>
								<p className="truncate text-[10px] text-muted-foreground hidden sm:block">
									客户端与隧道连接的实时事件与传输记录
								</p>
							</div>
							<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4 font-mono">
								{filteredLogs.length} / {logs.length}
							</Badge>
						</div>

						{/* Action Toolbar */}
						<div className="flex items-center gap-1.5 flex-wrap">
							{/* Level Filters */}
							<div className="flex items-center rounded border border-input p-0.5 text-[10px]">
								{(["ALL", "INFO", "WARN", "ERROR"] as const).map((lvl) => (
									<button
										key={lvl}
										type="button"
										onClick={() => setLogFilterLevel(lvl)}
										className={cn(
											"rounded px-2 py-0.5 font-semibold transition cursor-pointer",
											logFilterLevel === lvl
												? "bg-primary text-primary-foreground"
												: "text-muted-foreground hover:text-foreground",
										)}
									>
										{lvl}
									</button>
								))}
							</div>

							{/* Search Filter */}
							<div className="relative w-28 sm:w-36">
								<Search className="pointer-events-none absolute top-1/2 left-2 h-3 w-3 -translate-y-1/2 text-muted-foreground" />
								<Input
									placeholder="Filter logs..."
									value={logSearchQuery}
									onChange={(e) => setLogSearchQuery(e.target.value)}
									className="h-7 pl-6 pr-2 text-xs font-mono"
								/>
							</div>

							<Button
								variant={autoScrollLogs && isAtBottom ? "secondary" : "outline"}
								size="xs"
								onClick={() => {
									if (autoScrollLogs && isAtBottom) {
										setAutoScrollLogs(false);
									} else {
										setAutoScrollLogs(true);
										scrollToBottom(true);
									}
								}}
								className="h-7 text-xs px-2 cursor-pointer"
							>
								Scroll: {autoScrollLogs ? (isAtBottom ? "ON" : "PAUSED") : "OFF"}
							</Button>

							<Button
								variant="outline"
								size="xs"
								onClick={handleClearLogs}
								className="h-7 text-xs px-2 text-destructive hover:bg-destructive/10 cursor-pointer"
							>
								清空
							</Button>

							<Button
								variant="outline"
								size="xs"
								onClick={handleCopyAllLogs}
								className="h-7 text-xs px-2 gap-1 cursor-pointer"
							>
								{copied === "all-logs" ? (
									<Check className="h-3 w-3 text-emerald-500" />
								) : (
									<Copy className="h-3 w-3" />
								)}
								<span>{copied === "all-logs" ? "已复制" : "复制全部"}</span>
							</Button>
						</div>
					</div>

					{/* Terminal Window Viewport */}
					<div className="relative flex-1 min-h-0 flex flex-col rounded-xl border border-border bg-slate-950 overflow-hidden shadow-inner">
						<div
							ref={logsContainerRef}
							onScroll={handleLogsScroll}
							className="flex-1 min-h-0 overflow-y-auto p-3 font-mono text-[11px] leading-relaxed text-slate-200 selection:bg-primary/30 scrollbar-thin"
						>
							{filteredLogs.length > 0 ? (
								<div className="flex flex-col gap-1">
									{filteredLogs.map((entry, idx) => {
										const lvl = entry.level.toUpperCase();
										const badgeColor =
											lvl === "ERROR"
												? "text-red-400 bg-red-950/60 border-red-800/40"
												: lvl === "WARN"
													? "text-amber-400 bg-amber-950/60 border-amber-800/40"
													: lvl === "DEBUG"
														? "text-slate-400 bg-slate-900 border-slate-800"
														: "text-emerald-400 bg-emerald-950/60 border-emerald-800/40";

										return (
											<div
												key={idx}
												className="flex items-start gap-1.5 leading-relaxed hover:bg-white/5 px-1 py-0.5 rounded transition-colors"
											>
												<span className="shrink-0 text-slate-500 selection:text-slate-300 text-[10px]">
													[
													{entry.timestamp.length > 8
														? entry.timestamp.includes("T")
															? (entry.timestamp.split("T")[1]?.slice(0, 8) ?? entry.timestamp)
															: entry.timestamp
														: entry.timestamp}
													]
												</span>
												<span
													className={cn(
														"shrink-0 rounded px-1.5 py-0.2 text-[9px] font-bold border",
														badgeColor,
													)}
												>
													{entry.level}
												</span>
												<span className="shrink-0 text-slate-400 font-semibold">
													{entry.target}:
												</span>
												<span className="break-all text-slate-100">{entry.message}</span>
											</div>
										);
									})}
								</div>
							) : (
								<div className="flex h-full flex-col items-center justify-center text-slate-500 py-8">
									<Terminal className="mb-2 h-8 w-8 opacity-40" />
									<p className="text-xs">暂无客户端运行日志记录</p>
								</div>
							)}
						</div>

						{/* Floating Jump to Bottom Button */}
						{!isAtBottom && filteredLogs.length > 0 ? (
							<button
								type="button"
								onClick={() => {
									setAutoScrollLogs(true);
									scrollToBottom(true);
								}}
								className="absolute bottom-3 right-3 z-10 flex items-center gap-1 rounded-full bg-primary hover:bg-primary/90 text-primary-foreground px-3 py-1 text-xs font-medium shadow-lg transition-all duration-150 backdrop-blur cursor-pointer"
							>
								<ArrowDown className="h-3 w-3" />
								<span>跳转到最新</span>
							</button>
						) : null}
					</div>
				</div>
			) : null}

			{/* 3. Settings Page: 隧道配置与配置集 (Profiles & Settings Manager) */}
			{currentTab === "settings" ? (
				<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-hidden">
					{/* Header Bar */}
					<div className="flex flex-none select-none items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
						<div className="flex items-center gap-2 min-w-0">
							<Settings2 className="h-4 w-4 text-primary flex-none" />
							<div className="min-w-0">
								<h1 className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground">
									隧道配置与配置集
								</h1>
								<p className="truncate text-[10px] text-muted-foreground hidden sm:block">
									管理连接配置集 (Profiles) 与底层网络传输协议参数
								</p>
							</div>
						</div>

						{/* Top Actions */}
						<div className="flex items-center gap-1.5 flex-none">
							<Button
								variant="outline"
								size="xs"
								onClick={() => {
									const id = `profile-${Date.now()}`;
									setProfileName("New Profile");
									setServerAddr("127.0.0.1:7000");
									setTransport("quic");
									setAuthToken("");
									setListenAddr("127.0.0.1:25565");
									setFakeLanBroadcast(true);
									setSelectedProfileId(id);
								}}
								className="h-7 gap-1 text-xs px-2.5 cursor-pointer"
							>
								<Plus className="h-3.5 w-3.5" />
								<span>新建配置集</span>
							</Button>

							<Button
								variant="outline"
								size="xs"
								onClick={() => setImportModalOpen(true)}
								className="h-7 gap-1 text-xs px-2.5 cursor-pointer"
							>
								<Download className="h-3.5 w-3.5 text-primary" />
								<span>导入链接</span>
							</Button>

							<Button
								variant="outline"
								size="xs"
								onClick={handleShareLink}
								className="h-7 gap-1 text-xs px-2.5 cursor-pointer"
							>
								{copied === "share" ? (
									<Check className="h-3.5 w-3.5 text-emerald-500" />
								) : (
									<Share2 className="h-3.5 w-3.5 text-primary" />
								)}
								<span>{copied === "share" ? "已复制" : "分享配置"}</span>
							</Button>
						</div>
					</div>

					{/* 2-Column Master-Detail Layout */}
					<div className="grid grid-cols-1 md:grid-cols-12 gap-3 flex-1 min-h-0 overflow-hidden">
						{/* Left Column: Saved Profiles List */}
						<div className="md:col-span-5 flex flex-col min-h-0 rounded-lg border border-border bg-card p-3 shadow-xs space-y-2">
							<div className="flex items-center justify-between pb-1.5 border-b border-border/50 flex-none">
								<span className="text-xs font-semibold text-foreground">已保存配置集</span>
								<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4">
									{profiles.length} 个配置
								</Badge>
							</div>

							<div className="flex-1 min-h-0 overflow-y-auto divide-y divide-border/40 pr-1 space-y-1">
								{profiles.length > 0 ? (
									profiles.map((p) => {
										const isSelected = p.id === selectedProfileId;
										return (
											<div
												key={p.id}
												onClick={() => handleSelectProfile(p.id)}
												className={cn(
													"flex items-center justify-between p-2 rounded-lg cursor-pointer transition-colors gap-2",
													isSelected
														? "bg-primary/10 border border-primary/30"
														: "hover:bg-accent/50",
												)}
											>
												<div className="min-w-0 flex-1">
													<div className="flex items-center gap-1.5">
														<span className="font-semibold text-foreground truncate text-xs">
															{p.name}
														</span>
														{isSelected ? (
															<Badge className="bg-primary text-primary-foreground text-[9px] px-1 py-0 h-3.5">
																当前
															</Badge>
														) : null}
													</div>
													<div className="font-mono text-[10px] text-muted-foreground truncate">
														{p.server_addr} ({p.transport.toUpperCase()}) &bull; 本地:{" "}
														{p.listen_addr}
													</div>
												</div>

												<div className="flex items-center gap-1 flex-none">
													<Button
														variant="ghost"
														size="icon-xs"
														onClick={(e) => {
															e.stopPropagation();
															handleDeleteProfile(p.id);
														}}
														className="h-6 w-6 p-0 text-destructive hover:bg-destructive/10 cursor-pointer"
														title="删除配置集"
													>
														<Trash2 className="h-3 w-3" />
													</Button>
												</div>
											</div>
										);
									})
								) : (
									<div className="py-8 text-center text-xs text-muted-foreground">
										暂无已保存配置集，点击上方“新建配置集”或“导入链接”创建。
									</div>
								)}
							</div>
						</div>

						{/* Right Column: Configuration Form */}
						<div className="md:col-span-7 flex flex-col min-h-0 rounded-lg border border-border bg-card p-3 shadow-xs overflow-y-auto space-y-3">
							<div className="flex items-center justify-between pb-1.5 border-b border-border/50 flex-none">
								<span className="text-xs font-semibold text-foreground truncate">
									编辑配置: {profileName || "未命名配置"}
								</span>
								<span className="text-[10px] text-muted-foreground">
									ID: {selectedProfileId || "新建"}
								</span>
							</div>

							<div className="space-y-2.5 flex-1">
								<div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
									<div className="space-y-1">
										<label className="text-[10px] uppercase font-bold text-muted-foreground">
											配置集名称
										</label>
										<Input
											value={profileName}
											onChange={(e) => setProfileName(e.target.value)}
											placeholder="例如: 我的游戏服务器"
											className="h-8 text-xs"
										/>
									</div>

									<div className="space-y-1">
										<label className="text-[10px] uppercase font-bold text-muted-foreground">
											远端中继服务器地址
										</label>
										<Input
											value={serverAddr}
											onChange={(e) => setServerAddr(e.target.value)}
											placeholder="relay.example.com:7000"
											className="h-8 text-xs font-mono"
										/>
									</div>
								</div>

								<div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
									<div className="space-y-1">
										<label className="text-[10px] uppercase font-bold text-muted-foreground">
											传输协议 (Transport)
										</label>
										<select
											aria-label="Transport Protocol"
											value={transport}
											onChange={(e) => {
												const val = e.target.value;
												setTransport(val);
												const matched = SUPPORTED_LINK_PROTOCOLS.find((item) => item.transport === val);
												if (matched) setLinkProtocol(matched.value);
											}}
											className="h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-ring"
										>
											<option value="quic">QUIC (极速抗丢包，推荐)</option>
											<option value="kcp">KCP (低延迟 UDP)</option>
											<option value="tcp">TCP (标准流传输)</option>
											<option value="websocket">WebSocket (穿透受限网络)</option>
										</select>
									</div>

									<div className="space-y-1">
										<label className="text-[10px] uppercase font-bold text-muted-foreground">
											本地监听端口 / Ingress
										</label>
										<Input
											value={listenAddr}
											onChange={(e) => setListenAddr(e.target.value)}
											placeholder="127.0.0.1:25565"
											className="h-8 text-xs font-mono"
										/>
									</div>
								</div>

								{/* Toggles */}
								<div className="space-y-2 pt-1">
									<div className="flex items-center justify-between rounded-lg border border-border/60 p-2 text-xs">
										<div>
											<div className="font-medium text-xs text-foreground">
												Minecraft 局域网广播 (LAN Discovery)
											</div>
											<div className="text-[10px] text-muted-foreground">
												在局域网内自动广播游戏服务，便于客户端发现
											</div>
										</div>
										<Switch checked={fakeLanBroadcast} onCheckedChange={setFakeLanBroadcast} />
									</div>

									<div className="flex items-center justify-between rounded-lg border border-border/60 p-2 text-xs">
										<div>
											<div className="font-medium text-xs text-foreground">
												控制面板自动连接 (Auto-Connect Panel)
											</div>
											<div className="text-[10px] text-muted-foreground">
												自动同步管理面板与鉴权状态 ({managementUrl})
											</div>
										</div>
										<Switch checked={autoConnectPanel} onCheckedChange={setAutoConnectPanel} />
									</div>
								</div>
							</div>

							{/* Footer Controls */}
							<div className="flex items-center justify-between pt-2 border-t border-border/50 flex-none">
								{selectedProfileId ? (
									<Button
										variant="outline"
										size="sm"
										onClick={() => handleDeleteProfile(selectedProfileId)}
										className="h-7 text-xs text-destructive hover:bg-destructive/10 cursor-pointer"
									>
										删除此配置
									</Button>
								) : (
									<div />
								)}

								<Button
									size="sm"
									onClick={handleSaveProfile}
									className="h-7 text-xs gap-1 px-3.5 cursor-pointer"
								>
									<Check className="h-3.5 w-3.5" />
									<span>保存配置集</span>
								</Button>
							</div>
						</div>
					</div>
				</div>
			) : null}

			{/* Import Modal */}
			{importModalOpen ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-3 sm:p-4 backdrop-blur-xs">
					<Card className="w-full max-w-sm sm:max-w-lg shadow-xl">
						<CardHeader>
							<CardTitle>Import Prism Invite Link</CardTitle>
							<CardDescription>
								Paste a <code className="text-primary">prism://</code> link or server address
							</CardDescription>
						</CardHeader>
						<CardContent className="space-y-3">
							<Input
								value={importUrl}
								onChange={(e) => setImportUrl(e.target.value)}
								placeholder="quic://play.example.com:7000 或 prism://play.example.com:7000?name=..."
							/>
							{importError ? <p className="text-xs text-destructive">{importError}</p> : null}
						</CardContent>
						<CardFooter className="flex justify-end gap-2 border-t border-border pt-4">
							<Button
								variant="outline"
								onClick={() => {
									setImportModalOpen(false);
									setImportError(null);
								}}
							>
								Cancel
							</Button>
							<Button onClick={handleImportLink}>Import & Apply</Button>
						</CardFooter>
					</Card>
				</div>
			) : null}

			{/* 选择登录方式对话框 (Select Login Method Modal) */}
			{loginModalOpen ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-3 sm:p-4 backdrop-blur-xs">
					<Card className="w-full max-w-sm sm:max-w-lg shadow-xl border-border bg-card">
						<CardHeader className="flex flex-row items-center justify-between pb-3">
							<div className="flex items-center gap-2 min-w-0">
								<Plug className="h-5 w-5 text-primary flex-none" />
								<div className="min-w-0">
									<CardTitle className="text-sm sm:text-base font-bold">选择登录方式</CardTitle>
									<CardDescription className="text-xs truncate">
										远端节点：<code className="text-foreground font-mono">{serverAddr || "未指定"}</code>
									</CardDescription>
								</div>
							</div>
							<Button
								variant="ghost"
								size="icon-xs"
								onClick={() => {
									setLoginModalOpen(false);
									setAuthError(null);
									setProvidersError(null);
								}}
							>
								<X className="h-4 w-4" />
							</Button>
						</CardHeader>
						<CardContent className="space-y-3.5">
							{checkingProviders ? (
								<div className="flex flex-col items-center justify-center py-8 text-center text-muted-foreground gap-2.5">
									<RotateCcw className="h-7 w-7 animate-spin text-primary" />
									<p className="text-xs">正在探测远端支持的登录方式...</p>
								</div>
							) : (
								<>
									<div className="space-y-1">
										<div className="flex items-center justify-between">
											<label className="text-[11px] font-medium text-muted-foreground">
												远端管理服务地址 (Auth Server URL)
											</label>
											<button
												type="button"
												onClick={() => void handleConnectFromLink()}
												className="text-primary hover:underline text-[11px] cursor-pointer flex items-center gap-1"
											>
												<RotateCcw className="h-3 w-3" />
												重新探测
											</button>
										</div>
										<Input
											value={authServerUrl}
											onChange={(e) => setAuthServerUrl(e.target.value)}
											className="h-8 text-xs font-mono"
											disabled={oauthLoading}
										/>
									</div>

									{providersError ? (
										<div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-2.5 text-xs text-amber-500 flex items-start gap-2">
											<WifiOff className="h-4 w-4 flex-none mt-0.5" />
											<div className="flex-1 min-w-0">
												<p>{providersError}</p>
											</div>
										</div>
									) : null}

									{authError ? (
										<div className="rounded-lg border border-destructive/30 bg-destructive/10 p-2.5 text-xs text-destructive">
											{authError}
										</div>
									) : null}

									{authToken ? (
										<div className="flex items-center gap-2.5 rounded-lg border border-emerald-500/30 bg-emerald-500/10 p-3 text-emerald-500">
											<CheckCircle2 className="h-5 w-5 flex-none" />
											<div className="text-xs">
												<p className="font-semibold">已完成登录与凭证配置！</p>
												{isAdmin || loginAdminUnlocked ? (
													<p className="text-emerald-400 font-medium mt-0.5">
														已确认管理员身份，侧边栏管理控制台已解锁。
													</p>
												) : (
													<p className="text-muted-foreground mt-0.5">访问凭证已保存至客户端配置。</p>
												)}
											</div>
										</div>
									) : (
										<div className="space-y-2.5 pt-1">
											<div className="text-xs font-medium text-muted-foreground">
												远端支持以下登录方式，请选择：
											</div>

											<div className="grid gap-2.5">
												{/* 1. GitHub OAuth Method */}
												{(providersResult?.github_enabled ||
													providersResult?.providers?.includes("github")) && (
													<div className="rounded-lg border border-border bg-muted/40 p-3 space-y-2 hover:border-primary/40 transition-colors">
														<div className="flex items-center justify-between">
															<div className="flex items-center gap-2">
																<div className="flex h-7 w-7 items-center justify-center rounded-md bg-foreground/10 text-foreground">
																	<Github className="h-4 w-4" />
																</div>
																<div>
																	<div className="text-xs font-bold text-foreground">
																		GitHub 授权登录
																	</div>
																	<div className="text-[11px] text-muted-foreground">
																		通过 GitHub OAuth 授权获取访问凭证
																	</div>
																</div>
															</div>
															<Badge variant="outline" className="text-[10px] text-emerald-500 border-emerald-500/30">
																推荐
															</Badge>
														</div>
														<p className="text-[11px] text-muted-foreground leading-relaxed">
															在浏览器中打开 GitHub 授权页面，授权完成后由 Deep Link 自动唤起客户端完成登录。
														</p>
														<Button
															size="sm"
															onClick={() => startGitHubAuthWithUrl(authServerUrl, serverAddr)}
															disabled={oauthLoading}
															className="w-full h-8 text-xs font-semibold gap-1.5 cursor-pointer bg-primary text-primary-foreground hover:bg-primary/90"
														>
															<Github className="h-3.5 w-3.5" />
															<span>{oauthLoading ? "正在获取授权链接..." : "前往 GitHub 授权登录"}</span>
														</Button>
													</div>
												)}
												{/* If remote returns no login methods */}
												{providersResult &&
													!providersResult.github_enabled &&
													(!providersResult.providers || !providersResult.providers.includes("github")) && (
														<div className="rounded-lg border border-muted bg-muted/20 p-4 text-center text-xs text-muted-foreground">
															远端节点未开启授权登录（如 GitHub OAuth），请联系服务端管理员开启配置。
														</div>
													)}
											</div>
										</div>
									)}
								</>
							)}
						</CardContent>
						<CardFooter className="flex justify-end border-t border-border pt-3">
							<Button
								variant="outline"
								size="sm"
								onClick={() => {
									setLoginModalOpen(false);
									setAuthError(null);
									setProvidersError(null);
								}}
							>
								关闭
							</Button>
						</CardFooter>
					</Card>
				</div>
			) : null}
		</div>
	);
}

function ThroughputSparkline({ samples }: { samples: number[] }) {
	const max = Math.max(...samples, 1024);
	const width = 280;
	const height = 18;
	const points = samples
		.map((v, i) => {
			const x = (i / (samples.length - 1)) * width;
			const y = height - (v / max) * (height - 4) - 2;
			return `${x.toFixed(1)},${y.toFixed(1)}`;
		})
		.join(" ");

	return (
		<svg viewBox={`0 0 ${width} ${height}`} className="h-4 w-full overflow-visible">
			<polyline
				fill="none"
				stroke="currentColor"
				strokeWidth="2"
				strokeLinecap="round"
				strokeLinejoin="round"
				className="text-emerald-500 transition-all duration-300"
				points={points}
			/>
		</svg>
	);
}
