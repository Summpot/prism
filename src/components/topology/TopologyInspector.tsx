import {
	Activity,
	ArrowDown,
	ArrowRight,
	ArrowUp,
	Cable,
	Clock,
	Info,
	Radio,
	X,
	Zap,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
	formatBitsPerSecond,
	formatBytes,
	formatDuration,
	formatGainMs,
	formatPercentage,
} from "@/lib/format";
import { m } from "@/paraglide/messages";

import type { SelectedElement } from "./topologyTypes";

interface TopologyInspectorProps {
	selected: SelectedElement;
	onClose: () => void;
}

export function TopologyInspector({ selected, onClose }: TopologyInspectorProps) {
	if (!selected) return null;

	return (
		<div className="rounded-xl border border-border bg-card p-4 shadow-sm space-y-4 animate-in fade-in slide-in-from-bottom-2 duration-200">
			{/* Inspector Header */}
			<div className="flex items-center justify-between border-b border-border/80 pb-3">
				<div className="flex items-center gap-2 min-w-0">
					<div className="p-1.5 rounded-lg bg-primary/10 text-primary">
						{selected.type === "path" ? (
							<Cable className="h-4 w-4" />
						) : (
							<Activity className="h-4 w-4" />
						)}
					</div>
					<div>
						<h3 className="text-sm font-semibold text-foreground">
							{selected.type === "path"
								? m.topology_inspector_path_title()
								: m.topology_inspector_node_title()}
						</h3>
						<p className="text-[11px] text-muted-foreground truncate">
							{selected.type === "path"
								? `${selected.sourceNode?.label ?? selected.path.source} → ${selected.targetNode?.label ?? selected.path.target}`
								: `${selected.node.label} (${selected.node.type})`}
						</p>
					</div>
				</div>

				<Button
					variant="ghost"
					size="icon-xs"
					onClick={onClose}
					className="text-muted-foreground hover:text-foreground cursor-pointer"
				>
					<X className="h-4 w-4" />
				</Button>
			</div>

			{/* Inspector Content: PATH */}
			{selected.type === "path" && (
				<div className="space-y-4">
					{/* Path Endpoints Overview */}
					<div className="flex items-center justify-between rounded-lg border border-border/60 bg-muted/30 p-3">
						<div className="min-w-0">
							<span className="text-[10px] uppercase font-bold text-muted-foreground tracking-wider">
								{m.topology_inspector_source()}
							</span>
							<p className="text-xs font-semibold text-foreground truncate">
								{selected.sourceNode?.label ?? selected.path.source}
							</p>
							<span className="text-[10px] text-muted-foreground font-mono">
								{selected.sourceNode?.sublabel || selected.sourceNode?.type}
							</span>
						</div>

						<ArrowRight className="h-4 w-4 text-muted-foreground flex-none mx-3" />

						<div className="min-w-0 text-right">
							<span className="text-[10px] uppercase font-bold text-muted-foreground tracking-wider">
								{m.topology_inspector_target()}
							</span>
							<p className="text-xs font-semibold text-foreground truncate">
								{selected.targetNode?.label ?? selected.path.target}
							</p>
							<span className="text-[10px] text-muted-foreground font-mono">
								{selected.targetNode?.sublabel || selected.targetNode?.type}
							</span>
						</div>
					</div>

					{/* Live Traffic Stats Grid */}
					<div className="grid grid-cols-2 sm:grid-cols-4 gap-2.5">
						<div className="rounded-lg border border-border/70 bg-card p-2.5 shadow-2xs">
							<span className="text-[10px] font-medium text-muted-foreground flex items-center gap-1">
								<ArrowUp className="h-3 w-3 text-emerald-500" />
								{m.topology_inspector_rate_uplink()}
							</span>
							<p className="mt-1 text-sm font-bold font-mono text-emerald-600 dark:text-emerald-400">
								{formatBitsPerSecond(selected.path.uplinkBps)}
							</p>
						</div>

						<div className="rounded-lg border border-border/70 bg-card p-2.5 shadow-2xs">
							<span className="text-[10px] font-medium text-muted-foreground flex items-center gap-1">
								<ArrowDown className="h-3 w-3 text-blue-500" />
								{m.topology_inspector_rate_downlink()}
							</span>
							<p className="mt-1 text-sm font-bold font-mono text-blue-600 dark:text-blue-400">
								{formatBitsPerSecond(selected.path.downlinkBps)}
							</p>
						</div>

						<div className="rounded-lg border border-border/70 bg-card p-2.5 shadow-2xs">
							<span className="text-[10px] font-medium text-muted-foreground">
								{m.topology_inspector_raw_bytes()}
							</span>
							<p className="mt-1 text-sm font-bold font-mono text-foreground">
								{formatBytes(selected.path.totalRawBytes)}
							</p>
						</div>

						<div className="rounded-lg border border-border/70 bg-card p-2.5 shadow-2xs">
							<span className="text-[10px] font-medium text-muted-foreground">
								{m.topology_inspector_wire_bytes()}
							</span>
							<p className="mt-1 text-sm font-bold font-mono text-foreground">
								{formatBytes(selected.path.totalWireBytes)}
								{selected.path.savedRatio ? (
									<span className="text-[10px] text-emerald-500 ml-1">
										({formatPercentage(selected.path.savedRatio)})
									</span>
								) : null}
							</p>
						</div>
					</div>

					{/* Optimizer Gain Info (if available) */}
					{selected.path.netGainMs !== undefined && (
						<div className="flex items-center gap-2 rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300">
							<Zap className="h-4 w-4 flex-none" />
							<span>
								{m.topology_inspector_net_gain()}：{formatGainMs(selected.path.netGainMs)}
							</span>
						</div>
					)}

					{/* Active Sessions on this Path */}
					<div className="space-y-2">
						<div className="flex items-center justify-between">
							<span className="text-xs font-semibold text-foreground flex items-center gap-1.5">
								<Radio className="h-3.5 w-3.5 text-primary" />
								{m.topology_inspector_sessions()} ({selected.path.sessions.length})
							</span>
						</div>

						{selected.path.sessions.length > 0 ? (
							<div className="max-h-48 overflow-y-auto divide-y divide-border/60 rounded-lg border border-border bg-card/60">
								{selected.path.sessions.map((sess) => (
									<div
										key={sess.id}
										className="flex items-center justify-between p-2.5 text-xs hover:bg-accent/20 transition-colors"
									>
										<div className="min-w-0 pr-2">
											<p className="font-mono text-xs font-medium text-foreground truncate">
												{sess.client}
											</p>
											<p className="text-[11px] text-muted-foreground truncate">
												Host: {sess.host || "—"} → {sess.upstream}
											</p>
										</div>
										<div className="text-right flex-none font-mono text-[11px] text-muted-foreground">
											<p>{formatBytes(sess.raw_bytes)}</p>
											<p className="text-[10px] flex items-center justify-end gap-1">
												<Clock className="h-2.5 w-2.5" />
												{formatDuration(sess.started_at_unix_ms)}
											</p>
										</div>
									</div>
								))}
							</div>
						) : (
							<p className="text-xs text-muted-foreground italic py-2">
								{m.topology_inspector_no_sessions()}
							</p>
						)}
					</div>
				</div>
			)}

			{/* Inspector Content: NODE */}
			{selected.type === "node" && (
				<div className="space-y-4">
					<div className="grid grid-cols-2 sm:grid-cols-3 gap-2.5">
						<div className="rounded-lg border border-border/70 bg-card p-2.5 shadow-2xs">
							<span className="text-[10px] font-medium text-muted-foreground">Node Type</span>
							<p className="mt-1 text-xs font-bold uppercase font-mono text-primary">
								{selected.node.type}
							</p>
						</div>

						<div className="rounded-lg border border-border/70 bg-card p-2.5 shadow-2xs">
							<span className="text-[10px] font-medium text-muted-foreground">Active Sessions</span>
							<p className="mt-1 text-sm font-bold font-mono text-foreground">
								{selected.node.activeSessions}
							</p>
						</div>

						<div className="rounded-lg border border-border/70 bg-card p-2.5 shadow-2xs">
							<span className="text-[10px] font-medium text-muted-foreground">Protocol</span>
							<p className="mt-1 text-xs font-bold uppercase font-mono text-foreground">
								{selected.node.protocol || "Any"}
							</p>
						</div>
					</div>

					{/* Node Metadata / Details */}
					{selected.node.details && (
						<div className="space-y-2">
							<span className="text-xs font-semibold text-foreground flex items-center gap-1">
								<Info className="h-3.5 w-3.5 text-muted-foreground" />
								Node Details
							</span>
							<pre className="p-2.5 rounded-lg border border-border bg-muted/40 text-[11px] font-mono text-muted-foreground overflow-x-auto whitespace-pre-wrap">
								{JSON.stringify(selected.node.details, null, 2)}
							</pre>
						</div>
					)}
				</div>
			)}
		</div>
	);
}
