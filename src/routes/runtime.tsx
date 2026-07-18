import { createFileRoute } from "@tanstack/react-router";
import { Activity, Database, FileCode2, HeartPulse, RotateCcw, Server } from "lucide-react";
import { useCallback, useState } from "react";

import {
	ErrorBanner,
	EmptyState,
	InfoValue,
	MetricCard,
	PageHeader,
	RefreshButton,
	SecondaryButton,
	SectionCard,
	StateCard,
	ToggleChip,
} from "@/components/ui";
import { formatBytes, formatRelative, formatTime } from "@/lib/format";
import {
	getConfigPath,
	getHealth,
	getMetrics,
	type MetricsSnapshot,
	triggerReload,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";

export const Route = createFileRoute("/runtime")({
	component: RuntimePage,
});

function RuntimePage() {
	const { connection, ready } = usePanelSession();
	const [healthOk, setHealthOk] = useState<boolean | null>(null);
	const [configPath, setConfigPath] = useState<string | null>(null);
	const [metrics, setMetrics] = useState<MetricsSnapshot | null>(null);
	const [metricsDisabled, setMetricsDisabled] = useState(false);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [autoRefresh, setAutoRefresh] = useState(true);
	const [reloading, setReloading] = useState(false);
	const [reloadResult, setReloadResult] = useState<string | null>(null);

	const fetchData = useCallback(() => {
		if (!connection) {
			setHealthOk(null);
			setConfigPath(null);
			setMetrics(null);
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
			getMetrics(connection)
				.then((response) => {
					setMetrics(response);
					setMetricsDisabled(false);
				})
				.catch((nextError) => {
					if (
						nextError &&
						typeof nextError === "object" &&
						"status" in nextError &&
						nextError.status === 404
					) {
						setMetrics(null);
						setMetricsDisabled(true);
						return;
					}
					throw nextError;
				}),
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
			setReloadResult(`Reload signal sent (seq ${response.seq})`);
			fetchData();
		} catch (nextError) {
			setReloadResult(
				`Reload failed: ${nextError instanceof Error ? nextError.message : String(nextError)}`,
			);
		} finally {
			setReloading(false);
		}
	};

	if (!ready) {
		return <StateCard label="Restoring session…" />;
	}

	if (!connection) {
		return (
			<StateCard label="Connect the panel to a management node before inspecting runtime state." />
		);
	}

	const routeHits = metrics
		? Object.entries(metrics.route_hits_total).toSorted((a, b) => b[1] - a[1])
		: [];

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow="Local runtime"
				title="Management node runtime"
				description="Health, local config path, live metrics, and a reload signal for the Prism process serving this admin API."
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							Auto-refresh {autoRefresh ? "on" : "off"}
						</ToggleChip>
						<RefreshButton onClick={fetchData} loading={loading} />
						<SecondaryButton onClick={handleReload} disabled={reloading}>
							<RotateCcw className={`h-4 w-4 ${reloading ? "animate-spin" : ""}`} />
							Reload config
						</SecondaryButton>
					</>
				}
			/>

			{reloadResult ? (
				<div
					className={`rounded-3xl border px-5 py-4 text-sm ${
						reloadResult.startsWith("Reload failed")
							? "border-red-400/20 bg-red-400/8 text-red-100"
							: "border-emerald-400/20 bg-emerald-400/8 text-emerald-100"
					}`}
				>
					{reloadResult}
				</div>
			) : null}

			{error ? <ErrorBanner message={error} onRetry={fetchData} /> : null}

			<section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
				<MetricCard
					label="Health"
					value={healthOk == null ? "…" : healthOk ? "ok" : "down"}
					icon={<HeartPulse className="h-5 w-5" />}
				/>
				<MetricCard
					label="Active connections"
					value={metrics?.active_connections ?? (metricsDisabled ? "n/a" : "…")}
					icon={<Activity className="h-5 w-5" />}
				/>
				<MetricCard
					label="Connections total"
					value={metrics?.connections_total ?? (metricsDisabled ? "n/a" : "…")}
					icon={<Server className="h-5 w-5" />}
				/>
				<MetricCard
					label="Config path"
					value={configPath ?? "…"}
					icon={<FileCode2 className="h-5 w-5" />}
					compact
				/>
			</section>

			<section className="grid gap-4 xl:grid-cols-2">
				<SectionCard
					title="Traffic counters"
					description="In-process counters from GET /metrics when metrics are enabled on this node."
					icon={<Activity className="h-5 w-5" />}
				>
					{metricsDisabled ? (
						<EmptyState
							icon={<Database className="h-8 w-8" />}
							label="Metrics are disabled on this node. Enable [metrics] enabled = true in the local Prism config to expose counters."
						/>
					) : metrics ? (
						<div className="grid gap-3 sm:grid-cols-2">
							<InfoValue label="Ingress bytes" value={formatBytes(metrics.bytes_ingress_total)} />
							<InfoValue label="Egress bytes" value={formatBytes(metrics.bytes_egress_total)} />
							<InfoValue label="Active connections" value={metrics.active_connections} />
							<InfoValue label="Connections total" value={metrics.connections_total} />
						</div>
					) : (
						<StateCard label="Loading metrics…" />
					)}
				</SectionCard>

				<SectionCard
					title="Metrics store"
					description="Optional DuckDB-backed snapshot metadata when local history is configured."
					icon={<Database className="h-5 w-5" />}
				>
					{metrics?.store ? (
						<div className="grid gap-3 sm:grid-cols-2">
							<InfoValue label="Backend" value={metrics.store.backend} />
							<InfoValue label="Path" value={metrics.store.path} />
							<InfoValue label="Flush interval" value={`${metrics.store.flush_interval_ms} ms`} />
							<InfoValue
								label="Last flush"
								value={
									metrics.store.last_flush_unix_ms
										? `${formatRelative(metrics.store.last_flush_unix_ms)} (${formatTime(metrics.store.last_flush_unix_ms, "short")})`
										: "never"
								}
							/>
							{metrics.store.last_error ? (
								<div className="sm:col-span-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm text-red-100">
									{metrics.store.last_error}
								</div>
							) : null}
						</div>
					) : (
						<EmptyState label="No metrics store metadata available." />
					)}
				</SectionCard>
			</section>

			<SectionCard
				title="Route hits"
				description="Per-host hit counters recorded by the proxy plane."
				icon={<Activity className="h-5 w-5" />}
			>
				{routeHits.length > 0 ? (
					<div className="overflow-hidden rounded-3xl border border-white/8">
						<table className="w-full text-sm">
							<thead>
								<tr className="border-b border-white/8 text-xs uppercase tracking-[0.2em] text-slate-500">
									<th className="px-5 py-4 text-left font-medium">Host</th>
									<th className="px-5 py-4 text-right font-medium">Hits</th>
								</tr>
							</thead>
							<tbody className="divide-y divide-white/5">
								{routeHits.map(([host, hits]) => (
									<tr key={host} className="transition hover:bg-white/3">
										<td className="px-5 py-3 font-mono text-cyan-200/85">{host}</td>
										<td className="px-5 py-3 text-right text-white">{hits}</td>
									</tr>
								))}
							</tbody>
						</table>
					</div>
				) : (
					<EmptyState label="No route hits recorded yet, or metrics are unavailable." />
				)}
			</SectionCard>
		</div>
	);
}
