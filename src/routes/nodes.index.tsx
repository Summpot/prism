import { createFileRoute, Link } from "@tanstack/react-router";
import { useCallback, useMemo, useState } from "react";

import {
	Badge,
	ErrorBanner,
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

export const Route = createFileRoute("/nodes/")({ component: NodesIndexPage });

function NodesIndexPage() {
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
		return <StateCard label="Restoring session…" />;
	}

	if (!connection) {
		return <StateCard label="Connect the panel to a management node before browsing workers." />;
	}

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow="Managed nodes"
				title="Prism worker inventory"
				description="Track active versus passive connectivity, revision drift, restart pressure, and jump into the structured config editor."
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							Auto-refresh {autoRefresh ? "on" : "off"}
						</ToggleChip>
						<RefreshButton onClick={fetchNodes} loading={loading} />
						<div className="rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm text-slate-300">
							{loading ? "Refreshing…" : `${nodes.length} nodes`}
						</div>
					</>
				}
			/>

			<div className="flex flex-col gap-3 lg:flex-row lg:items-center">
				<SearchInput
					value={query}
					onChange={setQuery}
					placeholder="Filter by node id, mode, agent URL, error…"
				/>
				<div className="flex flex-wrap gap-2">
					{(
						[
							["all", "All"],
							["drift", "Drift"],
							["restart", "Restart"],
							["error", "Errors"],
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
						to="/nodes/$nodeId"
						params={{ nodeId: node.node_id }}
						className="rounded-3xl border border-white/8 bg-slate-950/70 p-5 transition hover:border-cyan-400/25 hover:bg-cyan-400/8"
					>
						<div className="flex items-start justify-between gap-4">
							<div>
								<div className="text-xl font-semibold text-white">{node.node_id}</div>
								<div className="mt-2 text-sm text-slate-400">
									Mode <span className="text-cyan-200">{node.connection_mode ?? "unknown"}</span>
									{node.agent_url ? (
										<>
											{" · "}
											<span className="break-all text-slate-300">{node.agent_url}</span>
										</>
									) : null}
								</div>
							</div>
							<div className="flex flex-col items-end gap-2">
								<Badge tone={node.pending_restart ? "warn" : "ok"}>
									{node.pending_restart ? "restart pending" : "in sync"}
								</Badge>
								{node.desired_revision !== node.applied_revision ? (
									<Badge tone="info">revision drift</Badge>
								) : null}
							</div>
						</div>
						<div className="mt-5 grid gap-3 sm:grid-cols-2">
							<Value label="Desired revision" value={node.desired_revision} />
							<Value label="Applied revision" value={node.applied_revision} />
							<Value
								label="Last seen"
								value={`${formatRelative(node.last_seen_unix_ms)} · ${formatTime(node.last_seen_unix_ms, "short")}`}
							/>
							<Value label="Apply error" value={node.last_apply_error || "none"} />
						</div>
					</Link>
				))}

				{!loading && filtered.length === 0 ? (
					<StateCard
						label={
							nodes.length === 0
								? "No managed workers have enrolled yet."
								: "No nodes match the current filter."
						}
					/>
				) : null}
			</div>
		</div>
	);
}

function Value({ label, value }: { label: string; value: string | number }) {
	return (
		<div className="rounded-2xl border border-white/8 bg-white/4 px-4 py-3">
			<div className="text-xs uppercase tracking-[0.2em] text-slate-500">{label}</div>
			<div className="mt-2 break-all text-sm font-medium text-white">{value}</div>
		</div>
	);
}
