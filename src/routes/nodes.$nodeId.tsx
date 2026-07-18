import { createFileRoute, Link } from "@tanstack/react-router";
import { AlertTriangle, ArrowLeft, CircleSlash, RefreshCcw, ServerCog } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { ManagedConfigEditor } from "@/components/ManagedConfigEditor";
import {
	Badge,
	ErrorBanner,
	MetricCard,
	PageHeader,
	RefreshButton,
	StateCard,
} from "@/components/ui";
import { formatRelative, formatTime } from "@/lib/format";
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

	if (loading && !data) {
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
				<ErrorBanner message={error || "Node not found."} onRetry={fetchData} />
			</div>
		);
	}

	const { node } = data;
	const drifted = node.desired_revision !== node.applied_revision;

	return (
		<div className="space-y-6">
			<Link
				to="/nodes"
				className="inline-flex items-center gap-2 text-sm text-slate-400 transition hover:text-white"
			>
				<ArrowLeft className="h-4 w-4" />
				Back to nodes
			</Link>

			<PageHeader
				eyebrow="Node detail"
				title={node.node_id}
				description="Review desired versus applied revisions, inspect restart pressure, and edit the next structured config revision for this worker."
				actions={
					<>
						{node.pending_restart ? (
							<Badge tone="warn">restart required</Badge>
						) : (
							<Badge tone="ok">steady</Badge>
						)}
						{drifted ? <Badge tone="info">revision drift</Badge> : null}
						<RefreshButton onClick={fetchData} loading={loading} />
					</>
				}
			/>

			<section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
				<MetricCard
					label="Connection mode"
					value={node.connection_mode ?? "unknown"}
					icon={<ServerCog className="h-5 w-5" />}
				/>
				<MetricCard
					label="Desired revision"
					value={node.desired_revision}
					icon={<RefreshCcw className="h-5 w-5" />}
				/>
				<MetricCard
					label="Applied revision"
					value={node.applied_revision}
					icon={<CircleSlash className="h-5 w-5" />}
				/>
				<MetricCard
					label="Last seen"
					value={`${formatRelative(node.last_seen_unix_ms)} · ${formatTime(node.last_seen_unix_ms, "short")}`}
					icon={<AlertTriangle className="h-5 w-5" />}
					compact
				/>
			</section>

			{(node.agent_url || node.last_apply_attempt_unix_ms || node.last_apply_success_unix_ms) && (
				<section className="grid gap-4 md:grid-cols-3">
					{node.agent_url ? (
						<div className="rounded-3xl border border-white/8 bg-slate-950/70 p-5 text-sm">
							<div className="text-xs uppercase tracking-[0.2em] text-slate-500">Agent URL</div>
							<div className="mt-2 break-all text-white">{node.agent_url}</div>
						</div>
					) : null}
					<div className="rounded-3xl border border-white/8 bg-slate-950/70 p-5 text-sm">
						<div className="text-xs uppercase tracking-[0.2em] text-slate-500">
							Last apply attempt
						</div>
						<div className="mt-2 text-white">
							{formatTime(node.last_apply_attempt_unix_ms, "short")}
						</div>
					</div>
					<div className="rounded-3xl border border-white/8 bg-slate-950/70 p-5 text-sm">
						<div className="text-xs uppercase tracking-[0.2em] text-slate-500">
							Last apply success
						</div>
						<div className="mt-2 text-white">
							{formatTime(node.last_apply_success_unix_ms, "short")}
						</div>
					</div>
				</section>
			)}

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
