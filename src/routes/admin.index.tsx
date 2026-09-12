import { createFileRoute, Link } from "@tanstack/react-router";
import {
	Activity,
	AlertTriangle,
	ArrowRight,
	CheckCircle2,
	RefreshCw,
	RotateCcw,
	ServerCog,
	Unplug,
	Zap,
} from "lucide-react";
import { useState } from "react";

import {
	Badge,
	CountChip,
	ErrorBanner,
	MetricCard,
	NestedPanel,
	PageHeader,
	RefreshButton,
	ResultBanner,
	SecondaryButton,
	StateCard,
	ToggleChip,
} from "@/components/ui";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { formatBytes, formatPercentage, formatRelative } from "@/lib/format";
import {
	getConnections,
	getManagedNodes,
	getManagementStatus,
	getOptimizerStats,
	getTunnelServices,
	type ManagedNodeSnapshot,
	triggerReload,
} from "@/lib/managementApi";
import { useAdminQuery } from "@/hooks/useAdminQuery";
import { usePanelSession } from "@/lib/panelSession";
import { invalidateAdminQueries } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/")({ component: AdminDashboardPage });

function AdminDashboardPage() {
	const { connection, ready } = usePanelSession();
	const [reloading, setReloading] = useState(false);
	const [reloadResult, setReloadResult] = useState<{ ok: boolean; text: string } | null>(null);
	const [autoRefresh, setAutoRefresh] = useState(true);
	const interval = autoRefresh ? 8_000 : false;

	const statusQuery = useAdminQuery(queryKeys.admin.status(connection), getManagementStatus, {
		refetchInterval: interval,
	});
	const nodesQuery = useAdminQuery(queryKeys.admin.nodes(connection), getManagedNodes, {
		refetchInterval: interval,
	});
	const connsQuery = useAdminQuery(
		queryKeys.admin.connections(connection),
		(conn) => getConnections(conn).catch(() => []),
		{ refetchInterval: interval },
	);
	const servicesQuery = useAdminQuery(
		queryKeys.admin.services(connection),
		(conn) => getTunnelServices(conn).catch(() => []),
		{ refetchInterval: interval },
	);
	const optimizerQuery = useAdminQuery(
		queryKeys.admin.optimizer(connection),
		(conn) => getOptimizerStats(conn).catch(() => null),
		{ refetchInterval: interval },
	);

	const status = statusQuery.data ?? null;
	const nodes = nodesQuery.data ?? [];
	const optimizerStats = optimizerQuery.data ?? null;
	const connectionCount = connsQuery.data?.length ?? 0;
	const serviceCount = servicesQuery.data?.length ?? 0;
	const loading = statusQuery.isFetching && !statusQuery.data;
	const error = statusQuery.errorMessage ?? nodesQuery.errorMessage;
	const fetchData = () => {
		void statusQuery.refetch();
		void nodesQuery.refetch();
		void connsQuery.refetch();
		void servicesQuery.refetch();
		void optimizerQuery.refetch();
	};

	const handleReload = async () => {
		if (!connection) return;
		setReloading(true);
		setReloadResult(null);
		try {
			const response = await triggerReload(connection);
			setReloadResult({ ok: true, text: m.admin_reload_sent({ seq: response.seq }) });
			await invalidateAdminQueries();
			fetchData();
		} catch (nextError) {
			setReloadResult({
				ok: false,
				text: m.admin_reload_failed({
					message: nextError instanceof Error ? nextError.message : String(nextError),
				}),
			});
		} finally {
			setReloading(false);
		}
	};

	if (!ready) {
		return <StateCard label={m.dashboard_restoring_session()} />;
	}

	if (!connection) {
		return <ConnectState />;
	}

	const onlineNodes = nodes.filter((node) => node.last_seen_unix_ms > 0).length;
	const restartNodes = nodes.filter((node) => node.pending_restart).length;
	const drifted = nodes.filter((node) => node.desired_revision !== node.applied_revision).length;

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow={m.dashboard_eyebrow()}
				title={m.dashboard_title()}
				description={m.dashboard_description()}
				actions={
					<>
						<CountChip>
							<div>
								<div className="text-xs font-medium text-foreground">{m.dashboard_endpoint()}</div>
								<div className="mt-0.5 max-w-56 truncate font-mono text-xs">
									{connection.baseUrl}
								</div>
							</div>
						</CountChip>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							{m.admin_auto_refresh({ state: autoRefresh ? m.admin_on() : m.admin_off() })}
						</ToggleChip>
						<RefreshButton onClick={fetchData} loading={loading} />
						<SecondaryButton onClick={handleReload} disabled={reloading}>
							<RotateCcw className={`h-4 w-4 ${reloading ? "animate-spin" : ""}`} />
							{m.admin_reload()}
						</SecondaryButton>
					</>
				}
			/>

			{reloadResult ? <ResultBanner ok={reloadResult.ok}>{reloadResult.text}</ResultBanner> : null}

			{error ? <ErrorBanner message={error} onRetry={fetchData} /> : null}

			<section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
				<MetricCard
					label={m.dashboard_registered_nodes()}
					value={status?.node_count ?? nodes.length}
					icon={<ServerCog className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.dashboard_seen_online()}
					value={onlineNodes}
					icon={<CheckCircle2 className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.dashboard_revision_drift()}
					value={drifted}
					icon={<RefreshCw className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.dashboard_pending_restart()}
					value={restartNodes}
					icon={<AlertTriangle className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.dashboard_active_connections()}
					value={connectionCount}
					icon={<Activity className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.dashboard_tunnel_services()}
					value={serviceCount}
					icon={<Unplug className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.dashboard_optimizer_savings()}
					value={
						optimizerStats?.global && optimizerStats.global.raw_bytes > 0
							? `${formatBytes(optimizerStats.global.saved_bytes)} (${formatPercentage(optimizerStats.global.saved_ratio)})${
									optimizerStats.global.net_gain_ms > 0
										? ` · net -${optimizerStats.global.net_gain_ms.toFixed(1)}ms`
										: ""
								}`
							: "0 B"
					}
					icon={<Zap className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.dashboard_state_file()}
					value={status?.state_path ?? m.common_loading()}
					icon={<ServerCog className="h-5 w-5" />}
					compact
				/>
			</section>

			<Card className="shadow-xs">
				<CardHeader className="flex flex-row items-start justify-between gap-4">
					<div>
						<CardTitle>{m.dashboard_node_fleet()}</CardTitle>
						<CardDescription className="mt-1.5">
							{m.dashboard_node_fleet_description()}
						</CardDescription>
					</div>
					<div className="flex flex-wrap gap-2">
						<Button variant="outline" size="sm" render={<Link to="/admin/traffic" />}>
							{m.nav_server_traffic()}
							<ArrowRight className="h-4 w-4" />
						</Button>
						<Button variant="outline" size="sm" render={<Link to="/admin/connectors" />}>
							{m.nav_connector_traffic()}
							<ArrowRight className="h-4 w-4" />
						</Button>
						<Button variant="outline" size="sm" render={<Link to="/admin/nodes" />}>
							{m.dashboard_open_nodes()}
							<ArrowRight className="h-4 w-4" />
						</Button>
					</div>
				</CardHeader>
				<CardContent>
					<div className="grid gap-4 xl:grid-cols-2">
						{loading && nodes.length === 0 ? (
							<StateCard label={m.dashboard_loading_inventory()} />
						) : nodes.length > 0 ? (
							nodes.slice(0, 6).map((node) => <NodeCard key={node.node_id} node={node} />)
						) : (
							<StateCard label={m.dashboard_no_workers()} />
						)}
					</div>
				</CardContent>
			</Card>
		</div>
	);
}

function NodeCard({ node }: { node: ManagedNodeSnapshot }) {
	const drifted = node.desired_revision !== node.applied_revision;
	return (
		<Link
			to="/admin/nodes/$nodeId"
			params={{ nodeId: node.node_id }}
			className="block rounded-xl border border-border bg-card p-4 shadow-xs transition-colors hover:bg-muted/40"
		>
			<div className="flex items-start justify-between gap-4">
				<div>
					<div className="text-lg font-semibold text-foreground">{node.node_id}</div>
					<div className="mt-1.5 text-sm text-muted-foreground">
						{m.admin_mode()}:{" "}
						<span className="text-foreground">{node.connection_mode ?? m.admin_unknown()}</span>
						{" · "}
						<span>{formatRelative(node.last_seen_unix_ms)}</span>
					</div>
				</div>
				<div className="flex flex-col items-end gap-2">
					<Badge tone={node.pending_restart ? "warn" : "ok"}>
						{node.pending_restart ? m.dashboard_badge_restart() : m.dashboard_badge_steady()}
					</Badge>
					{drifted ? <Badge tone="info">{m.admin_drift()}</Badge> : null}
				</div>
			</div>
			<div className="mt-4 grid gap-3 sm:grid-cols-2">
				<StatusMini label={m.dashboard_desired()} value={node.desired_revision} />
				<StatusMini label={m.dashboard_applied()} value={node.applied_revision} />
			</div>
			{node.last_apply_error ? (
				<div className="mt-3 truncate rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive">
					{node.last_apply_error}
				</div>
			) : null}
		</Link>
	);
}

function StatusMini({ label, value }: { label: string; value: string | number }) {
	return (
		<NestedPanel className="px-3 py-2.5">
			<div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
				{label}
			</div>
			<div className="mt-1 text-lg font-semibold text-foreground">{value}</div>
		</NestedPanel>
	);
}

function ConnectState() {
	return (
		<section className="flex min-h-[70vh] items-center justify-center">
			<Card className="max-w-lg text-center shadow-xs">
				<CardHeader className="items-center">
					<div className="mx-auto flex size-14 items-center justify-center rounded-xl bg-primary/10 text-primary ring-1 ring-primary/20">
						<ServerCog className="size-7" />
					</div>
					<CardTitle className="text-2xl">{m.dashboard_connect_title()}</CardTitle>
					<CardDescription>{m.dashboard_connect_description()}</CardDescription>
				</CardHeader>
				<CardContent>
					<Button render={<Link to="/login" />}>
						{m.dashboard_configure_connection()}
						<ArrowRight className="h-4 w-4" />
					</Button>
				</CardContent>
			</Card>
		</section>
	);
}
