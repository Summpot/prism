import { createFileRoute, Link } from "@tanstack/react-router";
import {
	AlertTriangle,
	ArrowRight,
	CheckCircle2,
	RefreshCw,
	ServerCog,
} from "lucide-react";
import { useEffect, useState } from "react";

import {
	getManagedNodes,
	getManagementStatus,
	type ManagedNodeSnapshot,
	type ManagementStatusResponse,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";

export const Route = createFileRoute("/")({ component: DashboardPage });

function DashboardPage() {
	const { connection, ready } = usePanelSession();
	const [status, setStatus] = useState<ManagementStatusResponse | null>(null);
	const [nodes, setNodes] = useState<ManagedNodeSnapshot[]>([]);
	const [error, setError] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);

	useEffect(() => {
		if (!connection) {
			setStatus(null);
			setNodes([]);
			return;
		}

		let cancelled = false;
		setLoading(true);
		setError(null);

		Promise.all([getManagementStatus(connection), getManagedNodes(connection)])
			.then(([nextStatus, nextNodes]) => {
				if (cancelled) {
					return;
				}

				setStatus(nextStatus);
				setNodes(nextNodes);
			})
			.catch((nextError) => {
				if (!cancelled) {
					setError(
						nextError instanceof Error ? nextError.message : String(nextError),
					);
				}
			})
			.finally(() => {
				if (!cancelled) {
					setLoading(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, [connection]);

	if (!ready) {
		return <LoadingState label="Restoring Prism panel session…" />;
	}

	if (!connection) {
		return <ConnectState />;
	}

	const onlineNodes = nodes.filter((node) => node.last_seen_unix_ms > 0).length;
	const restartNodes = nodes.filter((node) => node.pending_restart).length;

	return (
		<div className="space-y-8">
			<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 px-6 py-8 shadow-[0_24px_80px_rgba(2,6,23,0.45)] md:px-8">
				<div className="flex flex-col gap-6 xl:flex-row xl:items-end xl:justify-between">
					<div className="max-w-4xl">
						<div className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">
							Prism management node
						</div>
						<h1 className="mt-3 text-4xl font-semibold tracking-tight text-white md:text-5xl">
							Operational visibility for every Prism worker.
						</h1>
						<p className="mt-4 max-w-2xl text-base leading-7 text-slate-400 md:text-lg">
							This panel treats the management node as the source of truth,
							shows active versus passive worker connectivity, and lets you edit
							managed listener and route revisions visually instead of
							hand-editing files.
						</p>
					</div>

					<div className="rounded-3xl border border-white/8 bg-white/4 px-5 py-4 text-sm text-slate-300">
						<div className="font-medium text-white">Connected endpoint</div>
						<div className="mt-2 break-all text-cyan-200/85">
							{connection.baseUrl}
						</div>
					</div>
				</div>
			</section>

			{error ? (
				<div className="rounded-3xl border border-red-400/20 bg-red-400/8 px-5 py-4 text-sm text-red-100">
					{error}
				</div>
			) : null}

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
					label="Pending restart"
					value={restartNodes}
					icon={<AlertTriangle className="h-5 w-5" />}
				/>
				<MetricCard
					label="State file"
					value={status?.state_path ?? "Loading…"}
					icon={<RefreshCw className="h-5 w-5" />}
					compact
				/>
			</section>

			<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 shadow-[0_24px_80px_rgba(2,6,23,0.45)] md:p-8">
				<div className="flex items-center justify-between gap-4">
					<div>
						<h2 className="text-2xl font-semibold text-white">Node fleet</h2>
						<p className="mt-2 text-sm leading-6 text-slate-400">
							Worker health, revision drift, and restart pressure across your
							managed Prism estate.
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
					{loading ? (
						<LoadingState label="Loading management inventory…" />
					) : nodes.length > 0 ? (
						nodes
							.slice(0, 4)
							.map((node) => <NodeCard key={node.node_id} node={node} />)
					) : (
						<div className="rounded-3xl border border-dashed border-white/10 bg-white/3 px-6 py-10 text-sm text-slate-400">
							No workers have enrolled yet. Connect one through the worker
							bootstrap config, then return here.
						</div>
					)}
				</div>
			</section>
		</div>
	);
}

function MetricCard({
	label,
	value,
	icon,
	compact = false,
}: {
	label: string;
	value: string | number;
	icon: React.ReactNode;
	compact?: boolean;
}) {
	return (
		<div className="rounded-3xl border border-white/8 bg-slate-950/70 p-5 shadow-[0_18px_70px_rgba(2,6,23,0.35)]">
			<div className="flex items-center justify-between text-slate-400">
				<span className="text-sm uppercase tracking-[0.2em]">{label}</span>
				<div className="text-cyan-300">{icon}</div>
			</div>
			<div
				className={`mt-4 ${compact ? "break-all text-sm text-white" : "text-3xl font-semibold text-white"}`}
			>
				{value}
			</div>
		</div>
	);
}

function NodeCard({ node }: { node: ManagedNodeSnapshot }) {
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
						Mode:{" "}
						<span className="text-cyan-200">
							{node.connection_mode ?? "unknown"}
						</span>
					</div>
				</div>
				<div
					className={`rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-[0.2em] ${node.pending_restart ? "bg-amber-400/12 text-amber-100" : "bg-emerald-400/12 text-emerald-100"}`}
				>
					{node.pending_restart ? "restart" : "steady"}
				</div>
			</div>
			<div className="mt-5 grid gap-3 sm:grid-cols-2">
				<StatusMini label="Desired" value={node.desired_revision} />
				<StatusMini label="Applied" value={node.applied_revision} />
			</div>
		</Link>
	);
}

function StatusMini({
	label,
	value,
}: {
	label: string;
	value: string | number;
}) {
	return (
		<div className="rounded-2xl border border-white/8 bg-slate-950/70 px-4 py-3">
			<div className="text-xs uppercase tracking-[0.2em] text-slate-500">
				{label}
			</div>
			<div className="mt-2 text-xl font-semibold text-white">{value}</div>
		</div>
	);
}

function LoadingState({ label }: { label: string }) {
	return (
		<div className="rounded-3xl border border-white/8 bg-slate-950/70 px-6 py-10 text-sm text-slate-400">
			{label}
		</div>
	);
}

function ConnectState() {
	return (
		<section className="flex min-h-[70vh] items-center justify-center">
			<div className="max-w-2xl rounded-[2rem] border border-white/8 bg-slate-950/70 px-8 py-10 text-center shadow-[0_24px_80px_rgba(2,6,23,0.45)]">
				<div className="mx-auto flex h-16 w-16 items-center justify-center rounded-3xl border border-cyan-400/30 bg-cyan-400/10 text-cyan-300">
					<PlugPanel />
				</div>
				<h1 className="mt-6 text-3xl font-semibold text-white">
					Connect to a management node
				</h1>
				<p className="mt-4 text-base leading-7 text-slate-400">
					The panel is deployable on its own, so it needs a management API base
					URL and a panel bearer token before it can render node inventory or
					edit managed revisions.
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

function PlugPanel() {
	return <ServerCog className="h-8 w-8" />;
}
