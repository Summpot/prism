import { createFileRoute } from "@tanstack/react-router";
import { Radio, Unplug } from "lucide-react";
import { useMemo, useState } from "react";

import { AdminReady } from "@/components/admin/AdminReady";
import { hasOptimizerTraffic, OptimizerStatsView } from "@/components/traffic/OptimizerStatsView";
import {
	Badge,
	CountChip,
	EmptyState,
	ErrorBanner,
	InfoValue,
	PageHeader,
	RefreshButton,
	SearchInput,
	ToggleChip,
} from "@/components/ui";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { formatBytes, formatPercentage } from "@/lib/format";
import { getOptimizerStats, getTunnelServices, type ServiceSnapshot } from "@/lib/managementApi";
import { useAdminQuery } from "@/hooks/useAdminQuery";
import type { PanelConnection } from "@/lib/panelConnection";
import { queryKeys } from "@/lib/state/queryKeys";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/connectors")({
	component: AdminConnectorsPage,
});

function AdminConnectorsPage() {
	return (
		<AdminReady connectLabel={m.services_connect_panel()}>
			{(connection) => <AdminConnectorsBody connection={connection} />}
		</AdminReady>
	);
}

function AdminConnectorsBody({ connection }: { connection: PanelConnection }) {
	const [query, setQuery] = useState("");
	const [autoRefresh, setAutoRefresh] = useState(true);
	const interval = autoRefresh ? 4_000 : false;

	const servicesQuery = useAdminQuery(queryKeys.admin.services(connection), getTunnelServices, {
		refetchInterval: interval,
	});
	const optimizerQuery = useAdminQuery(
		queryKeys.admin.optimizer(connection),
		(conn) => getOptimizerStats(conn).catch(() => null),
		{ refetchInterval: interval },
	);

	const services = servicesQuery.data ?? [];
	const optimizer = optimizerQuery.data ?? null;
	const loading = servicesQuery.isFetching && !servicesQuery.data;
	const error = servicesQuery.errorMessage;
	const fetchData = () => {
		void servicesQuery.refetch();
		void optimizerQuery.refetch();
	};

	const grouped = useMemo(() => {
		const needle = query.trim().toLowerCase();
		const byClient = new Map<string, ServiceSnapshot[]>();
		for (const snapshot of services) {
			const haystack = [
				snapshot.service.name,
				snapshot.service.proto,
				snapshot.client_id,
				snapshot.remote,
			]
				.join(" ")
				.toLowerCase();
			if (needle && !haystack.includes(needle)) {
				continue;
			}
			const list = byClient.get(snapshot.client_id) ?? [];
			list.push(snapshot);
			byClient.set(snapshot.client_id, list);
		}

		const orphanNames = Object.keys(optimizer?.services ?? {}).filter((name) => {
			if (needle && !name.toLowerCase().includes(needle)) return false;
			return !services.some((s) => s.service.name.toLowerCase() === name.toLowerCase());
		});

		return { byClient, orphanNames };
	}, [optimizer?.services, query, services]);

	const connectorCount = grouped.byClient.size;

	return (
		<div className="space-y-5">
			<PageHeader
				eyebrow={m.admin_connectors_eyebrow()}
				title={m.admin_connectors_title()}
				description={m.admin_connectors_description()}
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							{m.admin_auto_refresh({ state: autoRefresh ? m.admin_on() : m.admin_off() })}
						</ToggleChip>
						<RefreshButton onClick={fetchData} loading={loading} />
						<CountChip icon={<Radio className="h-4 w-4" />}>
							{loading
								? m.common_loading()
								: m.admin_connectors_count({
										connectors: connectorCount,
										services: services.length,
									})}
						</CountChip>
					</>
				}
			/>

			<SearchInput value={query} onChange={setQuery} placeholder={m.admin_connectors_filter()} />

			{error ? <ErrorBanner message={error} onRetry={fetchData} /> : null}

			{connectorCount === 0 && grouped.orphanNames.length === 0 && !loading ? (
				<EmptyState
					icon={<Unplug className="h-8 w-8" />}
					label={services.length === 0 ? m.admin_connectors_empty() : m.admin_connectors_no_match()}
				/>
			) : (
				<div className="space-y-4">
					{[...grouped.byClient.entries()].map(([clientId, snapshots]) => (
						<Card key={clientId} className="shadow-xs">
							<CardHeader className="flex flex-row items-start justify-between gap-4">
								<div>
									<CardTitle className="flex items-center gap-2 text-base">
										<Radio className="h-4 w-4 text-muted-foreground" />
										{clientId || m.admin_unknown()}
									</CardTitle>
									<div className="mt-1 text-xs text-muted-foreground">
										{m.admin_connector_services({ count: snapshots.length })}
									</div>
								</div>
							</CardHeader>
							<CardContent className="space-y-4">
								{snapshots.map((snapshot, index) => {
									const stats =
										optimizer?.services?.[snapshot.service.name] ??
										optimizer?.services?.[snapshot.service.name.toLowerCase()];
									return (
										<div
											key={`${snapshot.service.name}-${snapshot.client_id}-${index}`}
											className="space-y-3 rounded-lg border border-border/70 p-3"
										>
											<div className="flex flex-wrap items-start justify-between gap-2">
												<div>
													<div className="flex items-center gap-2">
														<Unplug className="h-4 w-4 text-muted-foreground" />
														<span className="text-sm font-semibold">{snapshot.service.name}</span>
														<Badge tone={snapshot.primary ? "ok" : "neutral"}>
															{snapshot.primary ? m.services_primary() : m.services_secondary()}
														</Badge>
													</div>
												</div>
												{stats && hasOptimizerTraffic(stats) ? (
													<span className="font-mono text-[11px] text-muted-foreground">
														{formatBytes(stats.raw_bytes)} → {formatBytes(stats.wire_bytes)}{" "}
														<span className="text-emerald-500">
															({formatPercentage(stats.saved_ratio)})
														</span>
													</span>
												) : null}
											</div>
											<div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
												<InfoValue label={m.services_protocol()} value={snapshot.service.proto} />
												<InfoValue
													label={m.services_local_addr()}
													value={snapshot.service.local_addr || "—"}
												/>
												<InfoValue
													label={m.services_remote_addr()}
													value={snapshot.service.remote_addr || "—"}
												/>
												<InfoValue label={m.services_remote_peer()} value={snapshot.remote} />
											</div>
											<OptimizerStatsView
												stats={stats}
												emptyLabel={m.admin_connectors_no_service_stats()}
											/>
										</div>
									);
								})}
							</CardContent>
						</Card>
					))}

					{grouped.orphanNames.length > 0 ? (
						<Card className="shadow-xs">
							<CardHeader>
								<CardTitle className="text-base">{m.admin_connectors_unattached()}</CardTitle>
							</CardHeader>
							<CardContent className="space-y-4">
								{grouped.orphanNames.map((name) => (
									<div key={name} className="space-y-3 rounded-lg border border-border/70 p-3">
										<div className="text-sm font-semibold">{name}</div>
										<OptimizerStatsView stats={optimizer?.services?.[name]} />
									</div>
								))}
							</CardContent>
						</Card>
					) : null}
				</div>
			)}
		</div>
	);
}
