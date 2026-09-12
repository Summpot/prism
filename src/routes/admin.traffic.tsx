import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowRight, Server } from "lucide-react";
import { useCallback, useState } from "react";

import { AdminReady } from "@/components/admin/AdminReady";
import { OptimizerStatsView } from "@/components/traffic/OptimizerStatsView";
import { CountChip, ErrorBanner, PageHeader, RefreshButton, ToggleChip } from "@/components/ui";
import { Button } from "@/components/ui/button";
import { formatBytes, formatPercentage } from "@/lib/format";
import {
	getConnections,
	getOptimizerStats,
	type OptimizerOverviewResponse,
	type SessionInfo,
} from "@/lib/managementApi";
import type { PanelConnection } from "@/lib/panelConnection";
import { usePolling } from "@/lib/usePolling";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/traffic")({
	component: AdminTrafficPage,
});

function AdminTrafficPage() {
	return <AdminReady>{(connection) => <AdminTrafficBody connection={connection} />}</AdminReady>;
}

function AdminTrafficBody({ connection }: { connection: PanelConnection }) {
	const [stats, setStats] = useState<OptimizerOverviewResponse | null>(null);
	const [conns, setConns] = useState<SessionInfo[]>([]);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [autoRefresh, setAutoRefresh] = useState(true);

	const fetchData = useCallback(() => {
		setLoading(true);
		setError(null);
		Promise.all([
			getOptimizerStats(connection),
			getConnections(connection).catch(() => [] as SessionInfo[]),
		])
			.then(([nextStats, nextConns]) => {
				setStats(nextStats);
				setConns(nextConns);
			})
			.catch((nextError) => {
				setError(nextError instanceof Error ? nextError.message : String(nextError));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	usePolling(fetchData, 3_000, autoRefresh);

	const rawFromConns = conns.reduce((sum, conn) => sum + (conn.raw_bytes || 0), 0);
	const wireFromConns = conns.reduce((sum, conn) => sum + (conn.wire_bytes || 0), 0);

	return (
		<div className="space-y-5">
			<PageHeader
				eyebrow={m.admin_traffic_eyebrow()}
				title={m.admin_traffic_title()}
				description={m.admin_traffic_description()}
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							{m.admin_auto_refresh({ state: autoRefresh ? m.admin_on() : m.admin_off() })}
						</ToggleChip>
						<RefreshButton onClick={fetchData} loading={loading} />
						<CountChip icon={<Server className="h-4 w-4" />}>
							{stats?.global
								? m.admin_optimizer_saved({
										bytes: formatBytes(stats.global.saved_bytes),
										percentage: formatPercentage(stats.global.saved_ratio),
									})
								: m.common_loading()}
						</CountChip>
						<Button variant="outline" size="sm" render={<Link to="/admin/connectors" />}>
							{m.nav_connector_traffic()}
							<ArrowRight className="h-4 w-4" />
						</Button>
					</>
				}
			/>

			{error ? <ErrorBanner message={error} onRetry={fetchData} /> : null}

			<section className="rounded-lg border border-border bg-card p-4 shadow-xs space-y-3">
				<div className="flex items-center justify-between gap-2">
					<h2 className="text-sm font-semibold text-foreground">{m.admin_traffic_global()}</h2>
					<span className="text-[11px] text-muted-foreground">
						{m.admin_traffic_live_sessions({ count: conns.length })}
						{rawFromConns > 0
							? ` · ${formatBytes(rawFromConns)} → ${formatBytes(wireFromConns)}`
							: ""}
					</span>
				</div>
				<OptimizerStatsView stats={stats?.global} />
			</section>
		</div>
	);
}
