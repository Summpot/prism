import { Link } from "@tanstack/react-router";
import {
	Activity,
	Check,
	Copy,
	Gamepad2,
	Plug,
	Power,
	Radio,
	RotateCcw,
	WifiOff,
	X,
} from "lucide-react";

import { Github } from "@/components/icons/Github";

import { useClient } from "@/context/ClientContext";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { formatBytes } from "@/lib/format";
import { usePanelSession } from "@/lib/panelSession";
import { SUPPORTED_LINK_PROTOCOLS } from "@/lib/prismLink";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";
import { useState } from "react";

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

export function ClientOverview() {
	const { authSession, isAdmin, clearConnection } = usePanelSession();
	const [copiedLink, setCopiedLink] = useState(false);

	const {
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
		copied,
		copyText,
		handleConnect,
		handleDisconnect,
		handleResetStats,
		profileName,
		serverAddr,
		transport,
		setAuthToken,
		listenAddr,
		fakeLanBroadcast,
		remoteLinkInput,
		setRemoteLinkInput,
		linkProtocol,
		handleSelectProtocol,
		handleAddressChange,
		handleAddressPaste,
		handleAddressCopy,
		handleConnectFromLink,
		checkingProviders,
		providersResult,
		setProvidersResult,
		authServerUrl,
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
		loginAdminUnlocked,
	} = useClient();

	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-y-auto">
			{error ? (
				<div className="flex flex-none items-center justify-between gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
					<span className="truncate">{error}</span>
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
									{authSession.display_name || authSession.username || m.session_logged_in()}
								</span>
								{isAdmin ? (
									<Badge className="bg-primary/20 text-primary border-primary/30 text-[9px] px-1.5 py-0 h-4">
										{m.client_admin()}
									</Badge>
								) : (
									<Badge variant="secondary" className="text-[9px] px-1.5 py-0 h-4">
										{m.client_member()}
									</Badge>
								)}
							</div>
							<p className="text-[10px] font-mono text-muted-foreground truncate">
								{m.client_default_node()}: {serverAddr || m.client_default_node()}
							</p>
						</div>
					</div>

					<div className="flex items-center gap-2 flex-none">
						{isAdmin || loginAdminUnlocked ? (
							<Link to="/admin" className="text-[11px] font-bold text-primary hover:underline">
								{m.client_admin_console()}
							</Link>
						) : null}
						<Button
							variant="outline"
							size="xs"
							onClick={() => {
								clearConnection();
								setAuthToken("");
								setProvidersResult(null);
								setAuthError(null);
								if (status?.running) {
									void handleDisconnect();
								}
							}}
							className="h-7 text-xs px-2.5 text-muted-foreground hover:text-destructive cursor-pointer"
						>
							<span>{m.client_sign_out()}</span>
						</Button>
					</div>
				</div>
			) : (
				<div className="flex-none rounded-lg border border-border bg-card p-3 shadow-xs space-y-2.5">
					<div className="flex items-center justify-between">
						<div className="flex items-center gap-1.5">
							<Radio className="h-4 w-4 text-primary" />
							<span className="text-xs font-bold text-foreground">{m.client_connect_and_login()}</span>
						</div>
						<span className="text-[11px] text-muted-foreground">
							{m.client_anonymous_hint()}
						</span>
					</div>

					<div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
						<div className="relative flex-1 min-w-0 flex items-stretch">
							<Select
								value={linkProtocol}
								onValueChange={(val) => {
									if (val) handleSelectProtocol(val);
								}}
								items={SUPPORTED_LINK_PROTOCOLS}
							>
								<SelectTrigger
									aria-label={m.client_select_protocol()}
									className="h-8 w-fit min-w-22.5 rounded-l-md rounded-r-none border-r-0 border-input bg-muted/60 px-2.5 text-xs font-mono font-semibold text-foreground shadow-none focus-visible:ring-0 focus-visible:border-input shrink-0 cursor-pointer hover:bg-muted"
								>
									<SelectValue placeholder={m.client_protocol()} />
								</SelectTrigger>
								<SelectContent align="start" className="min-w-36 text-xs font-mono">
									{SUPPORTED_LINK_PROTOCOLS.map((p) => (
										<SelectItem key={p.value} value={p.value} className="text-xs font-mono">
											{p.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
							<div className="relative flex-1 min-w-0">
								<Input
									value={remoteLinkInput}
									onChange={(e) => handleAddressChange(e.target.value)}
									onPaste={handleAddressPaste}
									onCopy={handleAddressCopy}
									placeholder={m.client_remote_placeholder()}
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
											title={m.client_copy_connection()}
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
											title={m.client_clear()}
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
									<span>{m.client_connecting()}</span>
								</>
							) : (
								<>
									<Plug className="h-3.5 w-3.5" />
									<span>{m.client_connect()}</span>
								</>
							)}
						</Button>
					</div>

					{/* 探测到 Provider 后的滑出式选择菜单 (Slide-out Provider Selection & Inline Auth) */}
					{oauthExchanging ? (
						<div className="border-t border-border/60 pt-2.5 flex items-center justify-center py-3 gap-2.5 text-muted-foreground animate-in fade-in-0 slide-in-from-top-2 duration-300">
							<RotateCcw className="h-4 w-4 animate-spin text-primary" />
							<span className="text-xs font-medium text-foreground">
								{m.client_exchange_github()}
							</span>
						</div>
					) : oauthWaitingCallback ? (
						<div className="border-t border-border/60 pt-2.5 space-y-2.5 animate-in fade-in-0 slide-in-from-top-2 duration-300">
							<div className="rounded-lg border border-primary/30 bg-primary/5 p-3 space-y-2.5">
								<div className="flex items-center justify-between">
									<div className="flex items-center gap-2">
										<RotateCcw className="h-4 w-4 animate-spin text-primary" />
										<span className="text-xs font-bold text-foreground">{m.client_wait_github()}</span>
									</div>
									<Button
										variant="ghost"
										size="xs"
										onClick={() => {
											setOauthWaitingCallback(false);
											setAuthError(null);
										}}
										className="h-6 text-[11px] text-muted-foreground hover:text-foreground cursor-pointer"
									>
										{m.client_back()}
									</Button>
								</div>
								<p className="text-[11px] text-muted-foreground leading-relaxed">
									{m.client_github_browser_hint()}
								</p>
								<div className="flex items-center gap-2">
									<Button
										variant="outline"
										size="xs"
										onClick={() => void startGitHubAuthWithUrl(authServerUrl, serverAddr)}
										disabled={oauthLoading}
										className="h-7 text-xs gap-1.5 cursor-pointer"
									>
										<RotateCcw className="h-3 w-3" />
										{m.client_reopen_authorization()}
									</Button>
								</div>
								{authError ? (
									<div className="rounded border border-destructive/30 bg-destructive/10 p-2 text-xs text-destructive">
										{authError}
									</div>
								) : null}
								<div className="flex items-center gap-2 pt-0.5">
									<Input
										value={manualCallbackInput}
										onChange={(e) => setManualCallbackInput(e.target.value)}
										onKeyDown={(e) => {
											if (e.key === "Enter" && manualCallbackInput.trim() && !oauthLoading) {
												void handleManualOAuthCallback(manualCallbackInput.trim());
											}
										}}
										placeholder={m.client_callback_placeholder()}
										className="h-7 text-xs font-mono"
									/>
									<Button
										size="xs"
										className="h-7 text-xs flex-none px-3 cursor-pointer"
										disabled={!manualCallbackInput.trim() || oauthLoading}
										onClick={() => void handleManualOAuthCallback(manualCallbackInput.trim())}
									>
										{m.client_verify()}
									</Button>
								</div>
							</div>
						</div>
					) : providersResult &&
					  (providersResult.github_enabled !== false ||
							(providersResult.providers && providersResult.providers.length > 0)) ? (
						<div className="border-t border-border/60 pt-2.5 space-y-2 animate-in fade-in-0 slide-in-from-top-2 duration-300">
							<div className="flex items-center justify-between">
								<div className="flex items-center gap-1.5">
									<span className="text-xs font-semibold text-foreground">
																{m.client_login_methods_detected()}
									</span>
									<Badge
										variant="outline"
										className="text-[9px] px-1.5 py-0 h-4 border-primary/30 text-primary"
									>
																	{m.client_optional_login()}
									</Badge>
								</div>
								<Button
									variant="ghost"
									size="icon-xs"
									onClick={() => {
										setProvidersResult(null);
										setAuthError(null);
									}}
									className="h-5 w-5 text-muted-foreground hover:text-foreground cursor-pointer"
									title={m.client_collapse()}
								>
									<X className="h-3.5 w-3.5" />
								</Button>
							</div>

							{authError ? (
								<div className="rounded border border-destructive/30 bg-destructive/10 p-2 text-xs text-destructive">
									{authError}
								</div>
							) : null}

							<div className="grid grid-cols-1 sm:grid-cols-2 gap-2 pt-0.5">
								{providersResult.github_enabled !== false ? (
									<button
										type="button"
										disabled={oauthLoading}
										onClick={() => void startGitHubAuthWithUrl(authServerUrl, serverAddr)}
										className="flex items-center gap-2.5 rounded-lg border border-border bg-card hover:border-primary/50 hover:bg-accent/40 p-2.5 text-left transition cursor-pointer group disabled:opacity-50"
									>
										<div className="flex h-8 w-8 items-center justify-center rounded-md bg-foreground text-background shrink-0">
											<Github className="h-4 w-4" />
										</div>
										<div className="min-w-0 flex-1">
											<div className="flex items-center gap-1.5">
												<span className="text-xs font-semibold text-foreground group-hover:text-primary transition-colors">
															{m.client_github_login()}
												</span>
												<Badge variant="secondary" className="text-[9px] px-1 py-0 h-3.5">
															{m.client_recommended()}
												</Badge>
											</div>
											<p className="text-[10px] text-muted-foreground truncate">
														{m.client_identity_hint()}
											</p>
										</div>
									</button>
								) : null}

								<button
									type="button"
									onClick={() => {
										setProvidersResult(null);
										setAuthError(null);
									}}
									className="flex items-center gap-2.5 rounded-lg border border-border bg-card hover:border-primary/50 hover:bg-accent/40 p-2.5 text-left transition cursor-pointer group"
								>
									<div className="flex h-8 w-8 items-center justify-center rounded-md bg-muted text-foreground shrink-0">
										<Plug className="h-4 w-4" />
									</div>
									<div className="min-w-0 flex-1">
										<div className="flex items-center gap-1.5">
											<span className="text-xs font-semibold text-foreground group-hover:text-primary transition-colors">
															{m.client_anonymous()}
											</span>
										</div>
										<p className="text-[10px] text-muted-foreground truncate">
															{m.client_guest_hint()}
										</p>
									</div>
								</button>
							</div>
						</div>
					) : null}
				</div>
			)}

			{/* 隧道连接与实时状态卡片 (Active Tunnel Status) - 架构上保留支持未来多服务器/多隧道并行连接的扩展能力 */}
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
									{isConnected
										? status?.actual_transport ||
											(status?.transport && status.transport !== "auto"
												? status.transport
												: null) ||
											transport
										: status?.transport || transport}
								</Badge>
							</div>
							<div className="flex items-center gap-1 text-[11px] font-mono text-muted-foreground truncate">
								<span className="truncate">{serverAddr || status?.server_addr || "No Server"}</span>
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
							<span>{m.client_disconnect_tunnel()}</span>
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
							<span>{m.client_start_connection()}</span>
						</Button>
					) : (
						<span className="text-[11px] font-medium text-amber-500 bg-amber-500/10 border border-amber-500/20 px-2 py-0.5 rounded">
							{m.client_waiting_for_login()}
						</span>
					)}
				</div>

				{/* Connected Metrics Strip */}
				{isConnected ? (
					<div className="border-t border-border/60 pt-2 space-y-2">
						{/* Mode Switcher */}
						<div className="flex items-center justify-between gap-1 text-[10px] text-muted-foreground">
							<span className="font-semibold uppercase tracking-wider text-[9px]">
								{statsViewMode === "session" ? m.client_current_session() : m.client_cumulative_lifetime()}
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
									{m.client_session()}
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
									{m.client_lifetime()}
								</button>
							</div>
						</div>

						{/* 4-col compact stats */}
						<div className="grid grid-cols-4 gap-1.5 text-center font-mono">
							<div className="rounded bg-muted/40 px-1.5 py-1">
								<div className="text-[9px] uppercase text-muted-foreground">
									{statsViewMode === "session" ? m.client_uptime() : m.client_sessions()}
								</div>
								<div className="text-xs font-bold text-foreground truncate">
									{statsViewMode === "session"
										? formatUptime(uptimeSeconds)
										: (cumulativeStats?.sessions_count ?? 0) + 1}
								</div>
							</div>
							<div className="rounded bg-muted/40 px-1.5 py-1">
								<div className="text-[9px] uppercase text-muted-foreground">{m.client_raw()}</div>
								<div className="text-xs font-bold text-foreground truncate">
									{formatBytes(
										statsViewMode === "session"
											? rawBytes
											: (cumulativeStats?.raw_bytes ?? 0) + rawBytes,
									)}
								</div>
							</div>
							<div className="rounded bg-muted/40 px-1.5 py-1">
								<div className="text-[9px] uppercase text-muted-foreground">{m.client_wire()}</div>
								<div className="text-xs font-bold text-foreground truncate">
									{formatBytes(
										statsViewMode === "session"
											? wireBytes
											: (cumulativeStats?.wire_bytes ?? 0) + wireBytes,
									)}
								</div>
							</div>
							<div className="rounded bg-muted/40 px-1.5 py-1">
								<div className="text-[9px] uppercase text-emerald-500">{m.client_saved()}</div>
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
										<span className="text-primary font-bold">↑</span> {m.client_up()}
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
										<span className="text-primary font-bold">↓</span> {m.client_down()}
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
									{m.client_latency()}
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
									<span>{m.client_lan_active()}</span>
									<button
										type="button"
										onClick={() => copyText(status?.listen_addr || listenAddr, "lan-btn")}
										className="ml-0.5 hover:opacity-80 cursor-pointer"
										title={m.client_copy_lan_address()}
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
											{m.client_history_stats()}
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
							title={m.client_reset_stats()}
						>
							{m.client_reset()}
						</Button>
					</div>
				) : null}
			</div>

			{/* 已发现远端服务卡片 (Discovered Remote Services) */}
			<div className="flex-1 min-h-0 flex flex-col rounded-lg border border-border bg-card p-3 shadow-xs">
				<div className="flex items-center justify-between pb-2 border-b border-border/50 flex-none">
					<span className="text-xs font-semibold text-foreground">{m.client_discovered_services()}</span>
					<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4">
						{status?.known_services.length || 0} {m.client_active()}
					</Badge>
				</div>

				<div className="flex-1 min-h-0 overflow-y-auto divide-y divide-border/40 pt-1">
					{!authSession?.authenticated && !status?.server_addr ? (
						<div className="flex h-full flex-col items-center justify-center py-8 text-center text-muted-foreground">
							<WifiOff className="mb-2 h-7 w-7 text-muted-foreground/50" />
							<p className="text-xs font-medium text-foreground">{m.client_not_connected_service()}</p>
							<p className="text-[11px] text-muted-foreground max-w-sm mt-1">
								{m.client_service_hint()}
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
													<span className="text-muted-foreground/70">({svc.masquerade_host})</span>
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
										<span>{copied === svc.name ? m.common_copied() : m.client_copy()}</span>
									</Button>
								</div>
							);
						})
					) : (
						<div className="flex h-full flex-col items-center justify-center py-8 text-center text-muted-foreground">
							<WifiOff className="mb-2 h-7 w-7 text-muted-foreground/50" />
							<p className="text-xs">
								{isConnected
									? m.client_waiting_services()
									: m.client_connect_server_services()}
							</p>
						</div>
					)}
				</div>
			</div>
		</div>
	);
}
