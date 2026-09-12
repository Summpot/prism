import { createFileRoute } from "@tanstack/react-router";
import { Shield, Unplug } from "lucide-react";
import { useMemo, useState } from "react";

import {
	Badge,
	CountChip,
	EmptyState,
	ErrorBanner,
	InfoValue,
	PageHeader,
	RefreshButton,
	SearchInput,
	StateCard,
	ToggleChip,
} from "@/components/ui";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { getTunnelServices } from "@/lib/managementApi";
import { useAdminQuery } from "@/hooks/useAdminQuery";
import { usePanelSession } from "@/lib/panelSession";
import { queryKeys } from "@/lib/state/queryKeys";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/tunnel-services")({
	component: AdminTunnelServicesPage,
});

function AdminTunnelServicesPage() {
	const { connection, ready } = usePanelSession();
	const [query, setQuery] = useState("");
	const [autoRefresh, setAutoRefresh] = useState(true);
	const [primaryOnly, setPrimaryOnly] = useState(false);

	const servicesQuery = useAdminQuery(queryKeys.admin.services(connection), getTunnelServices, {
		refetchInterval: autoRefresh ? 5_000 : false,
	});
	const services = servicesQuery.data ?? [];
	const loading = servicesQuery.isFetching && !servicesQuery.data;
	const error = servicesQuery.errorMessage;
	const fetchServices = () => {
		void servicesQuery.refetch();
	};

	const filtered = useMemo(() => {
		const needle = query.trim().toLowerCase();
		return services.filter((snapshot) => {
			if (primaryOnly && !snapshot.primary) {
				return false;
			}
			if (!needle) {
				return true;
			}
			const haystack = [
				snapshot.service.name,
				snapshot.service.proto,
				snapshot.service.local_addr,
				snapshot.service.remote_addr,
				snapshot.service.masquerade_host,
				snapshot.client_id,
				snapshot.remote,
			]
				.join(" ")
				.toLowerCase();
			return haystack.includes(needle);
		});
	}, [primaryOnly, query, services]);

	if (!ready) {
		return <StateCard label={m.common_restoring_session()} />;
	}

	if (!connection) {
		return <StateCard label={m.services_connect_panel()} />;
	}

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow={m.services_eyebrow()}
				title={m.services_title()}
				description={m.services_description()}
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							{m.admin_auto_refresh({ state: autoRefresh ? m.admin_on() : m.admin_off() })}
						</ToggleChip>
						<ToggleChip active={primaryOnly} onClick={() => setPrimaryOnly((value) => !value)}>
							{m.services_primary_only()}
						</ToggleChip>
						<RefreshButton onClick={fetchServices} loading={loading} />
						<CountChip icon={<Unplug className="h-4 w-4" />}>
							{loading
								? m.common_loading()
								: services.length === 1
									? m.services_count_one({ shown: filtered.length, total: services.length })
									: m.services_count({ shown: filtered.length, total: services.length })}
						</CountChip>
					</>
				}
			/>

			<SearchInput value={query} onChange={setQuery} placeholder={m.services_filter()} />

			{error ? <ErrorBanner message={error} onRetry={fetchServices} /> : null}

			{filtered.length > 0 ? (
				<div className="grid gap-4 xl:grid-cols-2">
					{filtered.map((snapshot, index) => (
						<Card
							key={`${snapshot.service.name}-${snapshot.client_id}-${index}`}
							className="shadow-xs"
						>
							<CardHeader className="flex flex-row items-start justify-between gap-4">
								<div>
									<CardTitle className="flex items-center gap-2">
										<Unplug className="h-4 w-4 text-muted-foreground" />
										{snapshot.service.name}
									</CardTitle>
									<div className="mt-1.5 text-sm text-muted-foreground">
										{m.services_client()}{" "}
										<span className="font-mono text-foreground">{snapshot.client_id}</span>
									</div>
								</div>
								<div className="flex flex-wrap items-center justify-end gap-2">
									<Badge tone={snapshot.primary ? "ok" : "neutral"}>
										{snapshot.primary ? m.services_primary() : m.services_secondary()}
									</Badge>
									{snapshot.service.route_only ? (
										<Badge tone="info">{m.services_route_only()}</Badge>
									) : null}
								</div>
							</CardHeader>
							<CardContent>
								<div className="grid gap-3 sm:grid-cols-2">
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
									{snapshot.service.masquerade_host ? (
										<InfoValue
											label={m.services_masquerade_host()}
											value={snapshot.service.masquerade_host}
										/>
									) : null}
								</div>
							</CardContent>
						</Card>
					))}
				</div>
			) : !loading ? (
				<EmptyState
					icon={<Shield className="h-8 w-8" />}
					label={services.length === 0 ? m.services_empty() : m.services_no_match()}
				/>
			) : null}
		</div>
	);
}
