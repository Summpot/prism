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
import { getConfigPath, getHealth, triggerReload } from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";

export const Route = createFileRoute("/runtime")({
	component: RuntimePage,
});

function RuntimePage() {
	const { connection, ready } = usePanelSession();
	const [healthOk, setHealthOk] = useState<boolean | null>(null);
	const [configPath, setConfigPath] = useState<string | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [autoRefresh, setAutoRefresh] = useState(true);
	const [reloading, setReloading] = useState(false);
	const [reloadResult, setReloadResult] = useState<string | null>(null);

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

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow="Local runtime"
				title="Management node runtime"
				description="Health, local config path, and a reload signal for the Prism process serving this admin API."
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

			<section className="grid gap-4 md:grid-cols-2">
				<MetricCard
					label="Health"
					value={healthOk == null ? "…" : healthOk ? "ok" : "down"}
					icon={<HeartPulse className="h-5 w-5" />}
				/>
				<MetricCard
					label="Config path"
					value={configPath ?? "…"}
					icon={<FileCode2 className="h-5 w-5" />}
					compact
				/>
			</section>
		</div>
	);
}
