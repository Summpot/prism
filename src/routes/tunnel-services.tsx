import { createFileRoute } from "@tanstack/react-router";
import { RefreshCw, Shield, Unplug } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { getTunnelServices, type ServiceSnapshot } from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";

export const Route = createFileRoute("/tunnel-services")({
	component: TunnelServicesPage,
});

function TunnelServicesPage() {
	const { connection, ready } = usePanelSession();
	const [services, setServices] = useState<ServiceSnapshot[]>([]);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const fetchServices = useCallback(() => {
		if (!connection) {
			setServices([]);
			return;
		}

		setLoading(true);
		setError(null);

		getTunnelServices(connection)
			.then((response) => {
				setServices(response);
			})
			.catch((nextError) => {
				setError(nextError instanceof Error ? nextError.message : String(nextError));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	useEffect(() => {
		fetchServices();
	}, [fetchServices]);

	if (!ready) {
		return <StateCard label="Restoring session…" />;
	}

	if (!connection) {
		return (
			<StateCard label="Connect the panel to a management node before viewing tunnel services." />
		);
	}

	return (
		<div className="space-y-6">
			<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 md:p-8">
				<div className="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
					<div>
						<div className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">
							Tunnel plane
						</div>
						<h1 className="mt-3 text-4xl font-semibold text-white">Registered tunnel services</h1>
						<p className="mt-3 max-w-3xl text-base leading-7 text-slate-400">
							Services registered by tunnel clients through the reverse-tunnel session protocol.
							Primary owners handle routing traffic; secondary registrations wait as failover
							candidates.
						</p>
					</div>
					<div className="flex items-center gap-3">
						<button
							type="button"
							onClick={fetchServices}
							disabled={loading}
							className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10 disabled:opacity-50"
						>
							<RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
							Refresh
						</button>
						<div className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm text-slate-300">
							<Unplug className="h-4 w-4 text-cyan-300" />
							{loading
								? "Loading…"
								: `${services.length} service${services.length === 1 ? "" : "s"}`}
						</div>
					</div>
				</div>
			</section>

			{error ? (
				<div className="flex items-center justify-between rounded-3xl border border-red-400/20 bg-red-400/8 px-5 py-4 text-sm text-red-100">
					<span>{error}</span>
					<button
						type="button"
						onClick={fetchServices}
						className="rounded-xl border border-red-400/30 px-3 py-1.5 text-xs font-medium transition hover:bg-red-400/15"
					>
						Retry
					</button>
				</div>
			) : null}

			{services.length > 0 ? (
				<div className="grid gap-4 xl:grid-cols-2">
					{services.map((snapshot, index) => (
						<div
							key={`${snapshot.service.name}-${snapshot.client_id}-${index}`}
							className="rounded-3xl border border-white/8 bg-slate-950/70 p-5"
						>
							<div className="flex items-start justify-between gap-4">
								<div>
									<div className="flex items-center gap-2">
										<Unplug className="h-5 w-5 text-cyan-300" />
										<div className="text-xl font-semibold text-white">{snapshot.service.name}</div>
									</div>
									<div className="mt-2 text-sm text-slate-400">
										Client <span className="font-mono text-cyan-200/85">{snapshot.client_id}</span>
									</div>
								</div>
								<div className="flex items-center gap-2">
									{snapshot.primary ? (
										<span className="rounded-full bg-emerald-400/12 px-3 py-1 text-xs font-semibold uppercase tracking-[0.2em] text-emerald-100">
											Primary
										</span>
									) : (
										<span className="rounded-full bg-slate-400/12 px-3 py-1 text-xs font-semibold uppercase tracking-[0.2em] text-slate-300">
											Secondary
										</span>
									)}
									{snapshot.service.route_only ? (
										<span className="rounded-full bg-violet-400/12 px-3 py-1 text-xs font-semibold uppercase tracking-[0.2em] text-violet-200">
											Route only
										</span>
									) : null}
								</div>
							</div>

							<div className="mt-5 grid gap-3 sm:grid-cols-2">
								<InfoValue label="Protocol" value={snapshot.service.proto} />
								<InfoValue label="Local addr" value={snapshot.service.local_addr || "—"} />
								<InfoValue label="Remote addr" value={snapshot.service.remote_addr || "—"} />
								<InfoValue label="Remote peer" value={snapshot.remote} />
								{snapshot.service.masquerade_host ? (
									<InfoValue label="Masquerade host" value={snapshot.service.masquerade_host} />
								) : null}
							</div>
						</div>
					))}
				</div>
			) : !loading ? (
				<div className="rounded-3xl border border-dashed border-white/10 bg-white/3 px-6 py-10 text-center text-sm text-slate-400">
					<Shield className="mx-auto mb-3 h-8 w-8 text-slate-500" />
					No tunnel services registered. Tunnel clients will appear here when they connect and
					register services through the tunnel session protocol.
				</div>
			) : null}
		</div>
	);
}

function InfoValue({ label, value }: { label: string; value: string }) {
	return (
		<div className="rounded-2xl border border-white/8 bg-white/4 px-4 py-3">
			<div className="text-xs uppercase tracking-[0.2em] text-slate-500">{label}</div>
			<div className="mt-2 text-sm font-medium text-white">{value}</div>
		</div>
	);
}

function StateCard({ label }: { label: string }) {
	return (
		<div className="rounded-3xl border border-white/8 bg-slate-950/70 px-6 py-10 text-sm text-slate-400">
			{label}
		</div>
	);
}
