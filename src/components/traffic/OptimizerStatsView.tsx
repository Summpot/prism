import { Activity } from "lucide-react";

import { NestedPanel } from "@/components/ui";
import {
	formatBitsPerSecond,
	formatBytes,
	formatCostMs,
	formatGainMs,
	formatPercentage,
} from "@/lib/format";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";
import type { DirectionStatsSnapshot, OptimizerStatsSnapshot, Quantiles } from "@/types/admin";

export function directionTooltip(label: string, dir?: DirectionStatsSnapshot): string {
	if (!dir) return m.client_dir_tooltip_none({ label });
	return m.client_dir_tooltip({
		label,
		net: dir.net_gain_ms.toFixed(1),
		gain: dir.transfer_gain_ms.toFixed(1),
		batching: dir.batching_penalty_ms.toFixed(1),
		compression: dir.compression_penalty_ms.toFixed(1),
		p99: dir.batching_delay.p99_us.toFixed(0),
	});
}

export function hasOptimizerTraffic(stats?: OptimizerStatsSnapshot | null): boolean {
	if (!stats) return false;
	return (
		stats.raw_bytes > 0 ||
		stats.wire_bytes > 0 ||
		stats.urgent_batches > 0 ||
		stats.timer_batches > 0 ||
		stats.threshold_batches > 0
	);
}

function StatTile({
	label,
	value,
	hint,
	tone,
}: {
	label: string;
	value: string;
	hint?: string;
	tone?: "default" | "good" | "warn";
}) {
	return (
		<div className="rounded-lg border border-border/60 bg-muted/30 px-3 py-2">
			<div className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
				{label}
			</div>
			<div
				className={cn(
					"mt-1 truncate font-mono text-sm font-semibold",
					tone === "good"
						? "text-emerald-500"
						: tone === "warn"
							? "text-amber-500"
							: "text-foreground",
				)}
			>
				{value}
			</div>
			{hint ? (
				<div className="mt-0.5 truncate text-[10px] text-muted-foreground">{hint}</div>
			) : null}
		</div>
	);
}

function QuantileRow({ label, q }: { label: string; q: Quantiles }) {
	return (
		<div className="grid grid-cols-5 gap-1 text-center font-mono text-[10px]">
			<div className="text-left text-[10px] font-sans font-medium text-muted-foreground self-center">
				{label}
			</div>
			<MiniQ label={m.traffic_p50()} value={`${q.p50_us.toFixed(0)}µs`} />
			<MiniQ label={m.traffic_p90()} value={`${q.p90_us.toFixed(0)}µs`} />
			<MiniQ label={m.traffic_p99()} value={`${q.p99_us.toFixed(0)}µs`} />
			<MiniQ label={m.traffic_max()} value={`${q.max_us.toFixed(0)}µs`} />
		</div>
	);
}

function MiniQ({ label, value }: { label: string; value: string }) {
	return (
		<div className="rounded bg-muted/40 px-1.5 py-1">
			<div className="text-[9px] uppercase text-muted-foreground">{label}</div>
			<div className="text-[10px] font-semibold text-foreground">{value}</div>
		</div>
	);
}

function DirectionCard({
	title,
	arrow,
	dir,
}: {
	title: string;
	arrow: string;
	dir?: DirectionStatsSnapshot;
}) {
	return (
		<NestedPanel className="space-y-3 p-3">
			<div className="flex items-center justify-between gap-2">
				<div className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
					<span className="font-bold text-primary">{arrow}</span>
					{title}
				</div>
				<span className="font-mono text-[11px] text-muted-foreground">
					{formatBytes(dir?.raw_bytes ?? 0)} → {formatBytes(dir?.wire_bytes ?? 0)}
				</span>
			</div>
			<div className="grid grid-cols-3 gap-1.5">
				<StatTile label={m.client_saved()} value={formatPercentage(dir?.saved_ratio)} tone="good" />
				<StatTile
					label={m.client_gain()}
					value={formatGainMs(dir?.transfer_gain_ms ?? 0)}
					tone="good"
				/>
				<StatTile
					label={m.client_net()}
					value={formatGainMs(dir?.net_gain_ms ?? 0)}
					tone={(dir?.net_gain_ms ?? 0) >= 0 ? "good" : "warn"}
				/>
			</div>
			{dir ? (
				<div className="space-y-1.5">
					<QuantileRow label={m.client_batching()} q={dir.batching_delay} />
					<QuantileRow label={m.client_compression()} q={dir.compression_time} />
				</div>
			) : null}
		</NestedPanel>
	);
}

export function OptimizerStatsView({
	stats,
	emptyLabel,
}: {
	stats?: OptimizerStatsSnapshot | null;
	emptyLabel?: string;
}) {
	if (!hasOptimizerTraffic(stats) || !stats) {
		return (
			<div className="rounded-lg border border-dashed border-border bg-muted/20 px-4 py-8 text-center text-xs text-muted-foreground">
				{emptyLabel ?? m.traffic_no_data()}
			</div>
		);
	}

	const batchingCostMs = (stats.batching_delay_us ?? 0) / 1000;
	const compressionCostMs =
		((stats.compression_time_us ?? 0) + (stats.decompression_time_us ?? 0)) / 1000;
	const linkRateText = stats.link_rate_measured
		? m.client_link_rate_measured({ rate: ((stats.link_rate_bps ?? 0) / 1e6).toFixed(1) })
		: m.client_link_rate_estimated({ rate: ((stats.link_rate_bps ?? 0) / 1e6).toFixed(1) });

	return (
		<div className="space-y-3">
			<div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
				<StatTile label={m.client_raw()} value={formatBytes(stats.raw_bytes)} />
				<StatTile label={m.client_wire()} value={formatBytes(stats.wire_bytes)} />
				<StatTile
					label={m.client_saved()}
					value={`${formatBytes(stats.saved_bytes)} (${formatPercentage(stats.saved_ratio)})`}
					tone="good"
				/>
				<StatTile
					label={m.client_net()}
					value={formatGainMs(stats.net_gain_ms)}
					hint={linkRateText}
					tone={stats.net_gain_ms >= 0 ? "good" : "warn"}
				/>
			</div>

			<div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
				<StatTile
					label={m.client_gain()}
					value={formatGainMs(stats.transfer_gain_ms)}
					tone="good"
				/>
				<StatTile
					label={m.client_batching()}
					value={formatCostMs(batchingCostMs)}
					tone={batchingCostMs !== 0 ? "warn" : "default"}
				/>
				<StatTile
					label={m.client_compression()}
					value={formatCostMs(compressionCostMs)}
					tone={compressionCostMs !== 0 ? "warn" : "default"}
				/>
				<StatTile
					label={m.traffic_link_rate()}
					value={formatBitsPerSecond(stats.link_rate_bps)}
					hint={stats.link_rate_measured ? m.traffic_link_measured() : m.traffic_link_estimated()}
				/>
			</div>

			<div className="grid grid-cols-3 gap-2">
				<StatTile label={m.traffic_batches_urgent()} value={String(stats.urgent_batches)} />
				<StatTile label={m.traffic_batches_timer()} value={String(stats.timer_batches)} />
				<StatTile label={m.traffic_batches_threshold()} value={String(stats.threshold_batches)} />
			</div>

			{stats.window && stats.window.window_ms > 0 ? (
				<NestedPanel className="p-3">
					<div className="mb-2 flex items-center gap-1.5 text-xs font-semibold text-foreground">
						<Activity className="h-3.5 w-3.5 text-primary" />
						{m.traffic_window({ ms: stats.window.window_ms })}
					</div>
					<div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
						<StatTile label={m.client_raw()} value={formatBytes(stats.window.raw_bytes)} />
						<StatTile label={m.client_wire()} value={formatBytes(stats.window.wire_bytes)} />
						<StatTile
							label={m.client_saved()}
							value={formatPercentage(stats.window.saved_ratio)}
							tone="good"
						/>
						<StatTile
							label={m.client_gain()}
							value={formatGainMs(stats.window.transfer_gain_ms)}
							tone="good"
						/>
					</div>
				</NestedPanel>
			) : null}

			<div className="grid gap-3 lg:grid-cols-2">
				<DirectionCard title={m.traffic_uplink()} arrow="↑" dir={stats.uplink} />
				<DirectionCard title={m.traffic_downlink()} arrow="↓" dir={stats.downlink} />
			</div>
		</div>
	);
}
