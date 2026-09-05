import { createFileRoute } from "@tanstack/react-router";
import {
	Check,
	Copy,
	Download,
	Eye,
	EyeOff,
	Gamepad2,
	Layers,
	Play,
	Plus,
	Radio,
	Share2,
	Square,
	Trash2,
	Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Badge, EmptyState, ErrorBanner, PageHeader } from "@/components/ui";
import { formatBytes } from "@/lib/format";
import {
	type ClientProfile,
	type ClientStatusResponse,
	getClientProfiles,
	getClientStatus,
	saveClientProfiles,
	startClient,
	stopClient,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { encodePrismLink, parsePrismLink } from "@/lib/prismLink";
import { usePolling } from "@/lib/usePolling";

export const Route = createFileRoute("/client")({
	component: ClientDashboardPage,
});

function ClientDashboardPage() {
	const { connection } = usePanelSession();
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

						<label className="block space-y-1.5">
							<span className="text-xs font-medium text-slate-400">Auth Token</span>
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
		</div>
	);
}
