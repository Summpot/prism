import { createFileRoute, Link } from "@tanstack/react-router";
import {
	Activity,
	ArrowRight,
	FileCode,
	Network,
	Radio,
	RotateCcw,
	Server,
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
import { formatBytes, formatPercentage } from "@/lib/format";
import {
	getConfigPath,
	getConnections,
	getOptimizerStats,
	getTunnelServices,
	triggerReload,
} from "@/lib/admin/adminApi";
import { useAdminQuery } from "@/hooks/useAdminQuery";
import { usePanelSession } from "@/lib/panelSession";
import { invalidateAdminQueries } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import { m } from "@/paraglide/messages";
import type { ServiceSnapshot } from "@/types/admin";

export const Route = createFileRoute("/admin/")({ component: AdminDashboardPage });

function AdminDashboardPage() {
	const { connection, ready } = usePanelSession();
	const [reloading, setReloading] = useState(false);
	const [reloadResult, setReloadResult] = useState<{ ok: boolean; text: string } | null>(null);
	const [autoRefresh, setAutoRefresh] = useState(true);
	const interval = autoRefresh ? 8_000 : false;

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
	const configPathQuery = useAdminQuery(
		queryKeys.admin.configPath(connection),
		(conn) => getConfigPath(conn).catch(() => null),
		{ refetchInterval: interval },
	);

	const optimizerStats = optimizerQuery.data ?? null;
	const connectionCount = connsQuery.data?.length ?? 0;
	const serviceCount = servicesQuery.data?.length ?? 0;
	const services = servicesQuery.data ?? [];
	const configPath = configPathQuery.data?.path ?? null;

	const loading = connsQuery.isFetching && !connsQuery.data;
	const error = connsQuery.errorMessage ?? servicesQuery.errorMessage ?? null;

	const fetchData = () => {
		void connsQuery.refetch();
		void servicesQuery.refetch();
		void optimizerQuery.refetch();
		void configPathQuery.refetch();
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
					label={m.dashboard_config_file()}
					value={configPath ?? m.common_loading()}
					icon={<FileCode className="h-5 w-5" />}
					compact
				/>
			</section>

			<Card className="shadow-xs">
				<CardHeader className="flex flex-row items-start justify-between gap-4">
					<div>
						<CardTitle>{m.dashboard_gateway_overview()}</CardTitle>
						<CardDescription className="mt-1.5">
							{m.dashboard_gateway_overview_description()}
						</CardDescription>
					</div>
					<div className="flex flex-wrap gap-2">
						<Button variant="outline" size="sm" render={<Link to="/admin/topology" />}>
							<Network className="h-4 w-4" />
							{m.topology_view_button()}
						</Button>
						<Button variant="outline" size="sm" render={<Link to="/admin/connections" />}>
							<Activity className="h-4 w-4" />
							{m.nav_connections()}
						</Button>
						<Button variant="outline" size="sm" render={<Link to="/admin/tunnel-services" />}>
							<Unplug className="h-4 w-4" />
							{m.nav_services()}
						</Button>
						<Button variant="outline" size="sm" render={<Link to="/admin/traffic" />}>
							<Server className="h-4 w-4" />
							{m.nav_server_traffic()}
						</Button>
						<Button variant="outline" size="sm" render={<Link to="/admin/connectors" />}>
							<Radio className="h-4 w-4" />
							{m.nav_connector_traffic()}
						</Button>
					</div>
				</CardHeader>
				<CardContent>
					<div className="grid gap-4 xl:grid-cols-2">
						{loading && services.length === 0 ? (
							<StateCard label={m.common_loading()} />
						) : services.length > 0 ? (
							services
								.slice(0, 6)
								.map((item) => <ServiceCard key={item.service.name} item={item} />)
						) : (
							<StateCard label={m.dashboard_no_services()} />
						)}
					</div>
				</CardContent>
			</Card>
		</div>
	);
}

function ServiceCard({ item }: { item: ServiceSnapshot }) {
	const svc = item.service;
	return (
		<div className="block rounded-xl border border-border bg-card p-4 shadow-xs transition-colors hover:bg-muted/40">
			<div className="flex items-start justify-between gap-4">
				<div>
					<div className="text-lg font-semibold text-foreground">{svc.name}</div>
					<div className="mt-1.5 text-sm text-muted-foreground">
						<span>{svc.proto.toUpperCase()}</span>
						{" · "}
						<span>{svc.local_addr}</span>
						{svc.remote_addr ? (
							<>
								{" -> "}
								<span>{svc.remote_addr}</span>
							</>
						) : null}
					</div>
				</div>
				<div className="flex flex-col items-end gap-2">
					<Badge tone={item.primary ? "ok" : "neutral"}>
						{item.primary ? "Primary" : "Secondary"}
					</Badge>
					{svc.route_only ? <Badge tone="info">Route Only</Badge> : null}
				</div>
			</div>
			<div className="mt-4 grid gap-3 sm:grid-cols-2">
				<NestedPanel className="px-3 py-2.5">
					<div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
						Client ID
					</div>
					<div className="mt-1 truncate font-mono text-sm font-semibold text-foreground">
						{item.client_id || "—"}
					</div>
				</NestedPanel>
				<NestedPanel className="px-3 py-2.5">
					<div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
						Remote Peer
					</div>
					<div className="mt-1 truncate font-mono text-sm font-semibold text-foreground">
						{item.remote || "—"}
					</div>
				</NestedPanel>
			</div>
		</div>
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
