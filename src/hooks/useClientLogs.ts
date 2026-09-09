import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { clearClientLogs, getClientLogs } from "@/lib/client/clientIpc";
import { usePolling } from "@/lib/usePolling";
import type { ClientLogEntry } from "@/types/client";

export function useClientLogs(onCopyStatus?: (id: string | null) => void) {
	const [logs, setLogs] = useState<ClientLogEntry[]>([]);
	const [logFilterLevel, setLogFilterLevel] = useState<string>("ALL");
	const [logSearchQuery, setLogSearchQuery] = useState("");
	const [autoScrollLogs, setAutoScrollLogs] = useState(true);
	const logsContainerRef = useRef<HTMLDivElement | null>(null);
	const [isAtBottom, setIsAtBottom] = useState(true);
	const isAtBottomRef = useRef(true);

	// Fetch logs with deduplication to avoid unnecessary re-renders
	const fetchLogs = useCallback(() => {
		getClientLogs(300)
			.then((entries) => {
				setLogs((prev) => {
					if (prev.length === entries.length) {
						const prevLast = prev[prev.length - 1];
						const newLast = entries[entries.length - 1];
						if (
							(!prevLast && !newLast) ||
							(prevLast &&
								newLast &&
								prevLast.timestamp === newLast.timestamp &&
								prevLast.message === newLast.message)
						) {
							return prev;
						}
					}
					return entries;
				});
			})
			.catch(() => {});
	}, []);

	useEffect(() => {
		fetchLogs();
	}, [fetchLogs]);

	// Poll logs frequently
	usePolling(fetchLogs, 1500, true);

	// Filter logs
	const filteredLogs = useMemo(() => {
		return logs.filter((l) => {
			if (logFilterLevel !== "ALL" && l.level.toUpperCase() !== logFilterLevel) {
				return false;
			}
			if (logSearchQuery.trim()) {
				const q = logSearchQuery.toLowerCase();
				return (
					l.message.toLowerCase().includes(q) ||
					l.target.toLowerCase().includes(q) ||
					l.level.toLowerCase().includes(q)
				);
			}
			return true;
		});
	}, [logs, logFilterLevel, logSearchQuery]);

	// Scroll management for logs container
	const handleLogsScroll = useCallback(() => {
		const container = logsContainerRef.current;
		if (!container) return;
		const distanceFromBottom =
			container.scrollHeight - container.scrollTop - container.clientHeight;
		const atBottom = distanceFromBottom <= 24;
		setIsAtBottom(atBottom);
		isAtBottomRef.current = atBottom;
	}, []);

	const scrollToBottom = useCallback((smooth = false) => {
		const container = logsContainerRef.current;
		if (!container) return;
		if (smooth) {
			container.scrollTo({ top: container.scrollHeight, behavior: "smooth" });
		} else {
			container.scrollTop = container.scrollHeight;
		}
		setIsAtBottom(true);
		isAtBottomRef.current = true;
	}, []);

	// Auto-scroll logs only when user is already at the bottom and auto-scroll is enabled
	useEffect(() => {
		if (autoScrollLogs && isAtBottomRef.current) {
			scrollToBottom(false);
		}
	}, [filteredLogs, autoScrollLogs, scrollToBottom]);

	const handleClearLogs = useCallback(async () => {
		try {
			await clearClientLogs();
			setLogs([]);
		} catch (err) {
			console.error("Failed to clear logs:", err);
		}
	}, []);

	const handleCopyAllLogs = useCallback(() => {
		const text = filteredLogs
			.map((l) => `[${l.timestamp}] [${l.level}] [${l.target}] ${l.message}`)
			.join("\n");
		navigator.clipboard.writeText(text);
		if (onCopyStatus) {
			onCopyStatus("all-logs");
			setTimeout(() => onCopyStatus(null), 2000);
		}
	}, [filteredLogs, onCopyStatus]);

	return {
		logs,
		setLogs,
		filteredLogs,
		logFilterLevel,
		setLogFilterLevel,
		logSearchQuery,
		setLogSearchQuery,
		autoScrollLogs,
		setAutoScrollLogs,
		logsContainerRef,
		isAtBottom,
		handleLogsScroll,
		scrollToBottom,
		handleClearLogs,
		handleCopyAllLogs,
		fetchLogs,
	};
}
