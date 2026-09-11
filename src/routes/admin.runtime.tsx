import { createFileRoute } from "@tanstack/react-router";
import { FileCode2, HeartPulse, RotateCcw } from "lucide-react";
import { useCallback, useState } from "react";

import {
	ErrorBanner,
	MetricCard,
	PageHeader,
	RefreshButton,
	SecondaryButton,
	StateCard,
	ToggleChip,
} from "@/components/ui";
import { MiddlewareConfigEditor } from "@/components/MiddlewareConfigEditor";
import { getConfigPath, getHealth, triggerReload } from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/runtime")({
	component: AdminRuntimePage,
});

function AdminRuntimePage() {
	const { connection, ready } = usePanelSession();
	const [healthOk, setHealthOk] = useState<boolean | null>(null);
	const [configPath, setConfigPath] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [autoRefresh, setAutoRefresh] = useState(true);
	const [reloading, setReloading] = useState(false);
	const [reloadResult, setReloadResult] = useState<{ ok: boolean; text: string } | null>(null);

	const fetchData = useCallback(() => {
		if (!connection) {
			setHealthOk(null);
			setConfigPath(null);
			return;
		}

		setLoading(true);
		setError(null);

		Promise.all([
			getHealth(connection)
				.then((response) => setHealthOk(response.ok))
				.catch((nextError) => {
					setHealthOk(false);
					throw nextError;
				}),
			getConfigPath(connection).then((response) => setConfigPath(response.path)),
		])
			.catch((nextError) => {
				setError(nextError instanceof Error ? nextError.message : String(nextError));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	usePolling(fetchData, 5_000, Boolean(connection) && autoRefresh);

	const handleReload = async () => {
		if (!connection) {
			return;
		}
		setReloading(true);
		setReloadResult(null);
		try {
			const response = await triggerReload(connection);
			setReloadResult({ ok: true, text: m.admin_reload_sent({ seq: response.seq }) });
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

	if (!ready) {
		return <StateCard label={m.common_restoring_session()} />;
	}

	if (!connection) {
		return <StateCard label={m.runtime_connect_panel()} />;
	}

	return (
		<div className="space-y-6">
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

			{reloadResult ? (
				<div
					className={`rounded-3xl border px-5 py-4 text-sm ${
						reloadResult.ok
							? "border-emerald-400/20 bg-emerald-400/8 text-emerald-100"
							: "border-red-400/20 bg-red-400/8 text-red-100"
					}`}
				>
					{reloadResult.text}
				</div>
			) : null}

			{error ? <ErrorBanner message={error} onRetry={fetchData} /> : null}

			<section className="grid gap-4 md:grid-cols-2">
				<MetricCard
					label={m.runtime_health()}
					value={healthOk == null ? "…" : healthOk ? m.runtime_health_ok() : m.runtime_health_down()}
					icon={<HeartPulse className="h-5 w-5" />}
				/>
				<MetricCard
					label={m.runtime_config_path()}
					value={configPath ?? "…"}
					icon={<FileCode2 className="h-5 w-5" />}
					compact
				/>
			</section>

			<MiddlewareConfigEditor connection={connection} />
		</div>
	);
}
