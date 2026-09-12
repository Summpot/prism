import { Activity, ArrowDownUp, RotateCcw, WifiOff } from "lucide-react";

import { OptimizerStatsView } from "@/components/traffic/OptimizerStatsView";
import { ThroughputSparkline } from "@/components/traffic/ThroughputSparkline";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useClient } from "@/context/ClientContext";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

export function ClientTraffic() {
	const {
		status,
		cumulativeStats,
		statsViewMode,
		setStatsViewMode,
		throughputSamples,
		uptimeSeconds,
		formatUptime,
		isConnected,
		rawBytes,
		wireBytes,
		savedRatio,
		handleResetStats,
	} = useClient();

	const lifetimeRaw = (cumulativeStats?.raw_bytes ?? 0) + (isConnected ? rawBytes : 0);
	const lifetimeWire = (cumulativeStats?.wire_bytes ?? 0) + (isConnected ? wireBytes : 0);
	const lifetimeSavedRatio =
		lifetimeRaw > 0 && lifetimeWire <= lifetimeRaw
			? ((lifetimeRaw - lifetimeWire) / lifetimeRaw) * 100
			: 0;

	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-y-auto">
			<div className="flex flex-none select-none items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
				<div className="flex items-center gap-2 min-w-0">
					<ArrowDownUp className="h-4 w-4 text-primary flex-none" />
					<div className="min-w-0">
						<h1 className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground">
							{m.client_traffic_title()}
						</h1>
						<p className="truncate text-[10px] text-muted-foreground hidden sm:block">
							{m.client_traffic_description()}
						</p>
					</div>
				</div>
				<div className="flex items-center gap-1.5 flex-none">
					<div className="flex items-center rounded border border-input p-0.5 text-[9px]">
						<button
							type="button"
							onClick={() => setStatsViewMode("session")}
							className={cn(
								"rounded px-1.5 py-0.5 font-medium transition cursor-pointer",
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
								"rounded px-1.5 py-0.5 font-medium transition cursor-pointer",
								statsViewMode === "lifetime"
									? "bg-primary text-primary-foreground font-semibold"
									: "text-muted-foreground hover:text-foreground",
							)}
						>
							{m.client_lifetime()}
						</button>
					</div>
					<Button
						variant="ghost"
						size="xs"
						onClick={() => void handleResetStats()}
						className="h-7 px-2 text-[10px] text-muted-foreground hover:text-destructive cursor-pointer"
						title={m.client_reset_stats()}
					>
						<RotateCcw className="h-3 w-3" />
						{m.client_reset()}
					</Button>
				</div>
			</div>

			{!isConnected && !(cumulativeStats && cumulativeStats.raw_bytes > 0) ? (
				<div className="flex flex-1 flex-col items-center justify-center rounded-lg border border-border bg-card p-8 text-center text-muted-foreground shadow-xs">
					<WifiOff className="mb-2 h-7 w-7 text-muted-foreground/50" />
					<p className="text-xs font-medium text-foreground">{m.client_traffic_disconnected()}</p>
					<p className="mt-1 max-w-sm text-[11px] text-muted-foreground">
						{m.client_traffic_disconnected_hint()}
					</p>
				</div>
			) : (
				<>
					<div className="grid grid-cols-4 gap-1.5 text-center font-mono">
						<div className="rounded-lg border border-border bg-card px-1.5 py-2 shadow-xs">
							<div className="text-[9px] uppercase text-muted-foreground">
								{statsViewMode === "session" ? m.client_uptime() : m.client_sessions()}
							</div>
							<div className="text-xs font-bold text-foreground truncate">
								{statsViewMode === "session"
									? formatUptime(uptimeSeconds)
									: (cumulativeStats?.sessions_count ?? 0) + (isConnected ? 1 : 0)}
							</div>
						</div>
						<div className="rounded-lg border border-border bg-card px-1.5 py-2 shadow-xs">
							<div className="text-[9px] uppercase text-muted-foreground">{m.client_raw()}</div>
							<div className="text-xs font-bold text-foreground truncate">
								{formatBytes(statsViewMode === "session" ? rawBytes : lifetimeRaw)}
							</div>
						</div>
						<div className="rounded-lg border border-border bg-card px-1.5 py-2 shadow-xs">
							<div className="text-[9px] uppercase text-muted-foreground">{m.client_wire()}</div>
							<div className="text-xs font-bold text-foreground truncate">
								{formatBytes(statsViewMode === "session" ? wireBytes : lifetimeWire)}
							</div>
						</div>
						<div className="rounded-lg border border-border bg-card px-1.5 py-2 shadow-xs">
							<div className="text-[9px] uppercase text-emerald-500">{m.client_saved()}</div>
							<div className="text-xs font-bold text-emerald-500 truncate">
								{statsViewMode === "session"
									? `${savedRatio.toFixed(1)}%`
									: `${lifetimeSavedRatio.toFixed(1)}%`}
							</div>
						</div>
					</div>

					{isConnected ? (
						<div className="rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
							<div className="mb-1 flex items-center justify-between gap-2 text-[10px] text-muted-foreground">
								<div className="flex items-center gap-1.5">
									<Activity className="h-3 w-3 text-emerald-500" />
									<span>{m.traffic_throughput()}</span>
								</div>
								<span className="font-mono font-semibold text-emerald-500">
									{formatBytes(throughputSamples[throughputSamples.length - 1] || 0)}/s
								</span>
							</div>
							<ThroughputSparkline
								samples={throughputSamples}
								className="h-8 w-full overflow-visible"
							/>
						</div>
					) : null}

					<div className="rounded-lg border border-border bg-card p-3 shadow-xs">
						<div className="mb-3 flex items-center justify-between">
							<span className="text-xs font-semibold text-foreground">
								{statsViewMode === "session"
									? m.client_current_session()
									: m.client_cumulative_lifetime()}
							</span>
							{isConnected ? (
								<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4 font-mono">
									{status?.actual_transport || status?.transport || "—"}
								</Badge>
							) : null}
						</div>
						{statsViewMode === "session" ? (
							<OptimizerStatsView stats={status?.stats} />
						) : (
							<div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
								<div className="rounded-lg border border-border/60 bg-muted/30 px-3 py-2">
									<div className="text-[10px] uppercase text-muted-foreground">
										{m.client_raw()}
									</div>
									<div className="mt-1 font-mono text-sm font-semibold">
										{formatBytes(lifetimeRaw)}
									</div>
								</div>
								<div className="rounded-lg border border-border/60 bg-muted/30 px-3 py-2">
									<div className="text-[10px] uppercase text-muted-foreground">
										{m.client_wire()}
									</div>
									<div className="mt-1 font-mono text-sm font-semibold">
										{formatBytes(lifetimeWire)}
									</div>
								</div>
								<div className="rounded-lg border border-border/60 bg-muted/30 px-3 py-2">
									<div className="text-[10px] uppercase text-emerald-500">{m.client_saved()}</div>
									<div className="mt-1 font-mono text-sm font-semibold text-emerald-500">
										{formatBytes(Math.max(0, lifetimeRaw - lifetimeWire))} (
										{lifetimeSavedRatio.toFixed(1)}%)
									</div>
								</div>
							</div>
						)}
					</div>
				</>
			)}
		</div>
	);
}
