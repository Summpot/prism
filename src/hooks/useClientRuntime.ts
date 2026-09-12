import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { getClientStatus } from "@/lib/client/clientIpc";
import { formatUptime } from "@/lib/format";
import { useClientUiStore } from "@/lib/state/clientUiStore";
import { queryKeys } from "@/lib/state/queryKeys";
import type { ClientStatusResponse, CumulativeStats } from "@/types/client";

export function useUptimeSeconds(connectedAt: number | null): number {
	const [now, setNow] = useState(() => Date.now());

	useEffect(() => {
		if (connectedAt == null) {
			return;
		}
		const id = window.setInterval(() => setNow(Date.now()), 1000);
		return () => window.clearInterval(id);
	}, [connectedAt]);

	if (connectedAt == null) {
		return 0;
	}
	return Math.max(0, Math.floor((now - connectedAt) / 1000));
}

export function useClientRuntime() {
	const statusQuery = useQuery({
		queryKey: queryKeys.client.status,
		queryFn: getClientStatus,
		refetchInterval: 1_500,
	});
	const statsViewMode = useClientUiStore((s) => s.statsViewMode);
	const setStatsViewMode = useClientUiStore((s) => s.setStatsViewMode);
	const throughputSamples = useClientUiStore((s) => s.throughputSamples);
	const connectedAt = useClientUiStore((s) => s.connectedAt);
	const uptimeSeconds = useUptimeSeconds(connectedAt);

	const status: ClientStatusResponse | null = statusQuery.data ?? null;
	const cumulativeStats: CumulativeStats | null = status?.cumulative_stats ?? null;
	const isRunning = status?.running ?? false;
	const isConnected = status?.state === "connected";
	const isConnecting = status?.state === "connecting";
	const rawBytes = status?.stats.raw_bytes ?? 0;
	const wireBytes = status?.stats.wire_bytes ?? 0;
	const savedRatio = (status?.stats.saved_ratio ?? 0) * 100;

	return {
		status,
		cumulativeStats,
		statsViewMode,
		setStatsViewMode,
		throughputSamples,
		uptimeSeconds,
		formatUptime,
		isRunning,
		isConnected,
		isConnecting,
		rawBytes,
		wireBytes,
		savedRatio,
		isPending: statusQuery.isPending,
	};
}
