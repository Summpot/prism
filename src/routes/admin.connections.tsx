import { createFileRoute, Link } from "@tanstack/react-router";
import { Cable, Wifi, Zap } from "lucide-react";
import { useCallback, useMemo, useState } from "react";

import {
	CountChip,
	EmptyState,
	ErrorBanner,
	PageHeader,
	RefreshButton,
	SearchInput,
	StateCard,
	ToggleChip,
} from "@/components/ui";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { formatBytes, formatDuration, formatPercentage, formatTime } from "@/lib/format";
import {
	getConnections,
	getOptimizerStats,
	type SessionInfo,
	type OptimizerOverviewResponse,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/connections")({
	component: AdminConnectionsPage,
});

function AdminConnectionsPage() {
	const { connection, ready } = usePanelSession();
	const [conns, setConns] = useState<SessionInfo[]>([]);
	const [optimizerStats, setOptimizerStats] = useState<OptimizerOverviewResponse | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [query, setQuery] = useState("");
	const [autoRefresh, setAutoRefresh] = useState(true);

	const fetchConns = useCallback(() => {
		if (!connection) {
			setConns([]);
			setOptimizerStats(null);
			return;
		}

		setLoading(true);
		setError(null);

		Promise.all([getConnections(connection), getOptimizerStats(connection).catch(() => null)])
			.then(([connsResp, statsResp]) => {
				setConns(connsResp);
				setOptimizerStats(statsResp);
			})
			.catch((nextError) => {
				setError(nextError instanceof Error ? nextError.message : String(nextError));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	usePolling(fetchConns, 3_000, Boolean(connection) && autoRefresh);

	const filtered = useMemo(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) {
			return conns;
		}
		return conns.filter((conn) =>
			[conn.id, conn.client, conn.host, conn.upstream].join(" ").toLowerCase().includes(needle),
		);
	}, [conns, query]);

	if (!ready) {
		return <StateCard label={m.common_restoring_session()} />;
	}

	if (!connection) {
		return <StateCard label={m.common_connect_panel()} />;
	}

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow={m.admin_proxy_plane()}
				title={m.admin_active_connections()}
				description={m.admin_active_connections_description()}
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							{m.admin_auto_refresh({ state: autoRefresh ? m.admin_on() : m.admin_off() })}
						</ToggleChip>
						<RefreshButton onClick={fetchConns} loading={loading} />
						{optimizerStats?.global && optimizerStats.global.raw_bytes > 0 ? (
							<Link
								to="/admin/traffic"
								className="inline-flex items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2 text-sm text-emerald-600 dark:text-emerald-400 hover:bg-muted/70"
							>
								<Zap className="h-4 w-4" />
								{m.admin_optimizer_saved({
									bytes: formatBytes(optimizerStats.global.saved_bytes),
									percentage: formatPercentage(optimizerStats.global.saved_ratio),
								})}
							</Link>
						) : null}
						<CountChip icon={<Cable className="h-4 w-4" />}>
							{loading
								? m.common_loading()
								: conns.length === 1
									? m.admin_connections_count_one({ shown: filtered.length, total: conns.length })
									: m.admin_connections_count({ shown: filtered.length, total: conns.length })}
						</CountChip>
					</>
				}
			/>

			<SearchInput value={query} onChange={setQuery} placeholder={m.admin_filter_connections()} />

			{error ? <ErrorBanner message={error} onRetry={fetchConns} /> : null}

			{filtered.length > 0 ? (
				<div className="overflow-hidden rounded-xl border border-border bg-card shadow-xs">
					<div className="hidden md:block">
						<Table>
							<TableHeader>
								<TableRow>
									<TableHead>{m.admin_client()}</TableHead>
									<TableHead>{m.admin_host()}</TableHead>
									<TableHead>{m.admin_upstream()}</TableHead>
									<TableHead>{m.admin_optimizer()}</TableHead>
									<TableHead>{m.admin_started()}</TableHead>
									<TableHead>{m.admin_duration()}</TableHead>
								</TableRow>
							</TableHeader>
							<TableBody>
								{filtered.map((conn) => (
									<TableRow key={conn.id}>
										<TableCell className="font-mono text-xs">{conn.client}</TableCell>
										<TableCell>{conn.host || "—"}</TableCell>
										<TableCell className="font-mono text-xs">{conn.upstream}</TableCell>
										<TableCell className="font-mono text-xs whitespace-normal">
											<OptimizerCell conn={conn} />
										</TableCell>
										<TableCell className="text-muted-foreground">
											{formatTime(conn.started_at_unix_ms)}
										</TableCell>
										<TableCell className="text-muted-foreground">
											{formatDuration(conn.started_at_unix_ms)}
										</TableCell>
									</TableRow>
								))}
							</TableBody>
						</Table>
					</div>
					<div className="divide-y divide-border md:hidden">
						{filtered.map((conn) => (
							<div key={conn.id} className="space-y-3 px-4 py-4">
								<div className="flex items-center gap-2">
									<Wifi className="h-4 w-4 text-muted-foreground" />
									<span className="font-mono text-sm">{conn.client}</span>
								</div>
								<div className="grid grid-cols-2 gap-3">
									<MiniValue label={m.admin_host()} value={conn.host || "—"} />
									<MiniValue label={m.admin_upstream()} value={conn.upstream} />
									<MiniValue
										label={m.admin_optimizer()}
										value={
											conn.raw_bytes || conn.wire_bytes
												? `${formatBytes(conn.raw_bytes)} → ${formatBytes(conn.wire_bytes)}`
												: "—"
										}
									/>
									<MiniValue
										label={m.admin_duration()}
										value={formatDuration(conn.started_at_unix_ms)}
									/>
								</div>
							</div>
						))}
					</div>
				</div>
			) : !loading ? (
				<EmptyState
					label={conns.length === 0 ? m.admin_no_connections() : m.admin_no_connection_match()}
				/>
			) : null}
		</div>
	);
}

function OptimizerCell({ conn }: { conn: SessionInfo }) {
	if (!(conn.raw_bytes || conn.wire_bytes)) {
		return <span className="text-muted-foreground">—</span>;
	}

	return (
		<div className="space-y-1">
			{conn.uplink_raw_bytes || conn.uplink_wire_bytes ? (
				<div className="flex items-center gap-1.5 text-[11px]">
					<span className="font-bold text-primary">↑</span>
					<span className="text-muted-foreground">{formatBytes(conn.uplink_raw_bytes || 0)}</span>
					<span className="text-muted-foreground/60">→</span>
					<span>{formatBytes(conn.uplink_wire_bytes || 0)}</span>
				</div>
			) : null}
			{conn.downlink_raw_bytes || conn.downlink_wire_bytes ? (
				<div className="flex items-center gap-1.5 text-[11px]">
					<span className="font-bold text-emerald-500">↓</span>
					<span className="text-muted-foreground">{formatBytes(conn.downlink_raw_bytes || 0)}</span>
					<span className="text-muted-foreground/60">→</span>
					<span className="text-emerald-600 dark:text-emerald-400">
						{formatBytes(conn.downlink_wire_bytes || 0)}
					</span>
				</div>
			) : null}
			{!conn.uplink_raw_bytes && !conn.downlink_raw_bytes ? (
				<div className="flex items-center gap-1.5">
					<span className="text-muted-foreground">{formatBytes(conn.raw_bytes)}</span>
					<span className="text-muted-foreground/60">→</span>
					<span className="font-medium">{formatBytes(conn.wire_bytes)}</span>
					{conn.raw_bytes && conn.wire_bytes && conn.raw_bytes > conn.wire_bytes ? (
						<span className="ml-1 rounded bg-emerald-500/15 px-1.5 py-0.5 text-[11px] text-emerald-600 dark:text-emerald-400">
							-{formatPercentage(1 - conn.wire_bytes / conn.raw_bytes)}
						</span>
					) : null}
				</div>
			) : null}
		</div>
	);
}

function MiniValue({ label, value }: { label: string; value: string }) {
	return (
		<div className="rounded-lg border border-border bg-muted/30 px-3 py-2">
			<div className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
				{label}
			</div>
			<div className="mt-1 truncate text-xs text-foreground">{value}</div>
		</div>
	);
}
