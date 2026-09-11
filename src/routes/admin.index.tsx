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
import { useCallback, useState } from "react";

import {
	Badge,
	ErrorBanner,
	MetricCard,
	PageHeader,
	RefreshButton,
	SecondaryButton,
	StateCard,
	ToggleChip,
} from "@/components/ui";
import { formatBytes, formatPercentage, formatRelative } from "@/lib/format";
import {
	getConnections,
	getManagedNodes,
	getManagementStatus,
	getOptimizerStats,
	getTunnelServices,
	type ManagedNodeSnapshot,
	type ManagementStatusResponse,
	type OptimizerOverviewResponse,
	triggerReload,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/")({ component: AdminDashboardPage });

function AdminDashboardPage() {
	const { connection, ready } = usePanelSession();
	const [status, setStatus] = useState<ManagementStatusResponse | null>(null);
	const [nodes, setNodes] = useState<ManagedNodeSnapshot[]>([]);
	const [optimizerStats, setOptimizerStats] = useState<OptimizerOverviewResponse | null>(null);
	const [connectionCount, setConnectionCount] = useState(0);
	const [serviceCount, setServiceCount] = useState(0);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);
	const [reloading, setReloading] = useState(false);
	const [reloadResult, setReloadResult] = useState<{ ok: boolean; text: string } | null>(null);
	const [autoRefresh, setAutoRefresh] = useState(true);

	const fetchData = useCallback(() => {
		if (!connection) {
			setStatus(null);
			setNodes([]);
			setOptimizerStats(null);
			setConnectionCount(0);
			setServiceCount(0);
			return;
		}

		setLoading(true);
		setError(null);

		Promise.all([
			getManagementStatus(connection),
			getManagedNodes(connection),
			getConnections(connection).catch(() => [] as Awaited<ReturnType<typeof getConnections>>),
			getTunnelServices(connection).catch(
				() => [] as Awaited<ReturnType<typeof getTunnelServices>>,
			),
			getOptimizerStats(connection).catch(() => null),
		])
			.then(([nextStatus, nextNodes, nextConns, nextServices, nextOptimizer]) => {
				setStatus(nextStatus);
				setNodes(nextNodes);
				setConnectionCount(nextConns.length);
				setServiceCount(nextServices.length);
				setOptimizerStats(nextOptimizer);
			})
			.catch((nextError) => {
				setError(nextError instanceof Error ? nextError.message : String(nextError));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	usePolling(fetchData, 8_000, Boolean(connection) && autoRefresh);

	const handleReload = async () => {
		if (!connection) return;
		setReloading(true);
		setReloadResult(null);
		try {
			const response = await triggerReload(connection);
			setReloadResult({ ok: true, text: m.admin_reload_sent({ seq: response.seq }) });
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
		<div className="space-y-8">
			<PageHeader
				eyebrow={m.dashboard_eyebrow()}
				title={m.dashboard_title()}
				description={m.dashboard_description()}
				actions={
					<>
						<div className="rounded-3xl border border-white/8 bg-white/4 px-5 py-4 text-sm text-slate-300">
							<div className="font-medium text-white">{m.dashboard_endpoint()}</div>
							<div className="mt-2 break-all text-cyan-200/85">{connection.baseUrl}</div>
						</div>
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

			{reloadResult ? (
				<div
					className={`rounded-3xl border px-5 py-4 text-sm ${
						reloadResult.ok
							? "border-emerald-400/20 bg-emerald-400/8 text-emerald-100"
							: "border-red-400/20 bg-red-400/8 text-red-100"
					}`}
				>
					{reloadResult.text}
				</div>
			) : null}

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

			<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 shadow-[0_24px_80px_rgba(2,6,23,0.45)] md:p-8">
				<div className="flex items-center justify-between gap-4">
					<div>
						<h2 className="text-2xl font-semibold text-white">{m.dashboard_node_fleet()}</h2>
						<p className="mt-2 text-sm leading-6 text-slate-400">
							{m.dashboard_node_fleet_description()}
						</p>
					</div>
					<Link
						to="/admin/nodes"
						className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10"
					>
						{m.dashboard_open_nodes()}
						<ArrowRight className="h-4 w-4" />
					</Link>
				</div>

				<div className="mt-6 grid gap-4 xl:grid-cols-2">
					{loading && nodes.length === 0 ? (
						<StateCard label={m.dashboard_loading_inventory()} />
					) : nodes.length > 0 ? (
						nodes.slice(0, 6).map((node) => <NodeCard key={node.node_id} node={node} />)
					) : (
						<div className="rounded-3xl border border-dashed border-white/10 bg-white/3 px-6 py-10 text-sm text-slate-400">
							{m.dashboard_no_workers()}
						</div>
					)}
				</div>
			</section>
		</div>
	);
}

function NodeCard({ node }: { node: ManagedNodeSnapshot }) {
	const drifted = node.desired_revision !== node.applied_revision;
	return (
		<Link
			to="/admin/nodes/$nodeId"
			params={{ nodeId: node.node_id }}
			className="rounded-3xl border border-white/8 bg-white/4 p-5 transition hover:border-cyan-400/25 hover:bg-cyan-400/8"
		>
			<div className="flex items-start justify-between gap-4">
				<div>
					<div className="text-lg font-semibold text-white">{node.node_id}</div>
					<div className="mt-2 text-sm text-slate-400">
						{m.admin_mode()}:{" "}
						<span className="text-cyan-200">{node.connection_mode ?? m.admin_unknown()}</span>
						{" · "}
						<span className="text-slate-300">{formatRelative(node.last_seen_unix_ms)}</span>
					</div>
				</div>
				<div className="flex flex-col items-end gap-2">
					<Badge tone={node.pending_restart ? "warn" : "ok"}>
						{node.pending_restart
							? m.dashboard_badge_restart()
							: m.dashboard_badge_steady()}
					</Badge>
					{drifted ? <Badge tone="info">{m.admin_drift()}</Badge> : null}
				</div>
			</div>
			<div className="mt-5 grid gap-3 sm:grid-cols-2">
				<StatusMini label={m.dashboard_desired()} value={node.desired_revision} />
				<StatusMini label={m.dashboard_applied()} value={node.applied_revision} />
			</div>
			{node.last_apply_error ? (
				<div className="mt-4 truncate rounded-2xl border border-red-400/20 bg-red-400/8 px-3 py-2 text-xs text-red-100">
					{node.last_apply_error}
				</div>
			) : null}
		</Link>
	);
}

function StatusMini({ label, value }: { label: string; value: string | number }) {
	return (
		<div className="rounded-2xl border border-white/8 bg-slate-950/70 px-4 py-3">
			<div className="text-xs uppercase tracking-[0.2em] text-slate-500">{label}</div>
			<div className="mt-2 text-xl font-semibold text-white">{value}</div>
		</div>
	);
}

function ConnectState() {
	return (
		<section className="flex min-h-[70vh] items-center justify-center">
			<div className="max-w-2xl rounded-[2rem] border border-white/8 bg-slate-950/70 px-8 py-10 text-center shadow-[0_24px_80px_rgba(2,6,23,0.45)]">
				<div className="mx-auto flex h-16 w-16 items-center justify-center rounded-3xl border border-cyan-400/30 bg-cyan-400/10 text-cyan-300">
					<ServerCog className="h-8 w-8" />
				</div>
				<h1 className="mt-6 text-3xl font-semibold text-white">{m.dashboard_connect_title()}</h1>
				<p className="mt-4 text-base leading-7 text-slate-400">
					{m.dashboard_connect_description()}
				</p>
				<Link
					to="/login"
					className="mt-8 inline-flex items-center gap-3 rounded-2xl bg-cyan-400 px-5 py-3 text-sm font-semibold text-slate-950 transition hover:bg-cyan-300"
				>
					{m.dashboard_configure_connection()}
					<ArrowRight className="h-4 w-4" />
				</Link>
			</div>
		</section>
	);
}
