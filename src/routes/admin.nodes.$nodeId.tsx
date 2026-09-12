import { createFileRoute, Link } from "@tanstack/react-router";
import { AlertTriangle, ArrowLeft, CircleSlash, RefreshCcw, ServerCog } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { NodeConfigEditor } from "@/components/cluster/NodeConfigEditor";
import {
	Badge,
	ErrorBanner,
	InfoValue,
	MetricCard,
	PageHeader,
	RefreshButton,
	StateCard,
	WarningBanner,
} from "@/components/ui";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { formatRelative, formatTime } from "@/lib/format";
import {
	getManagedNodeConfig,
	type ManagedConfigDocument,
	type ManagedNodeConfigResponse,
	updateManagedNodeConfig,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/nodes/$nodeId")({
	component: AdminNodeDetailPage,
});

function AdminNodeDetailPage() {
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
		return <StateCard label={m.common_restoring_session()} />;
	}

	if (!connection) {
		return <StateCard label={m.nodetail_connect_panel()} />;
	}

	if (loading && !data) {
		return <StateCard label={m.nodetail_loading()} />;
	}

	if (error || !data) {
		return (
			<div className="space-y-4">
				<Button variant="ghost" size="sm" render={<Link to="/admin/nodes" />}>
					<ArrowLeft className="h-4 w-4" />
					{m.nodetail_back()}
				</Button>
				<ErrorBanner message={error || m.nodetail_not_found()} onRetry={fetchData} />
			</div>
		);
	}

	const { node } = data;
	const drifted = node.desired_revision !== node.applied_revision;

	return (
		<div className="space-y-6">
			<Button variant="ghost" size="sm" render={<Link to="/admin/nodes" />}>
				<ArrowLeft className="h-4 w-4" />
				{m.nodetail_back()}
			</Button>

			<PageHeader
				eyebrow={m.nodetail_eyebrow()}
				title={node.node_id}
				description={m.nodetail_description()}
				actions={
					<>
						{node.pending_restart ? (
							<Badge tone="warn">{m.nodetail_restart_required()}</Badge>
						) : (
							<Badge tone="ok">{m.nodetail_steady()}</Badge>
						)}
						{drifted ? <Badge tone="info">{m.nodetail_revision_drift()}</Badge> : null}
						<RefreshButton onClick={fetchData} loading={loading} />
					</>
				}
			/>

			<section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
				<MetricCard
					label={m.nodetail_connection_mode()}
					value={node.connection_mode ?? m.admin_unknown()}
					icon={<ServerCog className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.admin_desired_revision()}
					value={node.desired_revision}
					icon={<RefreshCcw className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.admin_applied_revision()}
					value={node.applied_revision}
					icon={<CircleSlash className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.admin_last_seen()}
					value={`${formatRelative(node.last_seen_unix_ms)} · ${formatTime(node.last_seen_unix_ms, "short")}`}
					icon={<AlertTriangle className="h-5 w-5" />}
					compact
				/>
			</section>

			{(node.agent_url || node.last_apply_attempt_unix_ms || node.last_apply_success_unix_ms) && (
				<section className="grid gap-4 md:grid-cols-3">
					{node.agent_url ? (
						<InfoValue label={m.nodetail_agent_url()} value={node.agent_url} />
					) : null}
					<InfoValue
						label={m.nodetail_last_apply_attempt()}
						value={formatTime(node.last_apply_attempt_unix_ms, "short")}
					/>
					<InfoValue
						label={m.nodetail_last_apply_success()}
						value={formatTime(node.last_apply_success_unix_ms, "short")}
					/>
				</section>
			)}

			{node.restart_reasons.length > 0 ? (
				<WarningBanner title={m.nodetail_restart_reasons()}>
					<ul className="mt-2 list-disc space-y-1 pl-5">
						{node.restart_reasons.map((reason) => (
							<li key={reason}>{reason}</li>
						))}
					</ul>
				</WarningBanner>
			) : null}

			{node.last_apply_error ? (
				<Alert variant="destructive">
					<AlertTriangle />
					<AlertTitle>{m.nodetail_last_apply_error()}</AlertTitle>
					<AlertDescription className="whitespace-pre-wrap break-all">
						{node.last_apply_error}
					</AlertDescription>
				</Alert>
			) : null}

			<NodeConfigEditor
				initialConfig={data.desired_config ?? undefined}
				isSaving={saving}
				saveError={saveError}
				onSave={saveConfig}
			/>
		</div>
	);
}
