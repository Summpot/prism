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
import { formatRelative } from "@/lib/format";
import {
	getConnections,
	getManagedNodes,
	getManagementStatus,
	getMetrics,
	getTunnelServices,
	type ManagedNodeSnapshot,
	type ManagementStatusResponse,
	type MetricsSnapshot,
	triggerReload,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";

export const Route = createFileRoute("/")({ component: DashboardPage });

function DashboardPage() {
	const { connection, ready } = usePanelSession();
	const [status, setStatus] = useState<ManagementStatusResponse | null>(null);
	const [nodes, setNodes] = useState<ManagedNodeSnapshot[]>([]);
	const [connectionCount, setConnectionCount] = useState(0);
	const [serviceCount, setServiceCount] = useState(0);
	const [metrics, setMetrics] = useState<MetricsSnapshot | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);
	const [reloading, setReloading] = useState(false);
	const [reloadResult, setReloadResult] = useState<string | null>(null);
	const [autoRefresh, setAutoRefresh] = useState(true);

	const fetchData = useCallback(() => {
		if (!connection) {
			setStatus(null);
			setNodes([]);
			setConnectionCount(0);
			setServiceCount(0);
			setMetrics(null);
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
			getMetrics(connection).catch(() => null),
		])
			.then(([nextStatus, nextNodes, nextConns, nextServices, nextMetrics]) => {
				setStatus(nextStatus);
				setNodes(nextNodes);
				setConnectionCount(nextConns.length);
				setServiceCount(nextServices.length);
				setMetrics(nextMetrics);
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
			setReloadResult(`Reload signal sent (seq ${response.seq})`);
			fetchData();
		} catch (nextError) {
			setReloadResult(
				`Reload failed: ${nextError instanceof Error ? nextError.message : String(nextError)}`,
			);
		} finally {
			setReloading(false);
		}
	};

	if (!ready) {
		return <StateCard label="Restoring Prism panel session…" />;
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
				eyebrow="Prism management node"
				title="Operational visibility for every Prism worker."
				description="Source of truth for managed workers, live proxy sessions, tunnel registrations, and local runtime counters."
				actions={
					<>
						<div className="rounded-3xl border border-white/8 bg-white/4 px-5 py-4 text-sm text-slate-300">
							<div className="font-medium text-white">Connected endpoint</div>
							<div className="mt-2 break-all text-cyan-200/85">{connection.baseUrl}</div>
						</div>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							Auto-refresh {autoRefresh ? "on" : "off"}
						</ToggleChip>
						<RefreshButton onClick={fetchData} loading={loading} />
						<SecondaryButton onClick={handleReload} disabled={reloading}>
							<RotateCcw className={`h-4 w-4 ${reloading ? "animate-spin" : ""}`} />
							Reload config
						</SecondaryButton>
					</>
				}
			/>

			{reloadResult ? (
				<div
					className={`rounded-3xl border px-5 py-4 text-sm ${
						reloadResult.startsWith("Reload failed")
							? "border-red-400/20 bg-red-400/8 text-red-100"
							: "border-emerald-400/20 bg-emerald-400/8 text-emerald-100"
					}`}
				>
					{reloadResult}
				</div>
			) : null}

			{error ? <ErrorBanner message={error} onRetry={fetchData} /> : null}

			<section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
				<MetricCard
					label="Registered nodes"
					value={status?.node_count ?? nodes.length}
					icon={<ServerCog className="h-5 w-5" />}
				/>
				<MetricCard
					label="Seen online"
					value={onlineNodes}
					icon={<CheckCircle2 className="h-5 w-5" />}
				/>
				<MetricCard
					label="Revision drift"
					value={drifted}
					icon={<RefreshCw className="h-5 w-5" />}
				/>
				<MetricCard
					label="Pending restart"
					value={restartNodes}
					icon={<AlertTriangle className="h-5 w-5" />}
				/>
				<MetricCard
					label="Active connections"
					value={connectionCount}
					icon={<Activity className="h-5 w-5" />}
				/>
				<MetricCard
					label="Tunnel services"
					value={serviceCount}
					icon={<Unplug className="h-5 w-5" />}
				/>
				<MetricCard
					label="Ingress bytes"
					value={metrics ? formatCompactBytes(metrics.bytes_ingress_total) : "n/a"}
					icon={<Activity className="h-5 w-5" />}
				/>
				<MetricCard
					label="State file"
					value={status?.state_path ?? "Loading…"}
					icon={<ServerCog className="h-5 w-5" />}
					compact
				/>
			</section>

			<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 shadow-[0_24px_80px_rgba(2,6,23,0.45)] md:p-8">
				<div className="flex items-center justify-between gap-4">
					<div>
						<h2 className="text-2xl font-semibold text-white">Node fleet</h2>
						<p className="mt-2 text-sm leading-6 text-slate-400">
							Worker health, revision drift, and restart pressure across managed workers.
						</p>
					</div>
					<Link
						to="/nodes"
						className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10"
					>
						Open nodes
						<ArrowRight className="h-4 w-4" />
					</Link>
				</div>

				<div className="mt-6 grid gap-4 xl:grid-cols-2">
					{loading && nodes.length === 0 ? (
						<StateCard label="Loading management inventory…" />
					) : nodes.length > 0 ? (
						nodes.slice(0, 6).map((node) => <NodeCard key={node.node_id} node={node} />)
					) : (
						<div className="rounded-3xl border border-dashed border-white/10 bg-white/3 px-6 py-10 text-sm text-slate-400">
							No workers have enrolled yet. Connect one through the worker bootstrap config, then
							return here.
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
			to="/nodes/$nodeId"
			params={{ nodeId: node.node_id }}
			className="rounded-3xl border border-white/8 bg-white/4 p-5 transition hover:border-cyan-400/25 hover:bg-cyan-400/8"
		>
			<div className="flex items-start justify-between gap-4">
				<div>
					<div className="text-lg font-semibold text-white">{node.node_id}</div>
					<div className="mt-2 text-sm text-slate-400">
						Mode: <span className="text-cyan-200">{node.connection_mode ?? "unknown"}</span>
						{" · "}
						<span className="text-slate-300">{formatRelative(node.last_seen_unix_ms)}</span>
					</div>
				</div>
				<div className="flex flex-col items-end gap-2">
					<Badge tone={node.pending_restart ? "warn" : "ok"}>
						{node.pending_restart ? "restart" : "steady"}
					</Badge>
					{drifted ? <Badge tone="info">drift</Badge> : null}
				</div>
			</div>
			<div className="mt-5 grid gap-3 sm:grid-cols-2">
				<StatusMini label="Desired" value={node.desired_revision} />
				<StatusMini label="Applied" value={node.applied_revision} />
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
				<h1 className="mt-6 text-3xl font-semibold text-white">Connect to a management node</h1>
				<p className="mt-4 text-base leading-7 text-slate-400">
					The panel authenticates against the management API with a base URL and panel bearer token
					before it can render inventory or edit managed revisions.
				</p>
				<Link
					to="/login"
					className="mt-8 inline-flex items-center gap-3 rounded-2xl bg-cyan-400 px-5 py-3 text-sm font-semibold text-slate-950 transition hover:bg-cyan-300"
				>
					Configure connection
					<ArrowRight className="h-4 w-4" />
				</Link>
			</div>
		</section>
	);
}

function formatCompactBytes(value: number) {
	if (value < 1024) return `${value} B`;
	if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
	if (value < 1024 * 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MB`;
	return `${(value / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
