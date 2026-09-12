import { createFileRoute } from "@tanstack/react-router";
import { FileCode2, HeartPulse, RotateCcw } from "lucide-react";
import { useState } from "react";

import { AdminReady } from "@/components/admin/AdminReady";
import {
	ErrorBanner,
	MetricCard,
	PageHeader,
	RefreshButton,
	ResultBanner,
	SecondaryButton,
	ToggleChip,
} from "@/components/ui";
import { getConfigPath, getHealth, triggerReload } from "@/lib/managementApi";
import { useAdminQuery } from "@/hooks/useAdminQuery";
import type { PanelConnection } from "@/lib/panelConnection";
import { invalidateAdminQueries } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/runtime")({
	component: AdminRuntimePage,
});

function AdminRuntimePage() {
	return (
		<AdminReady connectLabel={m.runtime_connect_panel()}>
			{(connection) => <AdminRuntimeBody connection={connection} />}
		</AdminReady>
	);
}

function AdminRuntimeBody({ connection }: { connection: PanelConnection }) {
	const [autoRefresh, setAutoRefresh] = useState(true);
	const [reloading, setReloading] = useState(false);
	const [reloadResult, setReloadResult] = useState<{ ok: boolean; text: string } | null>(null);
	const interval = autoRefresh ? 5_000 : false;

	const healthQuery = useAdminQuery(queryKeys.admin.health(connection), getHealth, {
		refetchInterval: interval,
	});
	const pathQuery = useAdminQuery(queryKeys.admin.configPath(connection), getConfigPath, {
		refetchInterval: interval,
	});

	const healthOk = healthQuery.isError ? false : (healthQuery.data?.ok ?? null);
	const configPath = pathQuery.data?.path ?? null;
	const loading = healthQuery.isFetching && !healthQuery.data;
	const error = healthQuery.errorMessage;
	const fetchData = () => {
		void healthQuery.refetch();
		void pathQuery.refetch();
	};

	const handleReload = async () => {
		if (!connection) {
			return;
		}
		setReloading(true);
		setReloadResult(null);
		try {
			const response = await triggerReload(connection);
			setReloadResult({ ok: true, text: m.admin_reload_sent({ seq: response.seq }) });
			await invalidateAdminQueries();
			fetchData();
		} catch (nextError) {
			setReloadResult({
				ok: false,
				text: m.admin_reload_failed({
					message: nextError instanceof Error ? nextError.message : String(nextError),
				}),
			});
		} finally {
			setReloading(false);
		}
	};

	return (
		<div className="space-y-5">
			<PageHeader
				eyebrow={m.runtime_eyebrow()}
				title={m.runtime_title()}
				description={m.runtime_description()}
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							{m.admin_auto_refresh({ state: autoRefresh ? m.admin_on() : m.admin_off() })}
						</ToggleChip>
						<RefreshButton onClick={fetchData} loading={loading} />
						<SecondaryButton onClick={handleReload} disabled={reloading}>
							<RotateCcw className={`h-4 w-4 ${reloading ? "animate-spin" : ""}`} />
							{m.admin_reload()}
						</SecondaryButton>
					</>
				}
			/>

			{reloadResult ? <ResultBanner ok={reloadResult.ok}>{reloadResult.text}</ResultBanner> : null}

			{error ? <ErrorBanner message={error} onRetry={fetchData} /> : null}

			<section className="grid gap-4 md:grid-cols-2">
				<MetricCard
					label={m.runtime_health()}
					value={
						healthOk == null ? "…" : healthOk ? m.runtime_health_ok() : m.runtime_health_down()
					}
					icon={<HeartPulse className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.runtime_config_path()}
					value={configPath ?? "…"}
					icon={<FileCode2 className="h-5 w-5" />}
					compact
				/>
			</section>
		</div>
	);
}
