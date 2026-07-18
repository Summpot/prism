import { createFileRoute } from "@tanstack/react-router";
import { Cable, Wifi } from "lucide-react";
import { useCallback, useMemo, useState } from "react";

import {
	EmptyState,
	ErrorBanner,
	PageHeader,
	RefreshButton,
	SearchInput,
	StateCard,
	ToggleChip,
} from "@/components/ui";
import { formatDuration, formatTime } from "@/lib/format";
import { getConnections, type SessionInfo } from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";

export const Route = createFileRoute("/connections")({
	component: ConnectionsPage,
});

function ConnectionsPage() {
	const { connection, ready } = usePanelSession();
	const [conns, setConns] = useState<SessionInfo[]>([]);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [query, setQuery] = useState("");
	const [autoRefresh, setAutoRefresh] = useState(true);

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
				setError(nextError instanceof Error ? nextError.message : String(nextError));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	usePolling(fetchConns, 3_000, Boolean(connection) && autoRefresh);

	const filtered = useMemo(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) {
			return conns;
		}
		return conns.filter((conn) =>
			[conn.id, conn.client, conn.host, conn.upstream].join(" ").toLowerCase().includes(needle),
		);
	}, [conns, query]);

	if (!ready) {
		return <StateCard label="Restoring session…" />;
	}

	if (!connection) {
		return <StateCard label="Connect the panel to a management node before viewing connections." />;
	}

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow="Proxy plane"
				title="Active connections"
				description="Live snapshot of TCP/UDP sessions held open by the proxy plane on this management endpoint."
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((value) => !value)}>
							Auto-refresh {autoRefresh ? "on" : "off"}
						</ToggleChip>
						<RefreshButton onClick={fetchConns} loading={loading} />
						<div className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm text-slate-300">
							<Cable className="h-4 w-4 text-cyan-300" />
							{loading
								? "Loading…"
								: `${filtered.length}/${conns.length} connection${conns.length === 1 ? "" : "s"}`}
						</div>
					</>
				}
			/>

			<SearchInput
				value={query}
				onChange={setQuery}
				placeholder="Filter by client, host, upstream…"
			/>

			{error ? <ErrorBanner message={error} onRetry={fetchConns} /> : null}

			{filtered.length > 0 ? (
				<div className="overflow-hidden rounded-3xl border border-white/8 bg-slate-950/70">
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
								{filtered.map((conn) => (
									<tr key={conn.id} className="transition hover:bg-white/3">
										<td className="px-5 py-4 font-mono text-cyan-200/85">{conn.client}</td>
										<td className="px-5 py-4 text-white">{conn.host || "—"}</td>
										<td className="px-5 py-4 font-mono text-slate-300">{conn.upstream}</td>
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
					<div className="divide-y divide-white/5 md:hidden">
						{filtered.map((conn) => (
							<div key={conn.id} className="space-y-3 px-5 py-4">
								<div className="flex items-center gap-2">
									<Wifi className="h-4 w-4 text-cyan-300" />
									<span className="font-mono text-sm text-cyan-200/85">{conn.client}</span>
								</div>
								<div className="grid grid-cols-2 gap-3">
									<MiniValue label="Host" value={conn.host || "—"} />
									<MiniValue label="Upstream" value={conn.upstream} />
									<MiniValue label="Started" value={formatTime(conn.started_at_unix_ms)} />
									<MiniValue label="Duration" value={formatDuration(conn.started_at_unix_ms)} />
								</div>
							</div>
						))}
					</div>
				</div>
			) : !loading ? (
				<EmptyState
					label={
						conns.length === 0
							? "No active proxy connections at this moment."
							: "No connections match the current filter."
					}
				/>
			) : null}
		</div>
	);
}

function MiniValue({ label, value }: { label: string; value: string }) {
	return (
		<div className="rounded-xl border border-white/8 bg-white/4 px-3 py-2">
			<div className="text-[10px] uppercase tracking-[0.2em] text-slate-500">{label}</div>
			<div className="mt-1 truncate text-xs text-white">{value}</div>
		</div>
	);
}
