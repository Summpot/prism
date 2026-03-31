import { createFileRoute, Link } from "@tanstack/react-router";
import { RefreshCcw, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { getManagedNodes, type ManagedNodeSnapshot } from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";

export const Route = createFileRoute("/nodes/")({ component: NodesIndexPage });

function NodesIndexPage() {
	const { connection, ready } = usePanelSession();
	const [nodes, setNodes] = useState<ManagedNodeSnapshot[]>([]);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const fetchNodes = useCallback(() => {
		if (!connection) {
			setNodes([]);
			return;
		}

		setLoading(true);
		setError(null);

		getManagedNodes(connection)
			.then((response) => {
				setNodes(response);
			})
			.catch((nextError) => {
				setError(nextError instanceof Error ? nextError.message : String(nextError));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	useEffect(() => {
		fetchNodes();
	}, [fetchNodes]);

	if (!ready) {
		return <StateCard label="Restoring session…" />;
	}

	if (!connection) {
		return <StateCard label="Connect the panel to a management node before browsing workers." />;
	}

	return (
		<div className="space-y-6">
			<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 md:p-8">
				<div className="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
					<div>
						<div className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">
							Managed nodes
						</div>
						<h1 className="mt-3 text-4xl font-semibold text-white">Prism worker inventory</h1>
						<p className="mt-3 max-w-3xl text-base leading-7 text-slate-400">
							Track whether nodes are syncing actively or waiting for passive access, spot revision
							drift, and jump straight into the structured config editor for any worker.
						</p>
					</div>
					<div className="flex items-center gap-3">
						<button
							type="button"
							onClick={fetchNodes}
							disabled={loading}
							className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10 disabled:opacity-50"
						>
							<RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
							Refresh
						</button>
						<div className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm text-slate-300">
							<RefreshCcw className="h-4 w-4 text-cyan-300" />
							{loading ? "Refreshing…" : `${nodes.length} nodes loaded`}
						</div>
					</div>
				</div>
			</section>

			{error ? (
				<div className="flex items-center justify-between rounded-3xl border border-red-400/20 bg-red-400/8 px-5 py-4 text-sm text-red-100">
					<span>{error}</span>
					<button
						type="button"
						onClick={fetchNodes}
						className="rounded-xl border border-red-400/30 px-3 py-1.5 text-xs font-medium transition hover:bg-red-400/15"
					>
						Retry
					</button>
				</div>
			) : null}

			<div className="grid gap-4 xl:grid-cols-2">
				{nodes.map((node) => (
					<Link
						key={node.node_id}
						to="/nodes/$nodeId"
						params={{ nodeId: node.node_id }}
						className="rounded-3xl border border-white/8 bg-slate-950/70 p-5 transition hover:border-cyan-400/25 hover:bg-cyan-400/8"
					>
						<div className="flex items-start justify-between gap-4">
							<div>
								<div className="text-xl font-semibold text-white">{node.node_id}</div>
								<div className="mt-2 text-sm text-slate-400">
									Mode <span className="text-cyan-200">{node.connection_mode ?? "unknown"}</span>
								</div>
							</div>
							<div
								className={`rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-[0.2em] ${node.pending_restart ? "bg-amber-400/12 text-amber-100" : "bg-emerald-400/12 text-emerald-100"}`}
							>
								{node.pending_restart ? "restart pending" : "in sync"}
							</div>
						</div>
						<div className="mt-5 grid gap-3 sm:grid-cols-2">
							<Value label="Desired revision" value={node.desired_revision} />
							<Value label="Applied revision" value={node.applied_revision} />
							<Value label="Last seen" value={formatTime(node.last_seen_unix_ms)} />
							<Value label="Apply error" value={node.last_apply_error || "none"} />
						</div>
					</Link>
				))}

				{!loading && nodes.length === 0 ? (
					<StateCard label="No managed workers have enrolled yet." />
				) : null}
			</div>
		</div>
	);
}

function Value({ label, value }: { label: string; value: string | number }) {
	return (
		<div className="rounded-2xl border border-white/8 bg-white/4 px-4 py-3">
			<div className="text-xs uppercase tracking-[0.2em] text-slate-500">{label}</div>
			<div className="mt-2 text-sm font-medium text-white">{value}</div>
		</div>
	);
}

function StateCard({ label }: { label: string }) {
	return (
		<div className="rounded-3xl border border-white/8 bg-slate-950/70 px-6 py-10 text-sm text-slate-400">
			{label}
		</div>
	);
}

function formatTime(value: number) {
	if (!value) {
		return "never";
	}

	return new Intl.DateTimeFormat("en-US", {
		dateStyle: "medium",
		timeStyle: "short",
	}).format(new Date(value));
}
