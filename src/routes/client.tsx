import { createFileRoute, Link } from "@tanstack/react-router";
import {
	Activity,
	ArrowDown,
	Check,
	CheckCircle2,
	Copy,
	Download,
	ExternalLink,
	Eye,
	EyeOff,
	Gamepad2,
	Github,
	Layers,
	Menu,
	Minus,
	Plus,
	Power,
	Radio,
	RotateCcw,
	Search,
	Server,
	Settings2,
	Share2,
	Terminal,
	Trash2,
	WifiOff,
	X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { closeWindow, isDesktopApp, minimizeWindow } from "@/lib/desktopWindow";

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
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { formatBytes } from "@/lib/format";
import {
	type ClientLogEntry,
	type ClientProfile,
	type ClientStatusResponse,
	type DeviceCodeResponse,
	clearClientLogs,
	getClientLogs,
	getClientProfiles,
	getClientStatus,
	getHealth,
	pollDeviceCode,
	requestDeviceCode,
	saveClientProfiles,
	startClient,
	stopClient,
} from "@/lib/managementApi";
import { deriveManagementUrl, normalizeBaseUrl } from "@/lib/panelConnection";
import { usePanelSession } from "@/lib/panelSession";
import { encodePrismLink, parsePrismLink } from "@/lib/prismLink";
import { usePolling } from "@/lib/usePolling";
import { cn } from "@/lib/utils";

export const Route = createFileRoute("/client")({
	component: ClientDashboardPage,
});

function ClientDashboardPage() {
	const { connection, saveConnection } = usePanelSession();
	const isDesktop = useMemo(() => isDesktopApp(), []);
	const effectiveConnection = useMemo(
		() =>
			connection ?? {
				baseUrl: isDesktop ? "http://127.0.0.1:8080" : "",
				token: "",
			},
		[connection, isDesktop],
	);

	const [status, setStatus] = useState<ClientStatusResponse | null>(null);
	const [profiles, setProfiles] = useState<ClientProfile[]>([]);
	const [selectedProfileId, setSelectedProfileId] = useState<string>("");
	const [actionLoading, setActionLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [copied, setCopied] = useState<string | null>(null);
	const [activeTab, setActiveTab] = useState<string>("overview");
	const [drawerOpen, setDrawerOpen] = useState(false);

	// Throughput sparkline history
	const [throughputSamples, setThroughputSamples] = useState<number[]>([
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
	]);
	const prevWireRef = useRef(0);

	// Form / profile config state
	const [serverAddr, setServerAddr] = useState("127.0.0.1:7000");
	const [transport, setTransport] = useState("quic");
	const [authToken, setAuthToken] = useState("");
	const [listenAddr, setListenAddr] = useState("127.0.0.1:25565");
	const [fakeLanBroadcast, setFakeLanBroadcast] = useState(true);
	const [profileName, setProfileName] = useState("Default Realm");
	const [showToken, setShowToken] = useState(false);

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

	// GitHub auth modal state
	const [githubAuthOpen, setGithubAuthOpen] = useState(false);
	const [authServerUrl, setAuthServerUrl] = useState("http://127.0.0.1:8080");
	const [deviceCode, setDeviceCode] = useState<DeviceCodeResponse | null>(null);
	const [deviceLoading, setDeviceLoading] = useState(false);
	const [devicePolling, setDevicePolling] = useState(false);
	const [deviceSuccess, setDeviceSuccess] = useState(false);
	const [authError, setAuthError] = useState<string | null>(null);

	// Import modal state
	const [importModalOpen, setImportModalOpen] = useState(false);
	const [importUrl, setImportUrl] = useState("");
	const [importError, setImportError] = useState<string | null>(null);

	// Fetch status
	const fetchStatus = useCallback(() => {
		getClientStatus(effectiveConnection)
			.then((resp) => {
				setStatus(resp);
				if (resp.running && resp.server_addr) {
					setServerAddr((prev) => prev || resp.server_addr);
				}
			})
			.catch((err) => {
				console.debug("Failed to fetch client status:", err);
			});
	}, [effectiveConnection]);

	// Fetch profiles
	const fetchProfiles = useCallback(() => {
		getClientProfiles(effectiveConnection)
			.then((list) => {
				setProfiles(list);
				if (list.length > 0 && !selectedProfileId) {
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
	}, [effectiveConnection, selectedProfileId]);

	// Fetch logs with deduplication to avoid unnecessary re-renders
	const fetchLogs = useCallback(() => {
		getClientLogs(effectiveConnection, 300)
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
	}, [effectiveConnection]);

	useEffect(() => {
		fetchStatus();
		fetchProfiles();
		fetchLogs();
	}, [fetchStatus, fetchProfiles, fetchLogs]);

	// Poll status frequently
	usePolling(fetchStatus, 1500, true);

	// Poll logs frequently while on logs tab or when running
	usePolling(fetchLogs, 1500, activeTab === "logs" || status?.running === true);

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
		if (activeTab !== "logs") return;
		if (autoScrollLogs && isAtBottomRef.current) {
			scrollToBottom(false);
		}
	}, [filteredLogs, autoScrollLogs, activeTab, scrollToBottom]);

	// When user opens/switches to logs tab, scroll to bottom if auto-scroll is enabled
	useEffect(() => {
		if (activeTab === "logs" && autoScrollLogs && isAtBottomRef.current) {
			const frame = requestAnimationFrame(() => {
				scrollToBottom(false);
			});
			return () => cancelAnimationFrame(frame);
		}
	}, [activeTab, autoScrollLogs, scrollToBottom]);

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

		const targetUrl = managementUrl.trim() || deriveManagementUrl(serverAddr || status.server_addr);
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
		managementUrl,
		serverAddr,
		authToken,
		connection,
		saveConnection,
	]);

	// Start Device Authorization Flow
	const startDeviceAuth = async () => {
		setDeviceLoading(true);
		setAuthError(null);
		try {
			const norm = normalizeBaseUrl(authServerUrl);
			const resp = await requestDeviceCode({ baseUrl: norm, token: "" });
			setDeviceCode(resp);
			setDevicePolling(true);

			if (typeof window !== "undefined") {
				window.open(resp.verification_uri, "_blank");
			}

			const intervalMs = Math.max(resp.interval, 5) * 1000;
			const timer = setInterval(async () => {
				try {
					const pollRes = await pollDeviceCode({ baseUrl: norm, token: "" }, resp.device_code);
					if (pollRes.status === "complete" && pollRes.token) {
						clearInterval(timer);
						setDevicePolling(false);
						setDeviceSuccess(true);
						setAuthToken(pollRes.token);
						if (autoConnectPanel) {
							saveConnection({ baseUrl: norm, token: pollRes.token });
						}
						setTimeout(() => {
							setGithubAuthOpen(false);
							setDeviceSuccess(false);
							setDeviceCode(null);
						}, 2000);
					} else if (pollRes.status === "expired" || pollRes.status === "denied") {
						clearInterval(timer);
						setDevicePolling(false);
						setAuthError(`GitHub device authorization failed: ${pollRes.status}`);
					}
				} catch {
					// continue polling
				}
			}, intervalMs);
		} catch (err) {
			setAuthError(err instanceof Error ? err.message : String(err));
		} finally {
			setDeviceLoading(false);
		}
	};

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
		await saveClientProfiles(effectiveConnection, updated).catch(() => {});
	};

	// Delete Profile
	const handleDeleteProfile = async (id: string) => {
		const updated = profiles.filter((p) => p.id !== id);
		setProfiles(updated);
		if (selectedProfileId === id) {
			setSelectedProfileId(updated[0]?.id || "");
		}
		await saveClientProfiles(effectiveConnection, updated).catch(() => {});
	};

	// Connect / Disconnect Handlers
	const handleConnect = async () => {
		setActionLoading(true);
		setError(null);
		try {
			await startClient(effectiveConnection, {
				server_addr: serverAddr,
				transport,
				auth_token: authToken,
				listen_addr: listenAddr,
				fake_lan_broadcast: fakeLanBroadcast,
			});
			fetchStatus();
			fetchLogs();
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
			await stopClient(effectiveConnection);
			fetchStatus();
			fetchLogs();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setActionLoading(false);
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
			await clearClientLogs(effectiveConnection);
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
		if (parsed.auth_token !== undefined) setAuthToken(parsed.auth_token);
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
		<div className="relative h-screen max-h-screen w-full overflow-hidden bg-background">
			{/* OpenVPN Style Slide-Out Navigation Drawer */}
			{drawerOpen ? (
				<div className="fixed inset-0 z-50 flex">
					{/* Backdrop */}
					<div
						className="fixed inset-0 bg-black/60 backdrop-blur-xs transition-opacity"
						onClick={() => setDrawerOpen(false)}
					/>

					{/* Drawer Content */}
					<div className="relative z-10 flex w-72 flex-col border-r border-border bg-card p-5 text-card-foreground shadow-2xl">
						<div className="flex items-center justify-between border-b border-border pb-4">
							<div className="flex items-center gap-3">
								<img
									src="/logo192.png"
									alt="Prism"
									className="h-9 w-9 rounded-lg object-contain shadow-xs ring-1 ring-primary/20"
								/>
								<div>
									<div className="text-sm font-bold tracking-tight text-foreground">
										Prism Connect
									</div>
									<div className="text-[11px] text-muted-foreground">OpenVPN Client Shell</div>
								</div>
							</div>
							<Button
								variant="ghost"
								size="icon"
								className="h-8 w-8 text-muted-foreground hover:text-foreground"
								onClick={() => setDrawerOpen(false)}
							>
								<X className="h-4 w-4" />
							</Button>
						</div>

						<nav className="mt-4 flex flex-1 flex-col gap-1.5">
							<button
								type="button"
								onClick={() => {
									setActiveTab("overview");
									setDrawerOpen(false);
								}}
								className={cn(
									"flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition",
									activeTab === "overview"
										? "bg-primary/10 text-primary ring-1 ring-primary/20"
										: "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
								)}
							>
								<Layers className="h-4 w-4" />
								<span>Connection Overview</span>
							</button>

							<button
								type="button"
								onClick={() => {
									setActiveTab("profiles");
									setDrawerOpen(false);
								}}
								className={cn(
									"flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition",
									activeTab === "profiles"
										? "bg-primary/10 text-primary ring-1 ring-primary/20"
										: "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
								)}
							>
								<Server className="h-4 w-4" />
								<span>Profiles</span>
							</button>

							<button
								type="button"
								onClick={() => {
									setActiveTab("logs");
									setDrawerOpen(false);
								}}
								className={cn(
									"flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition",
									activeTab === "logs"
										? "bg-primary/10 text-primary ring-1 ring-primary/20"
										: "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
								)}
							>
								<Terminal className="h-4 w-4" />
								<span>Diagnostics & Logs</span>
							</button>

							<button
								type="button"
								onClick={() => {
									setActiveTab("settings");
									setDrawerOpen(false);
								}}
								className={cn(
									"flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition",
									activeTab === "settings"
										? "bg-primary/10 text-primary ring-1 ring-primary/20"
										: "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
								)}
							>
								<Settings2 className="h-4 w-4" />
								<span>Settings</span>
							</button>

							<Separator className="my-3" />

							<Link
								to="/"
								className="flex items-center justify-between rounded-lg px-3 py-2.5 text-sm font-medium text-muted-foreground hover:bg-accent hover:text-accent-foreground"
								onClick={() => setDrawerOpen(false)}
							>
								<div className="flex items-center gap-3">
									<Activity className="h-4 w-4" />
									<span>Server Control Plane</span>
								</div>
								<ExternalLink className="h-3.5 w-3.5 opacity-60" />
							</Link>
						</nav>

						<div className="mt-auto border-t border-border pt-4">
							{isRunning ? (
								<Button
									variant="destructive"
									size="sm"
									className="w-full gap-2 text-xs"
									onClick={() => {
										handleDisconnect();
										setDrawerOpen(false);
									}}
								>
									<Power className="h-3.5 w-3.5" />
									<span>Disconnect Tunnel</span>
								</Button>
							) : (
								<Button
									variant="default"
									size="sm"
									className="w-full gap-2 text-xs"
									onClick={() => {
										handleConnect();
										setDrawerOpen(false);
									}}
								>
									<Power className="h-3.5 w-3.5" />
									<span>Connect Tunnel</span>
								</Button>
							)}
						</div>
					</div>
				</div>
			) : null}

			<div className="mx-auto flex h-full max-h-screen w-full max-w-5xl flex-1 min-h-0 flex-col gap-2 p-2 sm:p-3 overflow-hidden">
				{/* Compact Window Header Bar */}
				<div
					data-tauri-drag-region
					className="flex flex-none select-none items-center justify-between gap-1.5 rounded-lg border border-border bg-card px-2.5 py-1.5 shadow-xs cursor-default"
				>
					<div className="flex items-center gap-1.5 min-w-0" data-tauri-drag-region>
						<Button
							variant="ghost"
							size="icon"
							onClick={() => setDrawerOpen(true)}
							className="h-7 w-7 text-muted-foreground hover:text-foreground"
							aria-label="Open navigation drawer"
						>
							<Menu className="h-4 w-4" />
						</Button>

						<img
							src="/logo192.png"
							alt="Prism"
							data-tauri-drag-region
							className="h-7 w-7 flex-none rounded-md object-contain shadow-xs ring-1 ring-primary/20"
						/>

						<div className="flex items-center gap-1.5 min-w-0" data-tauri-drag-region>
							<h1
								data-tauri-drag-region
								className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground"
							>
								Prism Connect
							</h1>
							{isConnected ? (
								<span className="flex items-center gap-1 rounded-full bg-emerald-500/10 px-1.5 py-0.2 text-[9px] font-semibold text-emerald-500 ring-1 ring-emerald-500/30">
									<span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-500" />
									ONLINE
								</span>
							) : isConnecting ? (
								<span className="flex items-center gap-1 rounded-full bg-amber-500/10 px-1.5 py-0.2 text-[9px] font-semibold text-amber-500 ring-1 ring-amber-500/30">
									<span className="h-1.5 w-1.5 animate-ping rounded-full bg-amber-500" />
									CONNECTING
								</span>
							) : (
								<span className="flex items-center gap-1 rounded-full bg-muted px-1.5 py-0.2 text-[9px] font-medium text-muted-foreground">
									<span className="h-1.5 w-1.5 rounded-full bg-muted-foreground/50" />
									OFFLINE
								</span>
							)}
						</div>
					</div>

					{/* Header Actions */}
					<div className="flex items-center gap-1 flex-none">
						{profiles.length > 0 ? (
							<select
								aria-label="Select Profile"
								value={selectedProfileId}
								onChange={(e) => handleSelectProfile(e.target.value)}
								className="h-7 max-w-[110px] sm:max-w-[160px] truncate rounded-md border border-input bg-background px-1.5 text-xs font-medium text-foreground outline-none focus:ring-1 focus:ring-ring"
							>
								{profiles.map((p) => (
									<option key={p.id} value={p.id}>
										{p.name}
									</option>
								))}
							</select>
						) : null}

						<Button
							variant="outline"
							size="icon-xs"
							onClick={() => setImportModalOpen(true)}
							className="h-7 w-7 text-xs"
							title="Import profile"
						>
							<Download className="h-3.5 w-3.5 text-primary" />
						</Button>

						<Button
							variant="outline"
							size="icon-xs"
							onClick={handleShareLink}
							className="h-7 w-7 text-xs"
							title="Share profile link"
						>
							{copied === "share" ? (
								<Check className="h-3.5 w-3.5 text-emerald-500" />
							) : (
								<Share2 className="h-3.5 w-3.5 text-primary" />
							)}
						</Button>

						<Link
							to="/"
							className="hidden md:inline-flex items-center gap-1 rounded-md border border-border/80 px-2 py-1 text-xs font-medium text-muted-foreground transition hover:bg-accent hover:text-accent-foreground"
							title="Open Server Control Plane"
						>
							<Activity className="h-3 w-3 text-primary" />
							<span>Control Plane</span>
							<ExternalLink className="h-2.5 w-2.5 opacity-60" />
						</Link>

						{isDesktop ? (
							<div className="flex items-center gap-0.5 ml-0.5 pl-1 border-l border-border/60">
								<Button
									variant="ghost"
									size="icon-xs"
									onClick={() => void minimizeWindow()}
									className="h-7 w-7 text-muted-foreground hover:bg-accent hover:text-foreground"
									title="Minimize"
									aria-label="Minimize window"
								>
									<Minus className="h-3.5 w-3.5" />
								</Button>
								<Button
									variant="ghost"
									size="icon-xs"
									onClick={() => void closeWindow()}
									className="h-7 w-7 text-muted-foreground hover:bg-destructive/15 hover:text-destructive transition-colors"
									title="Close to tray"
									aria-label="Close to tray"
								>
									<X className="h-3.5 w-3.5" />
								</Button>
							</div>
						) : null}
					</div>
				</div>

				{error ? (
					<div className="flex flex-none items-center justify-between gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-2.5 py-1 text-xs text-destructive">
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

				{/* High-Density Connection Control Card */}
				<div className="flex-none rounded-lg border border-border bg-card p-2.5 shadow-xs">
					{/* Top row: Profile info + Start/Stop button */}
					<div className="flex items-center justify-between gap-2">
						<div className="flex items-center gap-2 min-w-0 flex-1">
							<div
								className={cn(
									"flex h-7 w-7 flex-none items-center justify-center rounded-md text-white transition-colors",
									isConnected
										? "bg-emerald-600"
										: isConnecting
											? "bg-amber-600 animate-pulse"
											: "bg-muted text-muted-foreground",
								)}
							>
								{isConnecting ? (
									<RotateCcw className="h-3.5 w-3.5 animate-spin" />
								) : (
									<Radio className="h-3.5 w-3.5" />
								)}
							</div>
							<div className="min-w-0 flex-1">
								<div className="flex items-center gap-1.5">
									<span className="font-bold text-xs text-foreground truncate">
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
										{status?.server_addr || serverAddr || "No Server"}
									</span>
									<span>&rarr;</span>
									<span className="truncate">{status?.listen_addr || listenAddr}</span>
								</div>
							</div>
						</div>

						{/* Action Button */}
						<Button
							size="sm"
							variant={isRunning ? "destructive" : "default"}
							disabled={actionLoading}
							onClick={handleToggleTunnel}
							className={cn(
								"h-7 px-3 text-xs font-bold gap-1 rounded-md flex-none transition-all shadow-xs",
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
							<span>{isRunning ? "Connected" : "Connect"}</span>
						</Button>
					</div>

					{/* Connected Metrics Strip */}
					{isConnected ? (
						<div className="mt-2 border-t border-border/60 pt-1.5 space-y-1.5">
							{/* 4-col compact stats */}
							<div className="grid grid-cols-4 gap-1 text-center font-mono">
								<div className="rounded bg-muted/40 px-1 py-0.5">
									<div className="text-[9px] uppercase text-muted-foreground">Uptime</div>
									<div className="text-[11px] font-bold text-foreground truncate">
										{formatUptime(uptimeSeconds)}
									</div>
								</div>
								<div className="rounded bg-muted/40 px-1 py-0.5">
									<div className="text-[9px] uppercase text-muted-foreground">Raw</div>
									<div className="text-[11px] font-bold text-foreground truncate">
										{formatBytes(rawBytes)}
									</div>
								</div>
								<div className="rounded bg-muted/40 px-1 py-0.5">
									<div className="text-[9px] uppercase text-muted-foreground">Wire</div>
									<div className="text-[11px] font-bold text-foreground truncate">
										{formatBytes(wireBytes)}
									</div>
								</div>
								<div className="rounded bg-muted/40 px-1 py-0.5">
									<div className="text-[9px] uppercase text-emerald-500">Saved</div>
									<div className="text-[11px] font-bold text-emerald-500 truncate">
										{savedRatio.toFixed(1)}%
									</div>
								</div>
							</div>

							{/* Throughput and LAN bar */}
							<div className="flex items-center justify-between gap-2 px-0.5 text-[10px] text-muted-foreground">
								<div className="flex items-center gap-1.5 flex-1 min-w-0">
									<Activity className="h-3 w-3 text-emerald-500 flex-none" />
									<span className="font-mono text-emerald-500 font-semibold flex-none text-[10px]">
										{formatBytes(throughputSamples[throughputSamples.length - 1] || 0)}/s
									</span>
									<div className="w-20 h-4 flex-none overflow-hidden">
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
											className="ml-0.5 hover:opacity-80"
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
				</div>

				{/* High-Density Tabs Section: Overview, Logs, Profiles, Settings */}
				<Tabs
					value={activeTab}
					onValueChange={setActiveTab}
					className="flex-1 min-h-0 flex flex-col overflow-hidden"
				>
					<TabsList className="grid w-full grid-cols-4 h-7 p-0.5 bg-muted/60 rounded-md flex-none">
						<TabsTrigger value="overview" className="h-6 gap-1 px-1 text-[11px]">
							<Layers className="h-3 w-3 flex-none" />
							<span>Overview</span>
						</TabsTrigger>
						<TabsTrigger value="logs" className="h-6 gap-1 px-1 text-[11px]">
							<Terminal className="h-3 w-3 flex-none" />
							<span>Logs</span>
							{logs.length > 0 ? (
								<span className="rounded-full bg-background px-1 py-0 text-[9px] font-bold">
									{logs.length}
								</span>
							) : null}
						</TabsTrigger>
						<TabsTrigger value="profiles" className="h-6 gap-1 px-1 text-[11px]">
							<Server className="h-3 w-3 flex-none" />
							<span>Profiles</span>
						</TabsTrigger>
						<TabsTrigger value="settings" className="h-6 gap-1 px-1 text-[11px]">
							<Settings2 className="h-3 w-3 flex-none" />
							<span>Settings</span>
						</TabsTrigger>
					</TabsList>

					{/* 1. Overview Tab: Discovered Services */}
					<TabsContent
						value="overview"
						className="flex-1 min-h-0 flex flex-col overflow-hidden mt-1.5 p-0"
					>
						<div className="flex-1 min-h-0 flex flex-col overflow-hidden rounded-lg border border-border bg-card p-2">
							<div className="flex items-center justify-between pb-1.5 border-b border-border/50 flex-none">
								<span className="text-xs font-semibold text-foreground">
									Discovered Remote Services
								</span>
								<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4">
									{status?.known_services.length || 0} active
								</Badge>
							</div>

							<div className="flex-1 min-h-0 overflow-y-auto divide-y divide-border/40 pt-1">
								{status?.known_services && status.known_services.length > 0 ? (
									status.known_services.map((svc) => (
										<div key={svc.name} className="flex items-center justify-between gap-2 py-1.5">
											<div className="flex items-center gap-2 min-w-0 flex-1">
												<div className="flex h-6 w-6 flex-none items-center justify-center rounded bg-primary/10 text-primary">
													<Radio className="h-3 w-3" />
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
													<div className="font-mono text-[10px] text-muted-foreground truncate">
														{svc.masquerade_host || "Direct Tunnel"}
													</div>
												</div>
											</div>

											<Button
												variant="outline"
												size="xs"
												onClick={() => copyText(status?.listen_addr || listenAddr, svc.name)}
												className="gap-1 text-[10px] h-6 px-2 flex-none"
											>
												{copied === svc.name ? (
													<Check className="h-3 w-3 text-emerald-500" />
												) : (
													<Copy className="h-3 w-3" />
												)}
												<span>{copied === svc.name ? "Copied" : "Copy"}</span>
											</Button>
										</div>
									))
								) : (
									<div className="flex h-full flex-col items-center justify-center py-6 text-center text-muted-foreground">
										<WifiOff className="mb-1.5 h-6 w-6 text-muted-foreground/50" />
										<p className="text-xs">
											{isConnected
												? "Waiting for Connector to publish game services..."
												: "Connect to a Prism server to view services."}
										</p>
									</div>
								)}
							</div>
						</div>
					</TabsContent>

					{/* 2. Client Logs Tab: Real-Time Terminal View */}
					<TabsContent
						value="logs"
						className="flex-1 min-h-0 flex flex-col overflow-hidden mt-1.5 p-0"
					>
						<div className="flex-1 min-h-0 flex flex-col overflow-hidden rounded-lg border border-border bg-card p-2">
							{/* Toolbar */}
							<div className="flex items-center justify-between gap-1 flex-none pb-1.5 flex-wrap">
								{/* Level Filters */}
								<div className="flex items-center rounded border border-input p-0.5 text-[10px]">
									{(["ALL", "INFO", "WARN", "ERROR"] as const).map((lvl) => (
										<button
											key={lvl}
											type="button"
											onClick={() => setLogFilterLevel(lvl)}
											className={cn(
												"rounded px-1.5 py-0.5 font-semibold transition",
												logFilterLevel === lvl
													? "bg-primary text-primary-foreground"
													: "text-muted-foreground hover:text-foreground",
											)}
										>
											{lvl}
										</button>
									))}
								</div>

								{/* Search */}
								<div className="relative flex-1 min-w-[90px] max-w-[130px]">
									<Search className="pointer-events-none absolute top-1/2 left-2 h-3 w-3 -translate-y-1/2 text-muted-foreground" />
									<Input
										placeholder="Filter..."
										value={logSearchQuery}
										onChange={(e) => setLogSearchQuery(e.target.value)}
										className="h-6 pl-6 pr-1 text-[10px] font-mono"
									/>
								</div>

								<div className="flex items-center gap-1">
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
										className="h-6 text-[10px] px-1.5"
									>
										Scroll: {autoScrollLogs ? (isAtBottom ? "ON" : "PAUSED") : "OFF"}
									</Button>

									<Button
										variant="outline"
										size="xs"
										onClick={handleClearLogs}
										className="h-6 text-[10px] px-1.5 text-destructive hover:bg-destructive/10"
									>
										Clear
									</Button>

									<Button
										variant="outline"
										size="xs"
										onClick={handleCopyAllLogs}
										className="h-6 text-[10px] px-1.5 gap-1"
									>
										{copied === "all-logs" ? (
											<Check className="h-2.5 w-2.5 text-emerald-500" />
										) : (
											<Copy className="h-2.5 w-2.5" />
										)}
										<span>{copied === "all-logs" ? "Copied" : "Copy"}</span>
									</Button>
								</div>
							</div>

							{/* Terminal Window */}
							<div className="relative flex-1 min-h-0 flex flex-col">
								<div
									ref={logsContainerRef}
									onScroll={handleLogsScroll}
									className="flex-1 min-h-0 overflow-y-auto rounded border border-border bg-slate-950 p-2 font-mono text-[10px] leading-snug text-slate-200 selection:bg-primary/30 scrollbar-thin"
								>
									{filteredLogs.length > 0 ? (
										<div className="flex flex-col gap-0.5">
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
														className="flex items-start gap-1 leading-snug hover:bg-white/5 px-0.5 py-0.2 rounded"
													>
														<span className="shrink-0 text-slate-500 selection:text-slate-300 text-[9.5px]">
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
																"shrink-0 rounded px-1 py-0 text-[8.5px] font-bold border",
																badgeColor,
															)}
														>
															{entry.level}
														</span>
														<span className="shrink-0 text-slate-400">{entry.target}:</span>
														<span className="break-all text-slate-100">{entry.message}</span>
													</div>
												);
											})}
										</div>
									) : (
										<div className="flex h-full flex-col items-center justify-center text-slate-500 py-4">
											<Terminal className="mb-1 h-5 w-5 opacity-40" />
											<p className="text-xs">No client logs recorded yet.</p>
										</div>
									)}
								</div>

								{/* Floating Jump to Bottom Button when user is scrolled up */}
								{!isAtBottom && filteredLogs.length > 0 ? (
									<button
										type="button"
										onClick={() => {
											setAutoScrollLogs(true);
											scrollToBottom(true);
										}}
										className="absolute bottom-2 right-2.5 z-10 flex items-center gap-1 rounded-full bg-primary/90 hover:bg-primary text-primary-foreground px-2 py-0.5 text-[10px] font-medium shadow-md transition-all duration-150 backdrop-blur"
									>
										<ArrowDown className="h-3 w-3" />
										<span>Latest</span>
									</button>
								) : null}
							</div>
						</div>
					</TabsContent>

					{/* 3. Profiles Tab: Manage & Import/Export */}
					<TabsContent
						value="profiles"
						className="flex-1 min-h-0 flex flex-col overflow-hidden mt-1.5 p-0"
					>
						<div className="flex-1 min-h-0 flex flex-col overflow-hidden rounded-lg border border-border bg-card p-2">
							<div className="flex items-center justify-between pb-1.5 border-b border-border/50 flex-none">
								<span className="text-xs font-semibold text-foreground">Saved Tunnel Profiles</span>
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
									className="h-6 gap-1 text-[11px] px-2"
								>
									<Plus className="h-3 w-3" />
									<span>Add New</span>
								</Button>
							</div>

							<div className="flex-1 min-h-0 overflow-y-auto divide-y divide-border/40 pt-1">
								{profiles.length > 0 ? (
									profiles.map((p) => {
										const isSelected = p.id === selectedProfileId;
										return (
											<div key={p.id} className="flex items-center justify-between py-1.5 gap-2">
												<div className="flex items-center gap-1.5 min-w-0 flex-1">
													<Button
														variant={isSelected ? "default" : "outline"}
														size="xs"
														onClick={() => handleSelectProfile(p.id)}
														className="text-[10px] h-6 px-1.5 flex-none"
													>
														{isSelected ? "Active" : "Select"}
													</Button>
													<div className="min-w-0 flex-1">
														<div className="font-semibold text-foreground truncate text-xs">
															{p.name}
														</div>
														<div className="font-mono text-[10px] text-muted-foreground truncate">
															{p.server_addr} ({p.transport.toUpperCase()}) &bull; Local:{" "}
															{p.listen_addr}
														</div>
													</div>
												</div>

												<div className="flex items-center gap-1 flex-none">
													<Button
														variant="outline"
														size="icon-xs"
														onClick={() => {
															handleSelectProfile(p.id);
															setActiveTab("settings");
														}}
														className="h-6 w-6 p-0"
														title="Edit profile settings"
													>
														<Settings2 className="h-3 w-3" />
													</Button>
													<Button
														variant="outline"
														size="icon-xs"
														onClick={() => handleDeleteProfile(p.id)}
														className="h-6 w-6 p-0 text-destructive hover:bg-destructive/10"
														title="Delete profile"
													>
														<Trash2 className="h-3 w-3" />
													</Button>
												</div>
											</div>
										);
									})
								) : (
									<div className="py-6 text-center text-xs text-muted-foreground">
										No saved profiles yet. Click "Add New" or import a link.
									</div>
								)}
							</div>
						</div>
					</TabsContent>

					{/* 4. Settings Tab: Connection Configuration & GitHub Auth */}
					<TabsContent value="settings" className="flex-1 min-h-0 overflow-y-auto mt-1.5 p-0">
						<div className="rounded-lg border border-border bg-card p-2.5 space-y-2">
							<div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
								<div className="space-y-0.5">
									<label className="text-[10px] uppercase font-bold text-muted-foreground">
										Profile Name
									</label>
									<Input
										value={profileName}
										onChange={(e) => setProfileName(e.target.value)}
										placeholder="e.g. My Realm"
										className="h-7 text-xs"
									/>
								</div>

								<div className="space-y-0.5">
									<label className="text-[10px] uppercase font-bold text-muted-foreground">
										Relay Server Address
									</label>
									<Input
										value={serverAddr}
										onChange={(e) => setServerAddr(e.target.value)}
										placeholder="relay.example.com:7000"
										className="h-7 text-xs font-mono"
									/>
								</div>
							</div>

							<div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
								<div className="space-y-0.5">
									<label className="text-[10px] uppercase font-bold text-muted-foreground">
										Transport Protocol
									</label>
									<select
										aria-label="Transport Protocol"
										value={transport}
										onChange={(e) => setTransport(e.target.value)}
										className="h-7 w-full rounded border border-input bg-background px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-ring"
									>
										<option value="quic">QUIC (Fast & Resilient)</option>
										<option value="kcp">KCP (Low Latency UDP)</option>
										<option value="tcp">TCP (Standard)</option>
										<option value="websocket">WebSocket (WS / WSS)</option>
									</select>
								</div>

								<div className="space-y-0.5">
									<label className="text-[10px] uppercase font-bold text-muted-foreground">
										Local Port / Ingress
									</label>
									<Input
										value={listenAddr}
										onChange={(e) => setListenAddr(e.target.value)}
										placeholder="127.0.0.1:25565"
										className="h-7 text-xs font-mono"
									/>
								</div>
							</div>

							{/* Auth Token with GitHub Quick Login */}
							<div className="space-y-0.5">
								<div className="flex items-center justify-between">
									<label className="text-[10px] uppercase font-bold text-muted-foreground">
										Authentication Token
									</label>
									<Button
										type="button"
										variant="outline"
										size="xs"
										onClick={() => {
											setAuthServerUrl(managementUrl || "http://127.0.0.1:8080");
											setGithubAuthOpen(true);
										}}
										className="h-5 text-[10px] px-1.5 gap-1 text-primary"
									>
										<Github className="h-2.5 w-2.5" />
										<span>GitHub 1-Click Login</span>
									</Button>
								</div>
								<div className="relative">
									<Input
										type={showToken ? "text" : "password"}
										value={authToken}
										onChange={(e) => setAuthToken(e.target.value)}
										placeholder="Optional server auth token"
										className="h-7 text-xs pr-7 font-mono"
									/>
									<button
										type="button"
										onClick={() => setShowToken(!showToken)}
										className="absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
									>
										{showToken ? <EyeOff className="h-3 w-3" /> : <Eye className="h-3 w-3" />}
									</button>
								</div>
							</div>

							{/* Toggles */}
							<div className="space-y-1.5 pt-1">
								<div className="flex items-center justify-between rounded border border-border/60 p-1.5 text-xs">
									<div>
										<div className="font-medium text-[11px] text-foreground">
											Minecraft LAN Discovery
										</div>
										<div className="text-[9.5px] text-muted-foreground">
											Broadcasts for LAN Games list
										</div>
									</div>
									<Switch
										checked={fakeLanBroadcast}
										onCheckedChange={setFakeLanBroadcast}
										className="scale-75 origin-right"
									/>
								</div>

								<div className="flex items-center justify-between rounded border border-border/60 p-1.5 text-xs">
									<div>
										<div className="font-medium text-[11px] text-foreground">
											Auto-Connect Panel
										</div>
										<div className="text-[9.5px] text-muted-foreground">
											Sync control panel ({managementUrl})
										</div>
									</div>
									<Switch
										checked={autoConnectPanel}
										onCheckedChange={setAutoConnectPanel}
										className="scale-75 origin-right"
									/>
								</div>
							</div>

							{/* Footer */}
							<div className="flex items-center justify-between pt-1 border-t border-border/50">
								{selectedProfileId ? (
									<Button
										variant="outline"
										size="xs"
										onClick={() => handleDeleteProfile(selectedProfileId)}
										className="h-6 text-[10px] text-destructive hover:bg-destructive/10"
									>
										Delete
									</Button>
								) : (
									<div />
								)}

								<Button
									size="xs"
									onClick={handleSaveProfile}
									className="h-6 text-[10px] gap-1 px-2.5"
								>
									<Check className="h-3 w-3" />
									<span>Save Profile</span>
								</Button>
							</div>
						</div>
					</TabsContent>
				</Tabs>

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
									placeholder="prism://play.example.com:7000?token=..."
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

				{/* GitHub Device Auth Modal */}
				{githubAuthOpen ? (
					<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-3 sm:p-4 backdrop-blur-xs">
						<Card className="w-full max-w-sm sm:max-w-lg shadow-xl">
							<CardHeader className="flex flex-row items-center justify-between pb-3">
								<div className="flex items-center gap-2">
									<Github className="h-5 w-5" />
									<CardTitle>GitHub Device Authorization</CardTitle>
								</div>
								<Button
									variant="ghost"
									size="icon-xs"
									onClick={() => {
										setGithubAuthOpen(false);
										setDeviceCode(null);
										setDevicePolling(false);
										setAuthError(null);
									}}
								>
									<X className="h-4 w-4" />
								</Button>
							</CardHeader>
							<CardContent className="space-y-4">
								<p className="text-sm text-muted-foreground">
									Authenticate via GitHub to fetch a client access token from the relay server.
								</p>

								<div className="space-y-1.5">
									<label className="text-xs font-medium text-muted-foreground">
										Auth Server Address
									</label>
									<Input
										value={authServerUrl}
										onChange={(e) => setAuthServerUrl(e.target.value)}
										disabled={devicePolling}
									/>
								</div>

								{authError ? (
									<div className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive">
										{authError}
									</div>
								) : null}

								{deviceSuccess ? (
									<div className="flex items-center gap-2.5 rounded-lg border border-emerald-500/30 bg-emerald-500/10 p-3 text-emerald-500">
										<CheckCircle2 className="h-5 w-5 flex-none" />
										<div className="text-xs">
											<p className="font-semibold">GitHub Authorization Succeeded!</p>
											<p>Token automatically applied to profile settings.</p>
										</div>
									</div>
								) : deviceCode ? (
									<div className="space-y-3 rounded-lg border border-border bg-muted/40 p-3 sm:p-4">
										<div className="text-xs text-muted-foreground">
											Enter this 8-digit device code on the GitHub verification page:
										</div>
										<div className="flex items-center justify-between rounded-lg border border-border bg-background px-3 py-2 sm:px-4 sm:py-2.5">
											<span className="font-mono text-lg sm:text-xl font-bold tracking-wider text-primary">
												{deviceCode.user_code}
											</span>
											<Button
												variant="outline"
												size="xs"
												onClick={() => copyText(deviceCode.user_code, "user-code")}
												className="gap-1 text-xs"
											>
												{copied === "user-code" ? (
													<Check className="h-3 w-3 text-emerald-500" />
												) : (
													<Copy className="h-3 w-3" />
												)}
												<span>{copied === "user-code" ? "Copied" : "Copy"}</span>
											</Button>
										</div>

										<div className="flex flex-wrap items-center justify-between gap-1.5 pt-1">
											<a
												href={deviceCode.verification_uri}
												target="_blank"
												rel="noreferrer"
												className="inline-flex items-center gap-1 text-xs font-medium text-primary hover:underline"
											>
												<span>Open GitHub Verification</span>
												<ExternalLink className="h-3 w-3" />
											</a>
											{devicePolling ? (
												<span className="flex items-center gap-1.5 text-xs text-muted-foreground">
													<span className="h-2 w-2 animate-ping rounded-full bg-primary" />
													Waiting for authorization...
												</span>
											) : null}
										</div>
									</div>
								) : (
									<Button
										onClick={startDeviceAuth}
										disabled={deviceLoading}
										className="w-full gap-2"
									>
										<Github className="h-4 w-4" />
										<span>
											{deviceLoading ? "Requesting code..." : "Request Device Code Login"}
										</span>
									</Button>
								)}
							</CardContent>
							<CardFooter className="flex justify-end border-t border-border pt-4">
								<Button
									variant="outline"
									onClick={() => {
										setGithubAuthOpen(false);
										setDeviceCode(null);
										setDevicePolling(false);
										setAuthError(null);
									}}
								>
									Close
								</Button>
							</CardFooter>
						</Card>
					</div>
				) : null}
			</div>
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
