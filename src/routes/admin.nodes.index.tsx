import { createFileRoute, Link } from "@tanstack/react-router";
import { useCallback, useMemo, useState } from "react";

import {
	Badge,
	CountChip,
	ErrorBanner,
	NestedPanel,
	PageHeader,
	RefreshButton,
	SearchInput,
	StateCard,
	ToggleChip,
} from "@/components/ui";
import { formatRelative, formatTime } from "@/lib/format";
import { getManagedNodes, type ManagedNodeSnapshot } from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/nodes/")({ component: AdminNodesIndexPage });

function AdminNodesIndexPage() {
	const { connection, ready } = usePanelSession();
	const [nodes, setNodes] = useState<ManagedNodeSnapshot[]>([]);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [query, setQuery] = useState("");
	const [autoRefresh, setAutoRefresh] = useState(true);
	const [filter, setFilter] = useState<"all" | "drift" | "restart" | "error">("all");

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

	usePolling(fetchNodes, 8_000, Boolean(connection) && autoRefresh);

	const filtered = useMemo(() => {
		const needle = query.trim().toLowerCase();
		return nodes.filter((node) => {
			if (filter === "drift" && node.desired_revision === node.applied_revision) {
				return false;
			}
			if (filter === "restart" && !node.pending_restart) {
				return false;
			}
			if (filter === "error" && !node.last_apply_error) {
				return false;
			}
			if (!needle) {
				return true;
			}
			const haystack = [
				node.node_id,
				node.connection_mode ?? "",
				node.agent_url ?? "",
				node.last_apply_error ?? "",
				...node.restart_reasons,
			]
				.join(" ")
				.toLowerCase();
			return haystack.includes(needle);
		});
	}, [filter, nodes, query]);

	if (!ready) {
		return <StateCard label={m.common_restoring_session()} />;
	}

	if (!connection) {
		return <StateCard label={m.common_connect_panel()} />;
	}

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow={m.admin_managed_nodes()}
				title={m.admin_worker_inventory()}
				description={m.admin_worker_inventory_description()}
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							{m.admin_auto_refresh({ state: autoRefresh ? m.admin_on() : m.admin_off() })}
						</ToggleChip>
						<RefreshButton onClick={fetchNodes} loading={loading} />
						<CountChip>
							{loading ? m.admin_refreshing() : m.admin_nodes_count({ count: nodes.length })}
						</CountChip>
					</>
				}
			/>

			<div className="flex flex-col gap-3 lg:flex-row lg:items-center">
				<SearchInput value={query} onChange={setQuery} placeholder={m.admin_filter_nodes()} />
				<div className="flex flex-wrap gap-2">
					{(
						[
							["all", m.admin_all()],
							["drift", m.admin_drift()],
							["restart", m.admin_restart()],
							["error", m.admin_errors()],
						] as const
					).map(([value, label]) => (
						<ToggleChip key={value} active={filter === value} onClick={() => setFilter(value)}>
							{label}
						</ToggleChip>
					))}
				</div>
			</div>

			{error ? <ErrorBanner message={error} onRetry={fetchNodes} /> : null}

			<div className="grid gap-4 xl:grid-cols-2">
				{filtered.map((node) => (
					<Link
						key={node.node_id}
						to="/admin/nodes/$nodeId"
						params={{ nodeId: node.node_id }}
						className="block rounded-xl border border-border bg-card p-4 shadow-xs transition-colors hover:bg-muted/40"
					>
						<div className="flex items-start justify-between gap-4">
							<div>
								<div className="text-xl font-semibold text-foreground">{node.node_id}</div>
								<div className="mt-1.5 text-sm text-muted-foreground">
									{m.admin_mode()}{" "}
									<span className="text-foreground">
										{node.connection_mode ?? m.admin_unknown()}
									</span>
									{node.agent_url ? (
										<>
											{" · "}
											<span className="break-all">{node.agent_url}</span>
										</>
									) : null}
								</div>
							</div>
							<div className="flex flex-col items-end gap-2">
								<Badge tone={node.pending_restart ? "warn" : "ok"}>
									{node.pending_restart ? m.admin_restart_pending() : m.admin_in_sync()}
								</Badge>
								{node.desired_revision !== node.applied_revision ? (
									<Badge tone="info">{m.admin_revision_drift()}</Badge>
								) : null}
							</div>
						</div>
						<div className="mt-4 grid gap-3 sm:grid-cols-2">
							<Value label={m.admin_desired_revision()} value={node.desired_revision} />
							<Value label={m.admin_applied_revision()} value={node.applied_revision} />
							<Value
								label={m.admin_last_seen()}
								value={`${formatRelative(node.last_seen_unix_ms)} · ${formatTime(node.last_seen_unix_ms, "short")}`}
							/>
							<Value label={m.admin_apply_error()} value={node.last_apply_error || m.admin_none()} />
						</div>
					</Link>
				))}

				{!loading && filtered.length === 0 ? (
					<StateCard
						label={nodes.length === 0 ? m.admin_no_workers() : m.admin_no_nodes_match()}
					/>
				) : null}
			</div>
		</div>
	);
}

function Value({ label, value }: { label: string; value: string | number }) {
	return (
		<NestedPanel className="px-3 py-2.5">
			<div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
				{label}
			</div>
			<div className="mt-1 break-all text-sm font-medium text-foreground">{value}</div>
		</NestedPanel>
	);
}
