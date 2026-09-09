import { useCallback, useEffect, useRef, useState } from "react";

import { getClientStatus } from "@/lib/client/clientIpc";
import { usePolling } from "@/lib/usePolling";
import type { ClientStatusResponse, CumulativeStats } from "@/types/client";

export function useClientStatus() {
	const [status, setStatus] = useState<ClientStatusResponse | null>(null);
	const [cumulativeStats, setCumulativeStats] = useState<CumulativeStats | null>(null);
	const [statsViewMode, setStatsViewMode] = useState<"session" | "lifetime">("session");
	const [throughputSamples, setThroughputSamples] = useState<number[]>([
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
	]);
	const prevWireRef = useRef(0);
	const [uptimeSeconds, setUptimeSeconds] = useState(0);

	const fetchStatus = useCallback(() => {
		getClientStatus()
			.then((resp) => {
				setStatus(resp);
				if (resp.cumulative_stats) {
					setCumulativeStats(resp.cumulative_stats);
				}
			})
			.catch((err) => {
				console.debug("Failed to fetch client status:", err);
			});
	}, []);

	useEffect(() => {
		fetchStatus();
	}, [fetchStatus]);

	// Poll status frequently
	usePolling(fetchStatus, 1500, true);

	// Connection duration timer
	useEffect(() => {
		let interval: ReturnType<typeof setInterval> | null = null;
		if (status?.state === "connected") {
			interval = setInterval(() => {
				setUptimeSeconds((prev) => prev + 1);
			}, 1000);
		} else {
			setUptimeSeconds(0);
		}
		return () => {
			if (interval) clearInterval(interval);
		};
	}, [status?.state]);

	// Throughput sample tracking for live waveform sparkline
	useEffect(() => {
		if (status?.running) {
			const currentWire = status.stats.wire_bytes;
			const delta = prevWireRef.current > 0 ? Math.max(0, currentWire - prevWireRef.current) : 0;
			prevWireRef.current = currentWire;
			setThroughputSamples((prev) => [...prev.slice(1), delta]);
		} else {
			prevWireRef.current = 0;
			setThroughputSamples([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
		}
	}, [status?.running, status?.stats.wire_bytes]);

	const isRunning = status?.running ?? false;
	const isConnected = status?.state === "connected";
	const isConnecting = status?.state === "connecting";

	const rawBytes = status?.stats.raw_bytes ?? 0;
	const wireBytes = status?.stats.wire_bytes ?? 0;
	const savedRatio = (status?.stats.saved_ratio ?? 0) * 100;

	const formatUptime = useCallback((seconds: number) => {
		const hrs = Math.floor(seconds / 3600);
		const mins = Math.floor((seconds % 3600) / 60);
		const secs = seconds % 60;
		return `${hrs.toString().padStart(2, "0")}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
	}, []);

	return {
		status,
		setStatus,
		cumulativeStats,
		setCumulativeStats,
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
		fetchStatus,
	};
}
