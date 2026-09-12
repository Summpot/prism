import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { clearClientLogs, getClientLogs } from "@/lib/client/clientIpc";
import { useClientUiStore } from "@/lib/state/clientUiStore";
import { queryKeys } from "@/lib/state/queryKeys";

export function useClientLogs() {
	const queryClient = useQueryClient();
	const logsQuery = useQuery({
		queryKey: queryKeys.client.logs,
		queryFn: () => getClientLogs(300),
		refetchInterval: 1_500,
	});
	const logs = logsQuery.data ?? [];

	const logFilterLevel = useClientUiStore((s) => s.logFilterLevel);
	const setLogFilterLevel = useClientUiStore((s) => s.setLogFilterLevel);
	const logSearchQuery = useClientUiStore((s) => s.logSearchQuery);
	const setLogSearchQuery = useClientUiStore((s) => s.setLogSearchQuery);
	const autoScrollLogs = useClientUiStore((s) => s.autoScrollLogs);
	const setAutoScrollLogs = useClientUiStore((s) => s.setAutoScrollLogs);
	const copyText = useClientUiStore((s) => s.copyText);
	const copied = useClientUiStore((s) => s.copied);

	const logsContainerRef = useRef<HTMLDivElement | null>(null);
	const [isAtBottom, setIsAtBottom] = useState(true);
	const isAtBottomRef = useRef(true);

	const filteredLogs = useMemo(() => {
		return logs.filter((entry) => {
			if (logFilterLevel !== "ALL" && entry.level.toUpperCase() !== logFilterLevel) {
				return false;
			}
			if (logSearchQuery.trim()) {
				const q = logSearchQuery.toLowerCase();
				return (
					entry.message.toLowerCase().includes(q) ||
					entry.target.toLowerCase().includes(q) ||
					entry.level.toLowerCase().includes(q)
				);
			}
			return true;
		});
	}, [logs, logFilterLevel, logSearchQuery]);

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

	useEffect(() => {
		if (autoScrollLogs && isAtBottomRef.current) {
			scrollToBottom(false);
		}
	}, [filteredLogs, autoScrollLogs, scrollToBottom]);

	const handleClearLogs = useCallback(async () => {
		try {
			await clearClientLogs();
			queryClient.setQueryData(queryKeys.client.logs, []);
		} catch (err) {
			console.error("Failed to clear logs:", err);
		}
	}, [queryClient]);

	const handleCopyAllLogs = useCallback(() => {
		const text = filteredLogs
			.map((entry) => `[${entry.timestamp}] [${entry.level}] [${entry.target}] ${entry.message}`)
			.join("\n");
		copyText(text, "all-logs");
	}, [copyText, filteredLogs]);

	return {
		logs,
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
		copied,
	};
}
