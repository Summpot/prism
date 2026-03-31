import { createFileRoute, Link } from "@tanstack/react-router";
import {
	AlertTriangle,
	ArrowLeft,
	CircleSlash,
	RefreshCcw,
	RefreshCw,
	ServerCog,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { ManagedConfigEditor } from "@/components/ManagedConfigEditor";
import {
	getManagedNodeConfig,
	type ManagedConfigDocument,
	type ManagedNodeConfigResponse,
	updateManagedNodeConfig,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";

export const Route = createFileRoute("/nodes/$nodeId")({
	component: NodeDetailPage,
});

function NodeDetailPage() {
	const params = Route.useParams();
	const { connection, ready } = usePanelSession();
	const [data, setData] = useState<ManagedNodeConfigResponse | null>(null);
	const [loading, setLoading] = useState(false);
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [saveError, setSaveError] = useState<string | null>(null);

	const fetchData = useCallback(() => {
		if (!connection) {
			setData(null);
			return;
		}

		setLoading(true);
		setError(null);

		getManagedNodeConfig(connection, params.nodeId)
			.then((response) => {
				setData(response);
			})
			.catch((nextError) => {
				setError(nextError instanceof Error ? nextError.message : String(nextError));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection, params.nodeId]);

	useEffect(() => {
		fetchData();
	}, [fetchData]);

	const saveConfig = async (desiredConfig: ManagedConfigDocument) => {
		if (!connection) {
			return;
		}

		setSaving(true);
		setSaveError(null);
		try {
			const response = await updateManagedNodeConfig(connection, params.nodeId, desiredConfig);
			setData(response);
		} catch (nextError) {
			setSaveError(nextError instanceof Error ? nextError.message : String(nextError));
		} finally {
			setSaving(false);
		}
	};

	if (!ready) {
		return <StateCard label="Restoring session…" />;
	}

	if (!connection) {
		return <StateCard label="Connect the panel to a management node before opening node detail." />;
	}

	if (loading) {
		return <StateCard label="Loading node detail…" />;
	}

	if (error || !data) {
		return (
			<div className="space-y-4">
				<Link
					to="/nodes"
					className="inline-flex items-center gap-2 text-sm text-slate-400 transition hover:text-white"
				>
					<ArrowLeft className="h-4 w-4" />
					Back to nodes
				</Link>
				<div className="flex items-center justify-between rounded-3xl border border-red-400/20 bg-red-400/8 px-5 py-4 text-sm text-red-100">
					<span>{error || "Node not found."}</span>
					<button
						type="button"
						onClick={fetchData}
						className="rounded-xl border border-red-400/30 px-3 py-1.5 text-xs font-medium transition hover:bg-red-400/15"
					>
						Retry
					</button>
				</div>
			</div>
		);
	}

	const { node } = data;

	return (
		<div className="space-y-6">
			<Link
				to="/nodes"
				className="inline-flex items-center gap-2 text-sm text-slate-400 transition hover:text-white"
			>
				<ArrowLeft className="h-4 w-4" />
				Back to nodes
			</Link>

			<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 md:p-8">
				<div className="flex flex-col gap-5 xl:flex-row xl:items-start xl:justify-between">
					<div>
						<div className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">
							Node detail
						</div>
						<h1 className="mt-3 text-4xl font-semibold text-white">{node.node_id}</h1>
						<p className="mt-3 max-w-3xl text-base leading-7 text-slate-400">
							Review desired versus applied revisions, inspect restart pressure, and edit the next
							structured config revision for this worker.
						</p>
					</div>

					<div className="flex flex-col items-end gap-3">
						<div
							className={`rounded-3xl border px-4 py-3 text-sm ${node.pending_restart ? "border-amber-400/25 bg-amber-400/8 text-amber-100" : "border-emerald-400/25 bg-emerald-400/8 text-emerald-100"}`}
						>
							{node.pending_restart
								? "Restart required before this worker is fully converged."
								: "Worker has no restart-required drift."}
						</div>
						<button
							type="button"
							onClick={fetchData}
							disabled={loading}
							className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10 disabled:opacity-50"
						>
							<RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
							Refresh
						</button>
					</div>
				</div>
			</section>

			<section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
				<InfoCard
					label="Connection mode"
					value={node.connection_mode ?? "unknown"}
					icon={<ServerCog className="h-5 w-5" />}
				/>
				<InfoCard
					label="Desired revision"
					value={node.desired_revision}
					icon={<RefreshCcw className="h-5 w-5" />}
				/>
				<InfoCard
					label="Applied revision"
					value={node.applied_revision}
					icon={<CircleSlash className="h-5 w-5" />}
				/>
				<InfoCard
					label="Last seen"
					value={formatTime(node.last_seen_unix_ms)}
					icon={<AlertTriangle className="h-5 w-5" />}
					compact
				/>
			</section>

			{node.restart_reasons.length > 0 ? (
				<section className="rounded-3xl border border-amber-400/20 bg-amber-400/8 p-5 text-sm text-amber-100">
					<div className="font-semibold">Restart reasons</div>
					<ul className="mt-3 list-disc space-y-2 pl-5">
						{node.restart_reasons.map((reason) => (
							<li key={reason}>{reason}</li>
						))}
					</ul>
				</section>
			) : null}

			{node.last_apply_error ? (
				<section className="rounded-3xl border border-red-400/20 bg-red-400/8 p-5 text-sm text-red-100">
					<div className="font-semibold">Last apply error</div>
					<div className="mt-2 whitespace-pre-wrap break-all">{node.last_apply_error}</div>
				</section>
			) : null}

			<ManagedConfigEditor
				initialConfig={data.desired_config ?? undefined}
				isSaving={saving}
				saveError={saveError}
				onSave={saveConfig}
			/>
		</div>
	);
}

function InfoCard({
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
		<div className="rounded-3xl border border-white/8 bg-slate-950/70 p-5">
			<div className="flex items-center justify-between text-slate-400">
				<span className="text-xs uppercase tracking-[0.2em]">{label}</span>
				<div className="text-cyan-300">{icon}</div>
			</div>
			<div
				className={`mt-4 ${compact ? "text-sm text-white" : "text-3xl font-semibold text-white"}`}
			>
				{value}
			</div>
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
