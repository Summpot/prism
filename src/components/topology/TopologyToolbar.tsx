import {
	Activity,
	ArrowDown,
	ArrowUp,
	Cable,
	Filter,
	Radio,
	RefreshCw,
	Search,
	Zap,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { formatBitsPerSecond, formatPercentage } from "@/lib/format";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

import type { TopologyFilter, TopologySummary } from "./topologyTypes";

interface TopologyToolbarProps {
	summary: TopologySummary;
	filter: TopologyFilter;
	onFilterChange: (newFilter: TopologyFilter) => void;
	autoRefresh: boolean;
	onToggleAutoRefresh: () => void;
	refreshInterval: number;
	onIntervalChange: (interval: number) => void;
	onRefresh: () => void;
	loading: boolean;
}

export function TopologyToolbar({
	summary,
	filter,
	onFilterChange,
	autoRefresh,
	onToggleAutoRefresh,
	refreshInterval,
	onIntervalChange,
	onRefresh,
	loading,
}: TopologyToolbarProps) {
	return (
		<div className="space-y-3">
			{/* Top Metric Stats Cards */}
			<div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
				{/* 1. Active Nodes */}
				<div className="rounded-lg border border-border/80 bg-card/60 p-2.5 shadow-xs">
					<div className="text-[10px] uppercase font-semibold text-muted-foreground flex items-center gap-1.5">
						<Activity className="h-3.5 w-3.5 text-sky-500" />
						<span>{m.topology_nodes_count({ count: summary.totalNodes })}</span>
					</div>
					<div className="mt-1 text-base font-bold font-mono text-foreground flex items-baseline gap-1.5">
						<span>{summary.activeNodes}</span>
						<span className="text-xs text-muted-foreground font-normal">
							/ {summary.totalNodes} active
						</span>
					</div>
				</div>

				{/* 2. Active Paths */}
				<div className="rounded-lg border border-border/80 bg-card/60 p-2.5 shadow-xs">
					<div className="text-[10px] uppercase font-semibold text-muted-foreground flex items-center gap-1.5">
						<Cable className="h-3.5 w-3.5 text-amber-500" />
						<span>{m.topology_paths_count({ count: summary.totalPaths })}</span>
					</div>
					<div className="mt-1 text-base font-bold font-mono text-foreground flex items-baseline gap-1.5">
						<span>{summary.activePaths}</span>
						<span className="text-xs text-muted-foreground font-normal">
							/ {summary.totalPaths} active
						</span>
					</div>
				</div>

				{/* 3. Live Ingress Rate */}
				<div className="rounded-lg border border-border/80 bg-card/60 p-2.5 shadow-xs">
					<div className="text-[10px] uppercase font-semibold text-muted-foreground flex items-center gap-1.5">
						<ArrowUp className="h-3.5 w-3.5 text-emerald-500" />
						<span>Ingress (↑)</span>
					</div>
					<div className="mt-1 text-base font-bold font-mono text-emerald-600 dark:text-emerald-400">
						{formatBitsPerSecond(summary.ingressRateBps)}
					</div>
				</div>

				{/* 4. Live Egress Rate */}
				<div className="rounded-lg border border-border/80 bg-card/60 p-2.5 shadow-xs">
					<div className="text-[10px] uppercase font-semibold text-muted-foreground flex items-center gap-1.5">
						<ArrowDown className="h-3.5 w-3.5 text-blue-500" />
						<span>Egress (↓)</span>
					</div>
					<div className="mt-1 text-base font-bold font-mono text-blue-600 dark:text-blue-400">
						{formatBitsPerSecond(summary.egressRateBps)}
					</div>
				</div>

				{/* 5. Active Sessions */}
				<div className="rounded-lg border border-border/80 bg-card/60 p-2.5 shadow-xs">
					<div className="text-[10px] uppercase font-semibold text-muted-foreground flex items-center gap-1.5">
						<Radio className="h-3.5 w-3.5 text-purple-500" />
						<span>{m.topology_active_sessions({ count: summary.totalSessions })}</span>
					</div>
					<div className="mt-1 text-base font-bold font-mono text-foreground">
						{summary.totalSessions}
					</div>
				</div>

				{/* 6. Optimizer Savings */}
				<div className="rounded-lg border border-border/80 bg-card/60 p-2.5 shadow-xs">
					<div className="text-[10px] uppercase font-semibold text-muted-foreground flex items-center gap-1.5">
						<Zap className="h-3.5 w-3.5 text-amber-500" />
						<span>Optimizer</span>
					</div>
					<div className="mt-1 text-base font-bold font-mono text-foreground">
						{summary.savedRatio > 0 ? (
							<span className="text-emerald-500">{formatPercentage(summary.savedRatio)}</span>
						) : (
							<span className="text-muted-foreground">0%</span>
						)}
					</div>
				</div>
			</div>

			{/* Filters & Control Bar */}
			<div className="flex flex-wrap items-center justify-between gap-2.5 rounded-lg border border-border bg-card/70 px-3 py-2">
				{/* Left: Search & Protocol Filters */}
				<div className="flex flex-wrap items-center gap-2 flex-1 min-w-[280px]">
					{/* Search input */}
					<div className="relative flex-1 max-w-xs">
						<Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
						<input
							type="text"
							value={filter.searchQuery}
							onChange={(e) => onFilterChange({ ...filter, searchQuery: e.target.value })}
							placeholder={m.topology_filter_search()}
							className="h-8 w-full rounded-md border border-border bg-background/80 pl-8 pr-3 text-xs text-foreground placeholder:text-muted-foreground/60 focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary"
						/>
					</div>

					{/* Protocol Filter Pills */}
					<div className="inline-flex rounded-md border border-border bg-muted/40 p-0.5 text-xs">
						{(["all", "tcp", "udp", "tunnel"] as const).map((p) => {
							const active = filter.protocol === p;
							return (
								<button
									key={p}
									type="button"
									onClick={() => onFilterChange({ ...filter, protocol: p })}
									className={cn(
										"rounded px-2.5 py-1 font-medium transition-all text-xs cursor-pointer",
										active
											? "bg-card text-foreground shadow-xs"
											: "text-muted-foreground hover:text-foreground",
									)}
								>
									{p === "all"
										? m.topology_filter_proto_all()
										: p === "tcp"
											? m.topology_filter_proto_tcp()
											: p === "udp"
												? m.topology_filter_proto_udp()
												: m.topology_filter_proto_tunnel()}
								</button>
							);
						})}
					</div>

					{/* Active Only toggle */}
					<button
						type="button"
						onClick={() => onFilterChange({ ...filter, activeOnly: !filter.activeOnly })}
						className={cn(
							"inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium border transition-all cursor-pointer",
							filter.activeOnly
								? "border-emerald-500/40 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
								: "border-border bg-muted/40 text-muted-foreground hover:text-foreground",
						)}
					>
						<Filter className="h-3 w-3" />
						<span>{m.topology_filter_active_only()}</span>
					</button>
				</div>

				{/* Right: Refresh Controls & Interval */}
				<div className="flex items-center gap-2">
					<div className="flex items-center gap-1">
						{([2000, 3000, 5000] as const).map((ms) => (
							<button
								key={ms}
								type="button"
								onClick={() => onIntervalChange(ms)}
								className={cn(
									"px-1.5 py-0.5 rounded text-[10px] font-mono transition-colors cursor-pointer",
									refreshInterval === ms
										? "bg-primary/20 text-primary font-semibold"
										: "text-muted-foreground hover:text-foreground",
								)}
							>
								{ms / 1000}s
							</button>
						))}
					</div>

					<button
						type="button"
						onClick={onToggleAutoRefresh}
						className={cn(
							"inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-medium border transition-all cursor-pointer",
							autoRefresh
								? "border-primary/40 bg-primary/10 text-primary"
								: "border-border bg-muted/40 text-muted-foreground hover:text-foreground",
						)}
					>
						<span
							className={cn(
								"h-1.5 w-1.5 rounded-full",
								autoRefresh ? "bg-primary animate-pulse" : "bg-muted-foreground",
							)}
						/>
						<span>
							{m.admin_auto_refresh({
								state: autoRefresh ? m.admin_on() : m.admin_off(),
							})}
						</span>
					</button>

					<Button
						variant="outline"
						size="sm"
						onClick={onRefresh}
						disabled={loading}
						className="h-8 gap-1 text-xs cursor-pointer"
					>
						<RefreshCw className={cn("h-3.5 w-3.5", loading ? "animate-spin" : "")} />
						<span>{m.common_refresh()}</span>
					</Button>
				</div>
			</div>
		</div>
	);
}
