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
import { formatBytes } from "@/lib/format";
import { usePanelSession } from "@/lib/panelSession";
import { SUPPORTED_LINK_PROTOCOLS } from "@/lib/prismLink";
import { cn } from "@/lib/utils";
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
							<Link to="/admin" className="text-[11px] font-bold text-primary hover:underline">
								进入管理控制台 &rarr;
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

					{/* 探测到 Provider 后的滑出式选择菜单 (Slide-out Provider Selection & Inline Auth) */}
					{oauthExchanging ? (
						<div className="border-t border-border/60 pt-2.5 flex items-center justify-center py-3 gap-2.5 text-muted-foreground animate-in fade-in-0 slide-in-from-top-2 duration-300">
							<RotateCcw className="h-4 w-4 animate-spin text-primary" />
							<span className="text-xs font-medium text-foreground">
								正在兑换 GitHub 授权凭证...
							</span>
						</div>
					) : oauthWaitingCallback ? (
						<div className="border-t border-border/60 pt-2.5 space-y-2.5 animate-in fade-in-0 slide-in-from-top-2 duration-300">
							<div className="rounded-lg border border-primary/30 bg-primary/5 p-3 space-y-2.5">
								<div className="flex items-center justify-between">
									<div className="flex items-center gap-2">
										<RotateCcw className="h-4 w-4 animate-spin text-primary" />
										<span className="text-xs font-bold text-foreground">等待 GitHub 授权完成</span>
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
										返回
									</Button>
								</div>
								<p className="text-[11px] text-muted-foreground leading-relaxed">
									已在默认浏览器中打开 GitHub 授权页面。完成授权后，系统将自动唤起客户端完成登录。
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
										重新打开授权页面
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
										placeholder="未自动唤起？手动粘贴回调链接或验证码"
										className="h-7 text-xs font-mono"
									/>
									<Button
										size="xs"
										className="h-7 text-xs flex-none px-3 cursor-pointer"
										disabled={!manualCallbackInput.trim() || oauthLoading}
										onClick={() => void handleConnectFromLink(manualCallbackInput.trim())}
									>
										验证
									</Button>
								</div>
							</div>
						</div>
					) : providersResult && (providersResult.github_enabled !== false || (providersResult.providers && providersResult.providers.length > 0)) ? (
						<div className="border-t border-border/60 pt-2.5 space-y-2 animate-in fade-in-0 slide-in-from-top-2 duration-300">
							<div className="flex items-center justify-between">
								<div className="flex items-center gap-1.5">
									<span className="text-xs font-semibold text-foreground">
										已探测到该节点支持的登录方式
									</span>
									<Badge variant="outline" className="text-[9px] px-1.5 py-0 h-4 border-primary/30 text-primary">
										可选登录
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
									title="收起"
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
													GitHub 授权登录
												</span>
												<Badge variant="secondary" className="text-[9px] px-1 py-0 h-3.5">
													推荐
												</Badge>
											</div>
											<p className="text-[10px] text-muted-foreground truncate">
												绑定账号身份与访问权限
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
												保持匿名连接
											</span>
										</div>
										<p className="text-[10px] text-muted-foreground truncate">
											直接作为访客使用当前隧道
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
										? (status?.actual_transport || (status?.transport && status.transport !== "auto" ? status.transport : null) || transport)
										: (status?.transport || transport)}
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
										title="复制局域网地址"
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
								历史统计
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
							title="重置累计统计"
						>
							重置
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
					{!authSession?.authenticated && !status?.server_addr ? (
						<div className="flex h-full flex-col items-center justify-center py-8 text-center text-muted-foreground">
							<WifiOff className="mb-2 h-7 w-7 text-muted-foreground/50" />
							<p className="text-xs font-medium text-foreground">未连接远端服务</p>
							<p className="text-[11px] text-muted-foreground max-w-sm mt-1">
								在上方输入远端节点连接地址并连接，即可查看此节点发布的代理服务与虚拟局域网端口映射。
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
	);
}
