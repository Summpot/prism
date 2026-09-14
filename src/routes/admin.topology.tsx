import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { AdminReady } from "@/components/admin/AdminReady";
import { TopologyInspector } from "@/components/topology/TopologyInspector";
import { TopologyToolbar } from "@/components/topology/TopologyToolbar";
import { TopologyView } from "@/components/topology/TopologyView";
import type { SelectedElement, TopologyFilter } from "@/components/topology/topologyTypes";
import { useTopologyData } from "@/components/topology/useTopologyData";
import { EmptyState, ErrorBanner, PageHeader } from "@/components/ui";
import type { PanelConnection } from "@/lib/panelConnection";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/topology")({
	component: AdminTopologyPage,
});

function AdminTopologyPage() {
	return <AdminReady>{(connection) => <AdminTopologyBody connection={connection} />}</AdminReady>;
}

function AdminTopologyBody({ connection }: { connection: PanelConnection }) {
	const [autoRefresh, setAutoRefresh] = useState(true);
	const [intervalMs, setIntervalMs] = useState(3000);
	const [filter, setFilter] = useState<TopologyFilter>({
		protocol: "all",
		activeOnly: false,
		searchQuery: "",
	});
	const [selected, setSelected] = useState<SelectedElement>(null);

	const pollInterval = autoRefresh ? intervalMs : false;
	const { nodes, paths, summary, loading, error, refetch, canvasWidth, canvasHeight } =
		useTopologyData(connection, pollInterval, filter);

	return (
		<div className="space-y-5">
			{/* Page Header */}
			<PageHeader
				eyebrow={m.topology_eyebrow()}
				title={m.topology_title()}
				description={m.topology_description()}
			/>

			{/* Error Banner if any */}
			{error ? <ErrorBanner message={error} onRetry={refetch} /> : null}

			{/* KPI Summary and Filters Toolbar */}
			<TopologyToolbar
				summary={summary}
				filter={filter}
				onFilterChange={setFilter}
				autoRefresh={autoRefresh}
				onToggleAutoRefresh={() => setAutoRefresh((v) => !v)}
				refreshInterval={intervalMs}
				onIntervalChange={setIntervalMs}
				onRefresh={refetch}
				loading={loading}
			/>

			{/* Interactive Topology Graph Canvas */}
			{nodes.length > 0 ? (
				<div className="space-y-4">
					<TopologyView
						nodes={nodes}
						paths={paths}
						selected={selected}
						onSelect={setSelected}
						canvasWidth={canvasWidth}
						canvasHeight={canvasHeight}
					/>

					{/* Inspector Details (when path or node selected) */}
					{selected && <TopologyInspector selected={selected} onClose={() => setSelected(null)} />}
				</div>
			) : (
				<EmptyState title={m.topology_empty_title()} description={m.topology_empty_description()} />
			)}
		</div>
	);
}
