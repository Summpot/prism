import { createFileRoute, Link } from "@tanstack/react-router";
import {
	ArrowRight,
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
	Play,
	Plus,
	Radio,
	Share2,
	Square,
	Trash2,
	X,
	Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Badge, EmptyState, ErrorBanner, PageHeader } from "@/components/ui";
import { formatBytes } from "@/lib/format";
import {
	type ClientProfile,
	type ClientStatusResponse,
	type DeviceCodeResponse,
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

export const Route = createFileRoute("/client")({
	component: ClientDashboardPage,
});

function ClientDashboardPage() {
	const { connection, saveConnection } = usePanelSession();
	const effectiveConnection = useMemo(() => connection ?? { baseUrl: "", token: "" }, [connection]);
	const [status, setStatus] = useState<ClientStatusResponse | null>(null);
	const [profiles, setProfiles] = useState<ClientProfile[]>([]);
	const [selectedProfileId, setSelectedProfileId] = useState<string>("");
	const [actionLoading, setActionLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [copied, setCopied] = useState<string | null>(null);

	// Form state
	const [serverAddr, setServerAddr] = useState("127.0.0.1:7000");
	const [transport, setTransport] = useState("quic");
	const [authToken, setAuthToken] = useState("");
	const [listenAddr, setListenAddr] = useState("127.0.0.1:25565");
	const [fakeLanBroadcast, setFakeLanBroadcast] = useState(true);
	const [profileName, setProfileName] = useState("Default Realm");
	const [showToken, setShowToken] = useState(false);

	// Auto connect management panel state
	const [autoConnectPanel, setAutoConnectPanel] = useState(true);
	const managementUrl = useMemo(
		() => deriveManagementUrl(serverAddr) || "http://127.0.0.1:8080",
		[serverAddr],
	);

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
					// sync active server info if connected
					setServerAddr((prev) => prev || resp.server_addr);
				}
			})
			.catch((err) => {
				// Ignore if offline
				console.debug("Failed to fetch client status:", err);
			});
	}, [effectiveConnection]);

	// Fetch saved profiles
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
			.catch(() => {
				// optional endpoint
			});
	}, [effectiveConnection, selectedProfileId]);

	useEffect(() => {
		fetchStatus();
		fetchProfiles();
	}, [fetchStatus, fetchProfiles]);

	usePolling(fetchStatus, 1_500, true);

	// Auto-connect management panel session when tunnel is running and connected
	useEffect(() => {
		if (!autoConnectPanel) return;
		if (!status?.running || status.state !== "connected") return;

		const targetUrl = managementUrl.trim() || deriveManagementUrl(serverAddr || status.server_addr);
		if (!targetUrl) return;

		// If already connected to targetUrl with this token, no need to re-verify
		if (connection?.baseUrl === targetUrl && connection?.token === authToken.trim()) {
			return;
		}

		getHealth({ baseUrl: targetUrl, token: authToken.trim() })
			.then(() => {
				saveConnection({ baseUrl: targetUrl, token: authToken.trim() });
			})
			.catch(() => {
				// Fallback: If remote :8080 is unreachable, check desktop local origin
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

			// Automatically open GitHub device verification URI
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
						}, 2500);
					} else if (pollRes.status === "expired" || pollRes.status === "denied") {
						clearInterval(timer);
						setDevicePolling(false);
						setAuthError(`GitHub 设备授权失败: ${pollRes.status}`);
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

	// Profile selection change
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

	// Save current profile
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

	// Delete profile
	const handleDeleteProfile = async (id: string) => {
		const updated = profiles.filter((p) => p.id !== id);
		setProfiles(updated);
		if (selectedProfileId === id) {
			setSelectedProfileId(updated[0]?.id || "");
		}
		await saveClientProfiles(effectiveConnection, updated).catch(() => {});
	};

	// Connect / Start client
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
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setActionLoading(false);
		}
	};

	// Disconnect / Stop client
	const handleDisconnect = async () => {
		setActionLoading(true);
		setError(null);
		try {
			await stopClient(effectiveConnection);
			fetchStatus();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setActionLoading(false);
		}
	};

	// Import invite link
	const handleImportLink = () => {
		setImportError(null);
		const parsed = parsePrismLink(importUrl);
		if (!parsed || !parsed.server_addr) {
			setImportError("Invalid Prism link or server address format.");
			return;
		}

		if (parsed.name) {
			setProfileName(parsed.name);
		}
		setServerAddr(parsed.server_addr);
		if (parsed.transport) {
			setTransport(parsed.transport);
		}
		if (parsed.auth_token !== undefined) {
			setAuthToken(parsed.auth_token);
		}
		if (parsed.listen_addr) {
			setListenAddr(parsed.listen_addr);
		}
		if (parsed.fake_lan_broadcast !== undefined) {
			setFakeLanBroadcast(parsed.fake_lan_broadcast);
		}

		setImportModalOpen(false);
		setImportUrl("");
	};

	// Copy invite link
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

	return (
		<div className="flex flex-col gap-6">
			<PageHeader
				eyebrow="Terminal Client"
				title="Prism Client Dashboard"
				description="Transparent network tunnel sidecar with L7 traffic optimization and Minecraft Fake LAN auto-discovery."
				actions={
					<div className="flex items-center gap-3">
						<button
							type="button"
							onClick={() => setImportModalOpen(true)}
							className="flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-2.5 text-sm font-medium text-slate-200 transition hover:border-cyan-400/40 hover:bg-cyan-400/10 hover:text-white"
						>
							<Download className="h-4 w-4 text-cyan-400" />
							Import Invite Link
						</button>
						<button
							type="button"
							onClick={handleShareLink}
							className="flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-2.5 text-sm font-medium text-slate-200 transition hover:border-cyan-400/40 hover:bg-cyan-400/10 hover:text-white"
						>
							{copied === "share" ? (
								<Check className="h-4 w-4 text-emerald-400" />
							) : (
								<Share2 className="h-4 w-4 text-cyan-400" />
							)}
							{copied === "share" ? "Copied!" : "Share Link"}
						</button>
					</div>
				}
			/>

			{error ? <ErrorBanner message={error} onRetry={fetchStatus} /> : null}

			{/* Top Hero Status Banner */}
			<div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
				{/* Connection Status Card */}
				<div className="relative overflow-hidden rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 backdrop-blur">
					<div className="flex items-center justify-between">
						<span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
							Tunnel State
						</span>
						{isConnected ? (
							<span className="inline-flex items-center gap-1.5 rounded-full bg-emerald-500/15 px-3 py-1 text-xs font-medium text-emerald-400 ring-1 ring-emerald-500/30">
								<span className="h-2 w-2 animate-pulse rounded-full bg-emerald-400" />
								Connected
							</span>
						) : isConnecting ? (
							<span className="inline-flex items-center gap-1.5 rounded-full bg-amber-500/15 px-3 py-1 text-xs font-medium text-amber-400 ring-1 ring-amber-500/30">
								<span className="h-2 w-2 animate-ping rounded-full bg-amber-400" />
								Reconnecting...
							</span>
						) : (
							<span className="inline-flex items-center gap-1.5 rounded-full bg-slate-500/15 px-3 py-1 text-xs font-medium text-slate-400 ring-1 ring-slate-500/30">
								<span className="h-2 w-2 rounded-full bg-slate-400" />
								Disconnected
							</span>
						)}
					</div>

					<div className="mt-4 flex items-baseline gap-2">
						<span className="text-2xl font-bold tracking-tight text-white">
							{status?.server_addr || serverAddr || "No Server Configured"}
						</span>
					</div>
					<div className="mt-2 flex items-center gap-2 text-xs text-slate-400">
						<Badge tone="cyan">{status?.transport || transport}</Badge>
						<span>Local Ingress: {status?.listen_addr || listenAddr}</span>
					</div>

					{connection ? (
						<div className="mt-3 flex items-center justify-between rounded-xl border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-xs">
							<div className="flex items-center gap-2 text-emerald-300">
								<span className="h-2 w-2 rounded-full bg-emerald-400" />
								<span>Admin Panel Sync: {connection.baseUrl}</span>
							</div>
							<Link
								to="/"
								className="flex items-center gap-1 font-semibold text-emerald-400 hover:text-emerald-300"
							>
								Dashboard <ArrowRight className="h-3.5 w-3.5" />
							</Link>
						</div>
					) : null}

					<div className="mt-6 flex gap-3">
						{isRunning ? (
							<button
								type="button"
								disabled={actionLoading}
								onClick={handleDisconnect}
								className="flex flex-1 items-center justify-center gap-2 rounded-2xl border border-red-500/30 bg-red-500/15 py-3 text-sm font-semibold text-red-300 transition hover:bg-red-500/25 disabled:opacity-50"
							>
								<Square className="h-4 w-4 fill-current" />
								Disconnect Tunnel
							</button>
						) : (
							<button
								type="button"
								disabled={actionLoading || !serverAddr}
								onClick={handleConnect}
								className="flex flex-1 items-center justify-center gap-2 rounded-2xl bg-gradient-to-r from-cyan-500 to-blue-600 py-3 text-sm font-semibold text-white shadow-lg shadow-cyan-500/20 transition hover:from-cyan-400 hover:to-blue-500 disabled:opacity-50"
							>
								<Play className="h-4 w-4 fill-current" />
								Connect Now
							</button>
						)}
					</div>
				</div>

				{/* Traffic Optimizer Metrics */}
				<div className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 backdrop-blur lg:col-span-2">
					<div className="flex items-center justify-between">
						<div className="flex items-center gap-2">
							<Zap className="h-4 w-4 text-cyan-400" />
							<span className="text-xs font-semibold uppercase tracking-wider text-slate-300">
								Traffic Optimizer Performance
							</span>
						</div>
						<span className="text-xs text-slate-400">
							{savedRatio > 0 ? `${savedRatio.toFixed(1)}% bandwidth saved` : "Ready"}
						</span>
					</div>

					{/* Progress Bar */}
					<div className="mt-4">
						<div className="flex justify-between text-xs text-slate-400">
							<span>Compression Ratio</span>
							<span className="font-semibold text-cyan-300">{savedRatio.toFixed(1)}%</span>
						</div>
						<div className="mt-2 h-3 w-full overflow-hidden rounded-full bg-slate-800">
							<div
								className="h-full rounded-full bg-gradient-to-r from-cyan-400 to-emerald-400 transition-all duration-500"
								style={{ width: `${Math.min(100, Math.max(0, savedRatio))}%` }}
							/>
						</div>
					</div>

					{/* Stats Grid */}
					<div className="mt-6 grid grid-cols-3 gap-4 border-t border-white/8 pt-4">
						<div>
							<div className="text-xs text-slate-400">Game Traffic (Raw)</div>
							<div className="mt-1 text-lg font-semibold text-white">{formatBytes(rawBytes)}</div>
						</div>
						<div>
							<div className="text-xs text-slate-400">Network Wire Sent</div>
							<div className="mt-1 text-lg font-semibold text-slate-300">
								{formatBytes(wireBytes)}
							</div>
						</div>
						<div>
							<div className="text-xs text-slate-400">Bandwidth Saved</div>
							<div className="mt-1 text-lg font-semibold text-emerald-400">
								{formatBytes(savedBytes)}
							</div>
						</div>
					</div>
				</div>
			</div>

			{/* Main Configuration & Discovered Services Layout */}
			<div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
				{/* Left 1 Col: Server Configuration & Profiles */}
				<div className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 backdrop-blur">
					<div className="flex items-center justify-between">
						<h2 className="text-base font-semibold text-white">Server Settings</h2>
						{profiles.length > 0 ? (
							<label className="flex items-center gap-2">
								<span className="sr-only">Select Server Profile</span>
								<select
									aria-label="Select Server Profile"
									value={selectedProfileId}
									onChange={(e) => handleSelectProfile(e.target.value)}
									className="rounded-xl border border-white/10 bg-slate-900 px-3 py-1.5 text-xs text-slate-300 outline-none transition focus:border-cyan-400"
								>
									{profiles.map((p) => (
										<option key={p.id} value={p.id}>
											{p.name}
										</option>
									))}
								</select>
							</label>
						) : null}
					</div>

					<div className="mt-5 flex flex-col gap-4">
						<label className="block space-y-1.5">
							<span className="text-xs font-medium text-slate-400">Profile Name</span>
							<input
								type="text"
								value={profileName}
								onChange={(e) => setProfileName(e.target.value)}
								placeholder="e.g. My Survival Realm"
								className="w-full rounded-xl border border-white/10 bg-white/4 px-3.5 py-2 text-sm text-white placeholder-slate-500 outline-none transition focus:border-cyan-400"
							/>
						</label>

						<label className="block space-y-1.5">
							<span className="text-xs font-medium text-slate-400">Relay Server Address</span>
							<input
								type="text"
								value={serverAddr}
								onChange={(e) => setServerAddr(e.target.value)}
								placeholder="relay.example.com:7000"
								className="w-full rounded-xl border border-white/10 bg-white/4 px-3.5 py-2 text-sm text-white placeholder-slate-500 outline-none transition focus:border-cyan-400"
							/>
						</label>

						<div className="grid grid-cols-2 gap-3">
							<label className="block space-y-1.5">
								<span className="text-xs font-medium text-slate-400">Transport</span>
								<select
									value={transport}
									onChange={(e) => setTransport(e.target.value)}
									className="w-full rounded-xl border border-white/10 bg-slate-900 px-3 py-2 text-sm text-white outline-none transition focus:border-cyan-400"
								>
									<option value="quic">QUIC (Fast & Resilient)</option>
									<option value="kcp">KCP (Low Latency UDP)</option>
									<option value="tcp">TCP (Standard)</option>
								</select>
							</label>

							<label className="block space-y-1.5">
								<span className="text-xs font-medium text-slate-400">Local Port</span>
								<input
									type="text"
									value={listenAddr}
									onChange={(e) => setListenAddr(e.target.value)}
									placeholder="127.0.0.1:25565"
									className="w-full rounded-xl border border-white/10 bg-white/4 px-3.5 py-2 text-sm text-white placeholder-slate-500 outline-none transition focus:border-cyan-400"
								/>
							</label>
						</div>

						<div className="space-y-1.5">
							<div className="flex items-center justify-between">
								<span className="text-xs font-medium text-slate-400">Auth Token</span>
								<button
									type="button"
									onClick={() => {
										setAuthServerUrl(managementUrl || "http://127.0.0.1:8080");
										setGithubAuthOpen(true);
									}}
									className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/5 px-2 py-0.5 text-xs font-medium text-cyan-300 transition hover:border-cyan-400/40 hover:bg-cyan-400/10 hover:text-white"
								>
									<Github className="h-3 w-3 text-cyan-400" />
									<span>GitHub 登录获取</span>
								</button>
							</div>
							<div className="relative">
								<input
									type={showToken ? "text" : "password"}
									value={authToken}
									onChange={(e) => setAuthToken(e.target.value)}
									placeholder="Server authentication token"
									className="w-full rounded-xl border border-white/10 bg-white/4 px-3.5 py-2 pr-10 text-sm text-white placeholder-slate-500 outline-none transition focus:border-cyan-400"
								/>
								<button
									type="button"
									onClick={() => setShowToken(!showToken)}
									className="absolute top-1/2 right-3 -translate-y-1/2 text-slate-400 transition hover:text-white"
								>
									{showToken ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
								</button>
							</div>
						</div>

						{/* Auto connect panel session toggle */}
						<label
							aria-label="Auto-connect Control Panel"
							className="mt-1 flex cursor-pointer items-start gap-3 rounded-xl border border-white/6 bg-white/2 p-3 transition hover:bg-white/4"
						>
							<input
								type="checkbox"
								checked={autoConnectPanel}
								onChange={(e) => setAutoConnectPanel(e.target.checked)}
								className="mt-1 h-4 w-4 rounded border-slate-700 bg-slate-900 text-cyan-500 focus:ring-cyan-400"
							/>
							<div className="flex flex-col">
								<span className="text-sm font-medium text-slate-200">
									Auto-connect Control Panel
								</span>
								<span className="text-xs text-slate-400">
									Automatically authenticate and attach panel session ({managementUrl}) when tunnel
									connects.
								</span>
							</div>
						</label>

						{/* Fake LAN toggle */}
						<label
							aria-label="Minecraft Fake LAN Discovery"
							className="mt-1 flex cursor-pointer items-start gap-3 rounded-xl border border-white/6 bg-white/2 p-3 transition hover:bg-white/4"
						>
							<input
								type="checkbox"
								checked={fakeLanBroadcast}
								onChange={(e) => setFakeLanBroadcast(e.target.checked)}
								className="mt-1 h-4 w-4 rounded border-slate-700 bg-slate-900 text-cyan-500 focus:ring-cyan-400"
							/>
							<div className="flex flex-col">
								<span className="text-sm font-medium text-slate-200">
									Minecraft Fake LAN Discovery
								</span>
								<span className="text-xs text-slate-400">
									Broadcasts game sessions locally so friends can connect from the "LAN Games" list
									without typing IP.
								</span>
							</div>
						</label>

						<div className="flex gap-2 pt-2">
							<button
								type="button"
								onClick={handleSaveProfile}
								className="flex flex-1 items-center justify-center gap-2 rounded-xl border border-cyan-400/30 bg-cyan-400/10 py-2.5 text-xs font-semibold text-cyan-300 transition hover:bg-cyan-400/20"
							>
								<Plus className="h-3.5 w-3.5" />
								Save Profile
							</button>
							{selectedProfileId ? (
								<button
									type="button"
									onClick={() => handleDeleteProfile(selectedProfileId)}
									className="flex items-center justify-center rounded-xl border border-red-400/20 bg-red-400/10 px-3 text-red-400 transition hover:bg-red-400/20"
								>
									<Trash2 className="h-4 w-4" />
								</button>
							) : null}
						</div>
					</div>
				</div>

				{/* Right 2 Cols: Discovered Services & Game Status */}
				<div className="flex flex-col gap-6 lg:col-span-2">
					{/* Fake LAN Active Card */}
					{fakeLanBroadcast && isConnected ? (
						<div className="relative overflow-hidden rounded-[2rem] border border-cyan-400/20 bg-gradient-to-r from-cyan-950/40 to-slate-950/60 p-6">
							<div className="flex items-start gap-4">
								<div className="flex h-12 w-12 flex-none items-center justify-center rounded-2xl bg-cyan-500/15 text-cyan-400 ring-1 ring-cyan-500/30">
									<Gamepad2 className="h-6 w-6" />
								</div>
								<div>
									<h3 className="text-base font-semibold text-white">
										Minecraft LAN Auto-Discovery Active
									</h3>
									<p className="mt-1 text-sm text-slate-300">
										Simply launch Minecraft on this machine, go to <b>Multiplayer</b>, and your
										server will appear automatically in the <b>LAN Games</b> list!
									</p>
									<div className="mt-3 flex items-center gap-3">
										<button
											type="button"
											onClick={() => copyText(status?.listen_addr || listenAddr, "lan-addr")}
											className="flex items-center gap-1.5 rounded-xl border border-white/10 bg-white/5 px-3 py-1.5 text-xs font-medium text-slate-300 transition hover:bg-white/10"
										>
											{copied === "lan-addr" ? (
												<Check className="h-3.5 w-3.5 text-emerald-400" />
											) : (
												<Copy className="h-3.5 w-3.5 text-slate-400" />
											)}
											Manual IP: {status?.listen_addr || listenAddr}
										</button>
									</div>
								</div>
							</div>
						</div>
					) : null}

					{/* Discovered Services Catalog */}
					<div className="flex-1 rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 backdrop-blur">
						<div className="flex items-center justify-between">
							<div className="flex items-center gap-2">
								<Layers className="h-4 w-4 text-cyan-400" />
								<h2 className="text-base font-semibold text-white">Discovered Remote Services</h2>
							</div>
							<span className="text-xs text-slate-400">
								{status?.known_services.length || 0} active services
							</span>
						</div>

						<div className="mt-4 flex flex-col gap-3">
							{status?.known_services && status.known_services.length > 0 ? (
								status.known_services.map((svc) => (
									<div
										key={svc.name}
										className="flex flex-col justify-between gap-3 rounded-2xl border border-white/8 bg-white/3 p-4 transition hover:border-cyan-400/30 md:flex-row md:items-center"
									>
										<div className="flex items-center gap-3">
											<div className="flex h-10 w-10 items-center justify-center rounded-xl bg-cyan-400/10 text-cyan-400">
												<Radio className="h-5 w-5" />
											</div>
											<div>
												<div className="flex items-center gap-2">
													<span className="font-semibold text-white">{svc.name}</span>
													<Badge tone="cyan">{svc.proto.toUpperCase()}</Badge>
												</div>
												<div className="mt-0.5 text-xs text-slate-400">
													Remote Route: {svc.masquerade_host || "Direct Tunnel"}
												</div>
											</div>
										</div>

										<div className="flex items-center gap-2">
											<button
												type="button"
												onClick={() => copyText(status?.listen_addr || listenAddr, svc.name)}
												className="flex items-center gap-1.5 rounded-xl border border-white/10 bg-white/5 px-3 py-2 text-xs font-medium text-slate-300 transition hover:bg-cyan-400/10 hover:text-white"
											>
												{copied === svc.name ? (
													<Check className="h-3.5 w-3.5 text-emerald-400" />
												) : (
													<Copy className="h-3.5 w-3.5 text-slate-400" />
												)}
												Copy Join Address
											</button>
										</div>
									</div>
								))
							) : (
								<EmptyState
									icon={<Radio className="h-8 w-8" />}
									label={
										isConnected
											? "Connected to relay server. Waiting for Connector to publish game services..."
											: "Connect to a Prism server to synchronize available services."
									}
								/>
							)}
						</div>
					</div>
				</div>
			</div>

			{/* Import Modal */}
			{importModalOpen ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 p-4 backdrop-blur-sm">
					<div className="w-full max-w-lg rounded-[2rem] border border-white/10 bg-slate-900 p-6 shadow-2xl">
						<h3 className="text-lg font-semibold text-white">Import Prism Invite Link</h3>
						<p className="mt-1 text-sm text-slate-400">
							Paste a <code className="text-cyan-300">prism://</code> link or server address
							provided by your server administrator:
						</p>

						<div className="mt-4">
							<input
								type="text"
								value={importUrl}
								onChange={(e) => setImportUrl(e.target.value)}
								placeholder="prism://play.example.com:7000?token=..."
								className="w-full rounded-xl border border-white/10 bg-white/5 p-3 text-sm text-white placeholder-slate-500 outline-none transition focus:border-cyan-400"
							/>
							{importError ? <p className="mt-2 text-xs text-red-400">{importError}</p> : null}
						</div>

						<div className="mt-6 flex justify-end gap-3">
							<button
								type="button"
								onClick={() => {
									setImportModalOpen(false);
									setImportError(null);
								}}
								className="rounded-xl border border-white/10 px-4 py-2 text-sm text-slate-300 transition hover:bg-white/5"
							>
								Cancel
							</button>
							<button
								type="button"
								onClick={handleImportLink}
								className="rounded-xl bg-cyan-500 px-4 py-2 text-sm font-semibold text-white transition hover:bg-cyan-400"
							>
								Import & Apply
							</button>
						</div>
					</div>
				</div>
			) : null}

			{/* GitHub Auth Modal */}
			{githubAuthOpen ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 p-4 backdrop-blur-sm">
					<div className="w-full max-w-lg rounded-[2rem] border border-white/10 bg-slate-900 p-6 shadow-2xl">
						<div className="flex items-center justify-between">
							<div className="flex items-center gap-2">
								<Github className="h-5 w-5 text-white" />
								<h3 className="text-lg font-semibold text-white">GitHub 授权登录</h3>
							</div>
							<button
								type="button"
								onClick={() => {
									setGithubAuthOpen(false);
									setDeviceCode(null);
									setDevicePolling(false);
									setAuthError(null);
								}}
								className="text-slate-400 hover:text-white"
							>
								<X className="h-5 w-5" />
							</button>
						</div>

						<p className="mt-2 text-sm text-slate-400">
							通过 GitHub 账号授权获取 Relay Server 的访问令牌，并同步连接管理控制面板。
						</p>

						<div className="mt-4 flex flex-col gap-3">
							<label className="block space-y-1">
								<span className="text-xs font-medium text-slate-400">
									认证服务地址 (Auth Server URL)
								</span>
								<input
									type="text"
									value={authServerUrl}
									onChange={(e) => setAuthServerUrl(e.target.value)}
									placeholder="http://127.0.0.1:8080"
									disabled={devicePolling}
									className="w-full rounded-xl border border-white/10 bg-white/5 px-3.5 py-2 text-sm text-white placeholder-slate-500 outline-none transition focus:border-cyan-400 disabled:opacity-50"
								/>
							</label>

							{authError ? (
								<div className="rounded-xl border border-red-500/20 bg-red-500/10 p-3 text-xs text-red-400">
									{authError}
								</div>
							) : null}

							{deviceSuccess ? (
								<div className="flex items-center gap-2 rounded-xl border border-emerald-500/30 bg-emerald-500/15 p-4 text-emerald-300">
									<CheckCircle2 className="h-5 w-5 flex-none" />
									<div>
										<p className="text-sm font-semibold">GitHub 授权成功！</p>
										<p className="text-xs text-emerald-400/80">
											Auth Token 已自动填入表单并同步连接管理面板。
										</p>
									</div>
								</div>
							) : deviceCode ? (
								<div className="rounded-2xl border border-white/10 bg-slate-950/60 p-4">
									<div className="text-xs text-slate-400">
										请在 GitHub 页面输入以下 8 位设备码：
									</div>
									<div className="mt-2 flex items-center justify-between rounded-xl border border-white/10 bg-white/5 px-4 py-3">
										<span className="font-mono text-xl font-bold tracking-wider text-cyan-300">
											{deviceCode.user_code}
										</span>
										<button
											type="button"
											onClick={() => copyText(deviceCode.user_code, "user-code")}
											className="flex items-center gap-1.5 text-xs text-slate-300 hover:text-white"
										>
											{copied === "user-code" ? (
												<Check className="h-4 w-4 text-emerald-400" />
											) : (
												<Copy className="h-4 w-4" />
											)}
											{copied === "user-code" ? "已复制" : "复制"}
										</button>
									</div>

									<div className="mt-4 flex items-center justify-between">
										<a
											href={deviceCode.verification_uri}
											target="_blank"
											rel="noreferrer"
											className="inline-flex items-center gap-1.5 text-xs font-medium text-cyan-400 hover:underline"
										>
											<span>打开 GitHub 验证页面</span>
											<ExternalLink className="h-3.5 w-3.5" />
										</a>
										{devicePolling ? (
											<span className="flex items-center gap-2 text-xs text-slate-400">
												<span className="h-2 w-2 animate-ping rounded-full bg-cyan-400" />
												等待用户授权中...
											</span>
										) : null}
									</div>
								</div>
							) : (
								<div className="mt-2 flex flex-col gap-3">
									<button
										type="button"
										onClick={startDeviceAuth}
										disabled={deviceLoading}
										className="flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-slate-800 to-slate-700 py-3 text-sm font-semibold text-white transition hover:from-slate-700 hover:to-slate-600 disabled:opacity-50"
									>
										<Github className="h-4 w-4" />
										{deviceLoading ? "正在请求设备码..." : "发起 GitHub 设备码授权登录"}
									</button>
									<div className="flex items-center justify-between text-xs text-slate-500">
										<span>浏览器标准 OAuth 登录流程：</span>
										<a
											href={`${normalizeBaseUrl(authServerUrl)}/auth/github/login`}
											className="flex items-center gap-1 text-cyan-400 hover:underline"
										>
											网页跳转登录 <ExternalLink className="h-3 w-3" />
										</a>
									</div>
								</div>
							)}
						</div>

						<div className="mt-6 flex justify-end">
							<button
								type="button"
								onClick={() => {
									setGithubAuthOpen(false);
									setDeviceCode(null);
									setDevicePolling(false);
									setAuthError(null);
								}}
								className="rounded-xl border border-white/10 px-4 py-2 text-sm text-slate-300 transition hover:bg-white/5"
							>
								关闭
							</button>
						</div>
					</div>
				</div>
			) : null}
		</div>
	);
}
