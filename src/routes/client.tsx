import { createFileRoute, Link } from "@tanstack/react-router";
import {
	Activity,
	Check,
	CheckCircle2,
	ChevronRight,
	Clock,
	Copy,
	Download,
	ExternalLink,
	Eye,
	EyeOff,
	Gamepad2,
	Github,
	Layers,
	Menu,
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
	Wifi,
	WifiOff,
	X,
	Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

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
	const effectiveConnection = useMemo(
		() =>
			connection ?? {
				baseUrl:
					typeof window !== "undefined" &&
					((window as unknown as { __TAURI__?: unknown }).__TAURI__ ||
						(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)
						? "http://127.0.0.1:8080"
						: "",
				token: "",
			},
		[connection],
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
	const logsEndRef = useRef<HTMLDivElement | null>(null);

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

	// Fetch logs
	const fetchLogs = useCallback(() => {
		getClientLogs(effectiveConnection, 300)
			.then((entries) => {
				setLogs(entries);
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

	// Auto-scroll logs to bottom
	useEffect(() => {
		if (autoScrollLogs && logsEndRef.current) {
			logsEndRef.current.scrollIntoView({ behavior: "smooth" });
		}
	}, [logs, autoScrollLogs]);

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

	const savedBytes = status?.stats.saved_bytes ?? 0;
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

	return (
		<div className="relative min-h-screen bg-background">
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
								<div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/10 text-primary shadow-xs ring-1 ring-primary/20">
									<Radio className="h-5 w-5" />
								</div>
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

			<div className="mx-auto flex max-w-5xl flex-col gap-6 px-4 py-6 md:px-8">
				{/* OpenVPN Connect Style Window Header Bar */}
				<div className="flex flex-col gap-4 rounded-2xl border border-border bg-card p-4 shadow-xs sm:flex-row sm:items-center sm:justify-between">
					<div className="flex items-center gap-3">
						<Button
							variant="ghost"
							size="icon"
							onClick={() => setDrawerOpen(true)}
							className="-ml-1 h-10 w-10 text-muted-foreground hover:text-foreground"
							aria-label="Open navigation drawer"
						>
							<Menu className="h-5 w-5" />
						</Button>

						<div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary shadow-xs ring-1 ring-primary/20">
							<Radio className="h-5 w-5" />
						</div>
						<div>
							<div className="flex items-center gap-2">
								<h1 className="text-base font-bold tracking-tight text-foreground">
									Prism Connect
								</h1>
								{isConnected ? (
									<Badge
										variant="outline"
										className="border-emerald-500/30 bg-emerald-500/10 text-emerald-500"
									>
										<span className="mr-1.5 h-2 w-2 animate-pulse rounded-full bg-emerald-500" />
										CONNECTED
									</Badge>
								) : isConnecting ? (
									<Badge
										variant="outline"
										className="border-amber-500/30 bg-amber-500/10 text-amber-500"
									>
										<span className="mr-1.5 h-2 w-2 animate-ping rounded-full bg-amber-500" />
										CONNECTING
									</Badge>
								) : (
									<Badge
										variant="outline"
										className="border-muted-foreground/30 text-muted-foreground"
									>
										<span className="mr-1.5 h-2 w-2 rounded-full bg-muted-foreground/50" />
										DISCONNECTED
									</Badge>
								)}
							</div>
							<p className="text-xs text-muted-foreground">
								High-performance transparent proxy tunnel with L7 traffic optimizer
							</p>
						</div>
					</div>

					{/* Header Actions */}
					<div className="flex flex-wrap items-center gap-2">
						{profiles.length > 0 ? (
							<select
								aria-label="Select Profile"
								value={selectedProfileId}
								onChange={(e) => handleSelectProfile(e.target.value)}
								className="h-8 rounded-lg border border-input bg-background px-3 text-xs font-medium text-foreground outline-none focus:ring-1 focus:ring-ring"
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
							size="sm"
							onClick={() => setImportModalOpen(true)}
							className="gap-1.5 text-xs"
						>
							<Download className="h-3.5 w-3.5 text-primary" />
							<span>Import</span>
						</Button>

						<Button
							variant="outline"
							size="sm"
							onClick={handleShareLink}
							className="gap-1.5 text-xs"
						>
							{copied === "share" ? (
								<Check className="h-3.5 w-3.5 text-emerald-500" />
							) : (
								<Share2 className="h-3.5 w-3.5 text-primary" />
							)}
							<span>{copied === "share" ? "Copied" : "Share"}</span>
						</Button>
					</div>
				</div>

				{error ? (
					<div className="flex items-center justify-between gap-4 rounded-xl border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
						<span>{error}</span>
						<Button variant="outline" size="xs" onClick={fetchStatus}>
							Retry
						</Button>
					</div>
				) : null}

				{/* Main Hero Card: Iconic OpenVPN Connect Style Connection Switch & Live Metrics */}
				<Card className="overflow-hidden border-border bg-card shadow-sm">
					<div className="p-6 md:p-8">
						<div className="flex flex-col gap-6 md:flex-row md:items-center md:justify-between">
							{/* Left: Profile Title & Server details */}
							<div className="space-y-2">
								<div className="flex items-center gap-2">
									<h2 className="text-2xl font-bold tracking-tight text-foreground">
										{profileName || "Prism Tunnel Profile"}
									</h2>
									<Badge variant="secondary" className="font-mono text-[11px] uppercase">
										{status?.transport || transport}
									</Badge>
								</div>

								<div className="flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
									<div className="flex items-center gap-1.5">
										<Server className="h-4 w-4 text-primary" />
										<span className="font-mono text-foreground">
											{status?.server_addr || serverAddr || "No Server Configured"}
										</span>
									</div>
									<Separator orientation="vertical" className="h-4" />
									<div className="flex items-center gap-1.5">
										<span>Ingress:</span>
										<span className="font-mono text-foreground">
											{status?.listen_addr || listenAddr}
										</span>
									</div>
								</div>

								{connection ? (
									<div className="flex items-center gap-2 pt-1 text-xs text-muted-foreground">
										<span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
										<span>Admin Sync: {connection.baseUrl}</span>
										<Link
											to="/"
											className="inline-flex items-center gap-0.5 text-primary hover:underline"
										>
											<span>Dashboard</span>
											<ChevronRight className="h-3 w-3" />
										</Link>
									</div>
								) : null}
							</div>

							{/* Right: The OpenVPN Style Tactile Big Connect Switch */}
							<div className="flex flex-col items-center gap-2 sm:items-end">
								<div
									onClick={!actionLoading ? handleToggleTunnel : undefined}
									role="button"
									tabIndex={0}
									onKeyDown={(e) => {
										if (e.key === "Enter" || e.key === " ") {
											handleToggleTunnel();
										}
									}}
									className={cn(
										"relative flex h-14 w-32 cursor-pointer items-center rounded-full p-1.5 transition-all duration-300 select-none",
										isRunning
											? "bg-emerald-500 shadow-md shadow-emerald-500/20"
											: "bg-muted ring-1 ring-border hover:bg-muted/80",
										actionLoading && "cursor-not-allowed opacity-60",
									)}
								>
									{/* Thumb */}
									<div
										className={cn(
											"flex h-11 w-11 items-center justify-center rounded-full bg-background text-foreground shadow-md transition-all duration-300",
											isRunning
												? "translate-x-[4.3rem] text-emerald-600"
												: "translate-x-0 text-muted-foreground",
										)}
									>
										{actionLoading ? (
											<RotateCcw className="h-5 w-5 animate-spin text-primary" />
										) : isRunning ? (
											<Power className="h-5 w-5" />
										) : (
											<Power className="h-5 w-5" />
										)}
									</div>

									{/* Internal Status Text */}
									<span
										className={cn(
											"absolute text-xs font-bold uppercase tracking-wider transition-opacity duration-300",
											isRunning ? "left-3 text-white" : "right-3 text-muted-foreground",
										)}
									>
										{isRunning ? "ON" : "OFF"}
									</span>
								</div>

								<div className="text-center text-xs font-semibold uppercase tracking-wider sm:text-right">
									{isConnected ? (
										<span className="text-emerald-500">Connected</span>
									) : isConnecting ? (
										<span className="text-amber-500">Connecting...</span>
									) : (
										<span className="text-muted-foreground">Disconnected</span>
									)}
								</div>
							</div>
						</div>

						{/* Active Connection Metrics (Uptime + Traffic Optimizer) */}
						{isConnected ? (
							<div className="mt-8 border-t border-border/80 pt-6">
								<div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
									<div className="rounded-xl border border-border/60 bg-muted/20 p-3.5">
										<div className="flex items-center gap-1.5 text-xs text-muted-foreground">
											<Clock className="h-3.5 w-3.5 text-primary" />
											<span>Connected Uptime</span>
										</div>
										<div className="mt-2 font-mono text-xl font-bold tracking-tight text-foreground">
											{formatUptime(uptimeSeconds)}
										</div>
									</div>

									<div className="rounded-xl border border-border/60 bg-muted/20 p-3.5">
										<div className="flex items-center gap-1.5 text-xs text-muted-foreground">
											<Activity className="h-3.5 w-3.5 text-primary" />
											<span>Raw Traffic</span>
										</div>
										<div className="mt-2 font-mono text-xl font-bold tracking-tight text-foreground">
											{formatBytes(rawBytes)}
										</div>
									</div>

									<div className="rounded-xl border border-border/60 bg-muted/20 p-3.5">
										<div className="flex items-center gap-1.5 text-xs text-muted-foreground">
											<Wifi className="h-3.5 w-3.5 text-primary" />
											<span>Wire Sent</span>
										</div>
										<div className="mt-2 font-mono text-xl font-bold tracking-tight text-foreground">
											{formatBytes(wireBytes)}
										</div>
									</div>

									<div className="rounded-xl border border-border/60 bg-muted/20 p-3.5">
										<div className="flex items-center gap-1.5 text-xs text-muted-foreground">
											<Zap className="h-3.5 w-3.5 text-emerald-500" />
											<span>Saved Bandwidth</span>
										</div>
										<div className="mt-2 font-mono text-xl font-bold tracking-tight text-emerald-500">
											{savedRatio.toFixed(1)}% ({formatBytes(savedBytes)})
										</div>
									</div>
								</div>

								{/* Live Throughput Waveform Graph */}
								<div className="mt-4 rounded-xl border border-border/60 bg-muted/20 p-4">
									<div className="flex items-center justify-between">
										<div className="flex items-center gap-2 text-xs text-muted-foreground">
											<Activity className="h-3.5 w-3.5 text-emerald-500" />
											<span className="font-medium text-foreground">Live Network Throughput</span>
										</div>
										<span className="font-mono text-xs font-semibold text-emerald-500">
											{formatBytes(throughputSamples[throughputSamples.length - 1] || 0)}/s
										</span>
									</div>
									<div className="mt-3 flex items-end justify-between gap-4">
										<div className="flex-1 overflow-hidden">
											<ThroughputSparkline samples={throughputSamples} />
										</div>
										<div className="flex-none text-right text-[11px] text-muted-foreground">
											<span>Zstd L7 Optimizer: {savedRatio.toFixed(1)}% Saved</span>
										</div>
									</div>
								</div>

								{/* Bandwidth Compression Progress Bar */}
								<div className="mt-4">
									<div className="h-2 w-full overflow-hidden rounded-full bg-muted">
										<div
											className="h-full rounded-full bg-emerald-500 transition-all duration-500"
											style={{ width: `${Math.min(100, Math.max(0, savedRatio))}%` }}
										/>
									</div>
								</div>
							</div>
						) : null}

						{/* Minecraft LAN Auto-Discovery Highlight Card */}
						{fakeLanBroadcast && isConnected ? (
							<div className="mt-6 flex flex-col gap-3 rounded-xl border border-primary/20 bg-primary/5 p-4 sm:flex-row sm:items-center sm:justify-between">
								<div className="flex items-center gap-3">
									<div className="flex h-9 w-9 flex-none items-center justify-center rounded-lg bg-primary/15 text-primary">
										<Gamepad2 className="h-5 w-5" />
									</div>
									<div>
										<div className="text-sm font-semibold text-foreground">
											Minecraft LAN Auto-Discovery Active
										</div>
										<div className="text-xs text-muted-foreground">
											Your server will appear directly under <b>Multiplayer &gt; LAN Games</b>!
										</div>
									</div>
								</div>

								<Button
									variant="outline"
									size="sm"
									onClick={() => copyText(status?.listen_addr || listenAddr, "lan-btn")}
									className="gap-1.5 text-xs"
								>
									{copied === "lan-btn" ? (
										<Check className="h-3.5 w-3.5 text-emerald-500" />
									) : (
										<Copy className="h-3.5 w-3.5" />
									)}
									<span>{copied === "lan-btn" ? "Copied" : status?.listen_addr || listenAddr}</span>
								</Button>
							</div>
						) : null}
					</div>
				</Card>

				{/* OpenVPN Connect Style Tabs Section: Overview, Logs, Profiles, Settings */}
				<Tabs value={activeTab} onValueChange={setActiveTab} className="w-full">
					<TabsList className="grid w-full grid-cols-4">
						<TabsTrigger value="overview" className="gap-2 text-xs">
							<Layers className="h-3.5 w-3.5" />
							<span>Overview</span>
						</TabsTrigger>
						<TabsTrigger value="logs" className="gap-2 text-xs">
							<Terminal className="h-3.5 w-3.5" />
							<span>Client Logs</span>
							{logs.length > 0 ? (
								<span className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-bold">
									{logs.length}
								</span>
							) : null}
						</TabsTrigger>
						<TabsTrigger value="profiles" className="gap-2 text-xs">
							<Server className="h-3.5 w-3.5" />
							<span>Profiles</span>
						</TabsTrigger>
						<TabsTrigger value="settings" className="gap-2 text-xs">
							<Settings2 className="h-3.5 w-3.5" />
							<span>Settings</span>
						</TabsTrigger>
					</TabsList>

					{/* 1. Overview Tab: Discovered Services */}
					<TabsContent value="overview" className="mt-4 space-y-4">
						<Card className="shadow-xs">
							<CardHeader className="flex flex-row items-center justify-between pb-3">
								<div>
									<CardTitle className="text-base font-semibold">
										Discovered Remote Services
									</CardTitle>
									<CardDescription>
										Synchronized game & proxy services broadcast by the tunnel relay
									</CardDescription>
								</div>
								<Badge variant="outline">{status?.known_services.length || 0} active</Badge>
							</CardHeader>
							<CardContent>
								{status?.known_services && status.known_services.length > 0 ? (
									<div className="divide-y divide-border">
										{status.known_services.map((svc) => (
											<div
												key={svc.name}
												className="flex flex-col justify-between gap-3 py-3 sm:flex-row sm:items-center"
											>
												<div className="flex items-center gap-3">
													<div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/10 text-primary">
														<Radio className="h-4 w-4" />
													</div>
													<div>
														<div className="flex items-center gap-2">
															<span className="font-semibold text-foreground">{svc.name}</span>
															<Badge
																variant="secondary"
																className="font-mono text-[10px] uppercase"
															>
																{svc.proto}
															</Badge>
														</div>
														<div className="font-mono text-xs text-muted-foreground">
															{svc.masquerade_host || "Direct Tunnel"}
														</div>
													</div>
												</div>

												<Button
													variant="outline"
													size="sm"
													onClick={() => copyText(status?.listen_addr || listenAddr, svc.name)}
													className="gap-1.5 text-xs"
												>
													{copied === svc.name ? (
														<Check className="h-3.5 w-3.5 text-emerald-500" />
													) : (
														<Copy className="h-3.5 w-3.5" />
													)}
													<span>{copied === svc.name ? "Copied" : "Copy Join Address"}</span>
												</Button>
											</div>
										))}
									</div>
								) : (
									<div className="flex flex-col items-center justify-center py-10 text-center text-muted-foreground">
										<WifiOff className="mb-2 h-8 w-8 text-muted-foreground/50" />
										<p className="text-sm">
											{isConnected
												? "Waiting for Connector to publish game services..."
												: "Connect to a Prism server to view and synchronize services."}
										</p>
									</div>
								)}
							</CardContent>
						</Card>
					</TabsContent>

					{/* 2. Client Logs Tab: Real-Time Terminal View */}
					<TabsContent value="logs" className="mt-4 space-y-3">
						<Card className="shadow-xs">
							<CardHeader className="flex flex-col gap-3 pb-3 sm:flex-row sm:items-center sm:justify-between">
								<div>
									<CardTitle className="text-base font-semibold">Client Sidecar Logs</CardTitle>
									<CardDescription>
										Real-time diagnostic events generated by the tunnel client runtime
									</CardDescription>
								</div>

								{/* Logs Control Toolbar */}
								<div className="flex flex-wrap items-center gap-2">
									{/* Level Filters */}
									<div className="flex items-center rounded-lg border border-input p-0.5 text-xs">
										{(["ALL", "INFO", "WARN", "ERROR"] as const).map((lvl) => (
											<button
												key={lvl}
												type="button"
												onClick={() => setLogFilterLevel(lvl)}
												className={cn(
													"rounded-md px-2.5 py-1 font-semibold transition",
													logFilterLevel === lvl
														? "bg-primary text-primary-foreground shadow-xs"
														: "text-muted-foreground hover:text-foreground",
												)}
											>
												{lvl}
											</button>
										))}
									</div>

									{/* Auto-scroll toggle */}
									<Button
										variant={autoScrollLogs ? "secondary" : "outline"}
										size="xs"
										onClick={() => setAutoScrollLogs(!autoScrollLogs)}
										className="text-xs"
									>
										Auto-scroll: {autoScrollLogs ? "ON" : "OFF"}
									</Button>

									{/* Clear Logs */}
									<Button
										variant="outline"
										size="xs"
										onClick={handleClearLogs}
										className="text-xs text-destructive hover:bg-destructive/10 hover:text-destructive"
									>
										Clear
									</Button>

									{/* Copy All Logs */}
									<Button
										variant="outline"
										size="xs"
										onClick={handleCopyAllLogs}
										className="gap-1 text-xs"
									>
										{copied === "all-logs" ? (
											<Check className="h-3 w-3 text-emerald-500" />
										) : (
											<Copy className="h-3 w-3" />
										)}
										<span>{copied === "all-logs" ? "Copied" : "Copy"}</span>
									</Button>
								</div>
							</CardHeader>

							<CardContent>
								{/* Filter Input */}
								<div className="relative mb-3">
									<Search className="pointer-events-none absolute top-1/2 left-3 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
									<Input
										placeholder="Search client logs..."
										value={logSearchQuery}
										onChange={(e) => setLogSearchQuery(e.target.value)}
										className="h-8 pl-8 text-xs font-mono"
									/>
								</div>

								{/* Terminal Window */}
								<div className="h-96 w-full overflow-y-auto rounded-lg border border-border bg-slate-950 p-4 font-mono text-xs text-slate-200 selection:bg-primary/30">
									{filteredLogs.length > 0 ? (
										<div className="flex flex-col gap-1.5">
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
														className="flex items-start gap-2.5 leading-relaxed hover:bg-white/5 px-1 py-0.5 rounded"
													>
														<span className="shrink-0 text-slate-500 selection:text-slate-300">
															[{entry.timestamp}]
														</span>
														<span
															className={cn(
																"shrink-0 rounded px-1.5 py-0.2 text-[10px] font-bold border",
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
											<div ref={logsEndRef} />
										</div>
									) : (
										<div className="flex h-full flex-col items-center justify-center text-slate-500">
											<Terminal className="mb-2 h-6 w-6 opacity-40" />
											<p>No client logs recorded yet.</p>
											<p className="text-[11px] text-slate-600">
												Start the tunnel client sidecar to observe real-time events.
											</p>
										</div>
									)}
								</div>
							</CardContent>
						</Card>
					</TabsContent>

					{/* 3. Profiles Tab: Manage & Import/Export */}
					<TabsContent value="profiles" className="mt-4 space-y-4">
						<Card className="shadow-xs">
							<CardHeader className="flex flex-row items-center justify-between pb-3">
								<div>
									<CardTitle className="text-base font-semibold">Saved Tunnel Profiles</CardTitle>
									<CardDescription>
										Quickly switch between configured game realms or tunnel servers
									</CardDescription>
								</div>
								<Button
									variant="outline"
									size="sm"
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
									className="gap-1.5 text-xs"
								>
									<Plus className="h-3.5 w-3.5" />
									<span>Add New</span>
								</Button>
							</CardHeader>
							<CardContent>
								{profiles.length > 0 ? (
									<div className="divide-y divide-border">
										{profiles.map((p) => {
											const isSelected = p.id === selectedProfileId;
											return (
												<div key={p.id} className="flex items-center justify-between py-3">
													<div className="flex items-center gap-3">
														<Button
															variant={isSelected ? "default" : "outline"}
															size="xs"
															onClick={() => handleSelectProfile(p.id)}
															className="text-xs"
														>
															{isSelected ? "Active" : "Select"}
														</Button>
														<div>
															<div className="font-semibold text-foreground">{p.name}</div>
															<div className="font-mono text-xs text-muted-foreground">
																{p.server_addr} ({p.transport.toUpperCase()}) &bull; Local:{" "}
																{p.listen_addr}
															</div>
														</div>
													</div>

													<div className="flex items-center gap-2">
														<Button
															variant="outline"
															size="icon-xs"
															onClick={() => {
																handleSelectProfile(p.id);
																setActiveTab("settings");
															}}
															title="Edit profile settings"
														>
															<Settings2 className="h-3.5 w-3.5" />
														</Button>
														<Button
															variant="outline"
															size="icon-xs"
															onClick={() => handleDeleteProfile(p.id)}
															className="text-destructive hover:bg-destructive/10"
															title="Delete profile"
														>
															<Trash2 className="h-3.5 w-3.5" />
														</Button>
													</div>
												</div>
											);
										})}
									</div>
								) : (
									<div className="py-6 text-center text-sm text-muted-foreground">
										No saved profiles yet. Click "Add New" or import a link.
									</div>
								)}
							</CardContent>
						</Card>
					</TabsContent>

					{/* 4. Settings Tab: Connection Configuration & GitHub Auth */}
					<TabsContent value="settings" className="mt-4 space-y-4">
						<Card className="shadow-xs">
							<CardHeader>
								<CardTitle className="text-base font-semibold">Profile Settings</CardTitle>
								<CardDescription>
									Configure connection parameters for the current profile
								</CardDescription>
							</CardHeader>
							<CardContent className="space-y-4">
								<div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
									<div className="space-y-1.5">
										<label className="text-xs font-medium text-muted-foreground">
											Profile Name
										</label>
										<Input
											value={profileName}
											onChange={(e) => setProfileName(e.target.value)}
											placeholder="e.g. My Survival Realm"
										/>
									</div>

									<div className="space-y-1.5">
										<label className="text-xs font-medium text-muted-foreground">
											Relay Server Address
										</label>
										<Input
											value={serverAddr}
											onChange={(e) => setServerAddr(e.target.value)}
											placeholder="relay.example.com:7000"
										/>
									</div>
								</div>

								<div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
									<div className="space-y-1.5">
										<label className="text-xs font-medium text-muted-foreground">
											Transport Protocol
										</label>
										<select
											aria-label="Transport Protocol"
											value={transport}
											onChange={(e) => setTransport(e.target.value)}
											className="h-9 w-full rounded-lg border border-input bg-background px-3 text-sm text-foreground outline-none focus:ring-1 focus:ring-ring"
										>
											<option value="quic">QUIC (Fast & Resilient)</option>
											<option value="kcp">KCP (Low Latency UDP)</option>
											<option value="tcp">TCP (Standard)</option>
										</select>
									</div>

									<div className="space-y-1.5">
										<label className="text-xs font-medium text-muted-foreground">
											Local Port / Ingress
										</label>
										<Input
											value={listenAddr}
											onChange={(e) => setListenAddr(e.target.value)}
											placeholder="127.0.0.1:25565"
										/>
									</div>
								</div>

								{/* Auth Token with GitHub Quick Login */}
								<div className="space-y-1.5">
									<div className="flex items-center justify-between">
										<label className="text-xs font-medium text-muted-foreground">
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
											className="gap-1.5 text-xs text-primary"
										>
											<Github className="h-3 w-3" />
											<span>GitHub 1-Click Login</span>
										</Button>
									</div>
									<div className="relative">
										<Input
											type={showToken ? "text" : "password"}
											value={authToken}
											onChange={(e) => setAuthToken(e.target.value)}
											placeholder="Server auth token (optional)"
											className="pr-10"
										/>
										<button
											type="button"
											onClick={() => setShowToken(!showToken)}
											className="absolute top-1/2 right-3 -translate-y-1/2 text-muted-foreground hover:text-foreground"
										>
											{showToken ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
										</button>
									</div>
								</div>

								<Separator />

								{/* Toggles */}
								<div className="space-y-3">
									<div className="flex items-center justify-between rounded-lg border border-border p-3">
										<div className="space-y-0.5">
											<div className="text-sm font-medium text-foreground">
												Minecraft Fake LAN Auto-Discovery
											</div>
											<div className="text-xs text-muted-foreground">
												Broadcasts game session on the local network for Minecraft "LAN Games"
											</div>
										</div>
										<Switch checked={fakeLanBroadcast} onCheckedChange={setFakeLanBroadcast} />
									</div>

									<div className="flex items-center justify-between rounded-lg border border-border p-3">
										<div className="space-y-0.5">
											<div className="text-sm font-medium text-foreground">
												Auto-Connect Control Panel
											</div>
											<div className="text-xs text-muted-foreground">
												Automatically sync admin panel session ({managementUrl}) when tunnel
												connects
											</div>
										</div>
										<Switch checked={autoConnectPanel} onCheckedChange={setAutoConnectPanel} />
									</div>
								</div>
							</CardContent>

							<CardFooter className="flex justify-between border-t border-border pt-4">
								{selectedProfileId ? (
									<Button
										variant="outline"
										onClick={() => handleDeleteProfile(selectedProfileId)}
										className="text-destructive hover:bg-destructive/10"
									>
										Delete Profile
									</Button>
								) : (
									<div />
								)}

								<Button onClick={handleSaveProfile} className="gap-2">
									<Check className="h-4 w-4" />
									<span>Save Profile</span>
								</Button>
							</CardFooter>
						</Card>
					</TabsContent>
				</Tabs>

				{/* Import Modal */}
				{importModalOpen ? (
					<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4 backdrop-blur-xs">
						<Card className="w-full max-w-lg shadow-xl">
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
					<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4 backdrop-blur-xs">
						<Card className="w-full max-w-lg shadow-xl">
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
									<div className="space-y-3 rounded-lg border border-border bg-muted/40 p-4">
										<div className="text-xs text-muted-foreground">
											Enter this 8-digit device code on the GitHub verification page:
										</div>
										<div className="flex items-center justify-between rounded-lg border border-border bg-background px-4 py-2.5">
											<span className="font-mono text-xl font-bold tracking-wider text-primary">
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

										<div className="flex items-center justify-between pt-1">
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
	const height = 36;
	const points = samples
		.map((v, i) => {
			const x = (i / (samples.length - 1)) * width;
			const y = height - (v / max) * (height - 8) - 4;
			return `${x.toFixed(1)},${y.toFixed(1)}`;
		})
		.join(" ");

	return (
		<svg viewBox={`0 0 ${width} ${height}`} className="h-9 w-full overflow-visible">
			<polyline
				fill="none"
				stroke="currentColor"
				strokeWidth="2.5"
				strokeLinecap="round"
				strokeLinejoin="round"
				className="text-emerald-500 transition-all duration-300"
				points={points}
			/>
		</svg>
	);
}
