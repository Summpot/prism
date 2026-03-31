import { createFileRoute } from "@tanstack/react-router";
import { Cable, RefreshCw, Wifi } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { getConnections, type SessionInfo } from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";

export const Route = createFileRoute("/connections")({
	component: ConnectionsPage,
});

function ConnectionsPage() {
	const { connection, ready } = usePanelSession();
	const [conns, setConns] = useState<SessionInfo[]>([]);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const fetchConns = useCallback(() => {
		if (!connection) {
			setConns([]);
			return;
		}

		setLoading(true);
		setError(null);

		getConnections(connection)
			.then((response) => {
				setConns(response);
			})
			.catch((nextError) => {
				setError(
					nextError instanceof Error ? nextError.message : String(nextError),
				);
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	useEffect(() => {
		fetchConns();
	}, [fetchConns]);

	if (!ready) {
		return <StateCard label="Restoring session…" />;
	}

	if (!connection) {
		return (
			<StateCard label="Connect the panel to a management node before viewing connections." />
		);
	}

	return (
		<div className="space-y-6">
			<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 md:p-8">
				<div className="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
					<div>
						<div className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">
							Proxy plane
						</div>
						<h1 className="mt-3 text-4xl font-semibold text-white">
							Active connections
						</h1>
						<p className="mt-3 max-w-3xl text-base leading-7 text-slate-400">
							Live snapshot of TCP/UDP sessions currently held open by the proxy
							plane. Each row represents a client-to-upstream connection pair.
						</p>
					</div>
					<div className="flex items-center gap-3">
						<button
							type="button"
							onClick={fetchConns}
							disabled={loading}
							className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10 disabled:opacity-50"
						>
							<RefreshCw
								className={`h-4 w-4 ${loading ? "animate-spin" : ""}`}
							/>
							Refresh
						</button>
						<div className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm text-slate-300">
							<Cable className="h-4 w-4 text-cyan-300" />
							{loading
								? "Loading…"
								: `${conns.length} active connection${conns.length === 1 ? "" : "s"}`}
						</div>
					</div>
				</div>
			</section>

			{error ? (
				<div className="flex items-center justify-between rounded-3xl border border-red-400/20 bg-red-400/8 px-5 py-4 text-sm text-red-100">
					<span>{error}</span>
					<button
						type="button"
						onClick={fetchConns}
						className="rounded-xl border border-red-400/30 px-3 py-1.5 text-xs font-medium transition hover:bg-red-400/15"
					>
						Retry
					</button>
				</div>
			) : null}

			{conns.length > 0 ? (
				<div className="overflow-hidden rounded-3xl border border-white/8 bg-slate-950/70">
					{/* Desktop table */}
					<div className="hidden md:block">
						<table className="w-full text-sm">
							<thead>
								<tr className="border-b border-white/8 text-xs uppercase tracking-[0.2em] text-slate-500">
									<th className="px-5 py-4 text-left font-medium">Client</th>
									<th className="px-5 py-4 text-left font-medium">Host</th>
									<th className="px-5 py-4 text-left font-medium">Upstream</th>
									<th className="px-5 py-4 text-left font-medium">Started</th>
									<th className="px-5 py-4 text-left font-medium">Duration</th>
								</tr>
							</thead>
							<tbody className="divide-y divide-white/5">
								{conns.map((conn) => (
									<tr key={conn.id} className="transition hover:bg-white/3">
										<td className="px-5 py-4 font-mono text-cyan-200/85">
											{conn.client}
										</td>
										<td className="px-5 py-4 text-white">{conn.host || "—"}</td>
										<td className="px-5 py-4 font-mono text-slate-300">
											{conn.upstream}
										</td>
										<td className="px-5 py-4 text-slate-400">
											{formatTime(conn.started_at_unix_ms)}
										</td>
										<td className="px-5 py-4 text-slate-400">
											{formatDuration(conn.started_at_unix_ms)}
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</div>
					{/* Mobile cards */}
					<div className="divide-y divide-white/5 md:hidden">
						{conns.map((conn) => (
							<div key={conn.id} className="space-y-3 px-5 py-4">
								<div className="flex items-center gap-2">
									<Wifi className="h-4 w-4 text-cyan-300" />
									<span className="font-mono text-sm text-cyan-200/85">
										{conn.client}
									</span>
								</div>
								<div className="grid grid-cols-2 gap-3">
									<MiniValue label="Host" value={conn.host || "—"} />
									<MiniValue label="Upstream" value={conn.upstream} />
									<MiniValue
										label="Started"
										value={formatTime(conn.started_at_unix_ms)}
									/>
									<MiniValue
										label="Duration"
										value={formatDuration(conn.started_at_unix_ms)}
									/>
								</div>
							</div>
						))}
					</div>
				</div>
			) : !loading ? (
				<div className="rounded-3xl border border-dashed border-white/10 bg-white/3 px-6 py-10 text-center text-sm text-slate-400">
					No active proxy connections at this moment.
				</div>
			) : null}
		</div>
	);
}

function MiniValue({ label, value }: { label: string; value: string }) {
	return (
		<div className="rounded-xl border border-white/8 bg-white/4 px-3 py-2">
			<div className="text-[10px] uppercase tracking-[0.2em] text-slate-500">
				{label}
			</div>
			<div className="mt-1 truncate text-xs text-white">{value}</div>
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

function formatTime(unixMs: number) {
	if (!unixMs) return "—";
	return new Intl.DateTimeFormat("en-US", {
		dateStyle: "medium",
		timeStyle: "medium",
	}).format(new Date(unixMs));
}

function formatDuration(startUnixMs: number) {
	if (!startUnixMs) return "—";
	const seconds = Math.floor((Date.now() - startUnixMs) / 1000);
	if (seconds < 60) return `${seconds}s`;
	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
	const hours = Math.floor(minutes / 60);
	return `${hours}h ${minutes % 60}m`;
}
