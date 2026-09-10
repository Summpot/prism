import { ArrowDown, Check, Copy, Search, Terminal } from "lucide-react";

import { useClient } from "@/context/ClientContext";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

export function ClientLogs() {
	const {
		logs,
		filteredLogs,
		logFilterLevel,
		setLogFilterLevel,
		logSearchQuery,
		setLogSearchQuery,
		autoScrollLogs,
		setAutoScrollLogs,
		isAtBottom,
		logsContainerRef,
		handleLogsScroll,
		scrollToBottom,
		handleClearLogs,
		handleCopyAllLogs,
		copied,
	} = useClient();

	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-hidden">
			{/* Header Bar */}
			<div className="flex flex-none select-none items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
				<div className="flex items-center gap-2 min-w-0">
					<Terminal className="h-4 w-4 text-primary flex-none" />
					<div className="min-w-0">
						<h1 className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground">
							{m.client_logs_title()}
						</h1>
						<p className="truncate text-[10px] text-muted-foreground hidden sm:block">
							{m.client_logs_description()}
						</p>
					</div>
					<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4 font-mono">
						{filteredLogs.length} / {logs.length}
					</Badge>
				</div>

				{/* Action Toolbar */}
				<div className="flex items-center gap-1.5 flex-wrap">
					{/* Level Filters */}
					<div className="flex items-center rounded border border-input p-0.5 text-[10px]">
						{(["ALL", "INFO", "WARN", "ERROR"] as const).map((lvl) => (
							<button
								key={lvl}
								type="button"
								onClick={() => setLogFilterLevel(lvl)}
								className={cn(
									"rounded px-2 py-0.5 font-semibold transition cursor-pointer",
									logFilterLevel === lvl
										? "bg-primary text-primary-foreground"
										: "text-muted-foreground hover:text-foreground",
								)}
							>
								{lvl}
							</button>
						))}
					</div>

					{/* Search Filter */}
					<div className="relative w-28 sm:w-36">
						<Search className="pointer-events-none absolute top-1/2 left-2 h-3 w-3 -translate-y-1/2 text-muted-foreground" />
						<Input
							placeholder={m.client_logs_filter()}
							value={logSearchQuery}
							onChange={(e) => setLogSearchQuery(e.target.value)}
							className="h-7 pl-6 pr-2 text-xs font-mono"
						/>
					</div>

					<Button
						variant={autoScrollLogs && isAtBottom ? "secondary" : "outline"}
						size="xs"
						onClick={() => {
							if (autoScrollLogs && isAtBottom) {
								setAutoScrollLogs(false);
							} else {
								setAutoScrollLogs(true);
								scrollToBottom(true);
							}
						}}
						className="h-7 text-xs px-2 cursor-pointer"
					>
						{m.client_logs_scroll({
							state: autoScrollLogs ? (isAtBottom ? m.client_logs_on() : m.client_logs_paused()) : m.client_logs_off(),
						})}
					</Button>

					<Button
						variant="outline"
						size="xs"
						onClick={handleClearLogs}
						className="h-7 text-xs px-2 text-destructive hover:bg-destructive/10 cursor-pointer"
					>
						{m.client_logs_clear()}
					</Button>

					<Button
						variant="outline"
						size="xs"
						onClick={handleCopyAllLogs}
						className="h-7 text-xs px-2 gap-1 cursor-pointer"
					>
						{copied === "all-logs" ? (
							<Check className="h-3 w-3 text-emerald-500" />
						) : (
							<Copy className="h-3 w-3" />
						)}
						<span>{copied === "all-logs" ? m.common_copied() : m.client_logs_copy_all()}</span>
					</Button>
				</div>
			</div>

			{/* Terminal Window Viewport */}
			<div className="relative flex-1 min-h-0 flex flex-col rounded-xl border border-border bg-slate-950 overflow-hidden shadow-inner">
				<div
					ref={logsContainerRef}
					onScroll={handleLogsScroll}
					className="flex-1 min-h-0 overflow-y-auto p-3 font-mono text-[11px] leading-relaxed text-slate-200 selection:bg-primary/30 scrollbar-thin"
				>
					{filteredLogs.length > 0 ? (
						<div className="flex flex-col gap-1">
							{filteredLogs.map((entry, idx) => {
								const lvl = entry.level.toUpperCase();
								const badgeColor =
									lvl === "ERROR"
										? "text-red-400 bg-red-950/60 border-red-800/40"
										: lvl === "WARN"
											? "text-amber-400 bg-amber-950/60 border-amber-800/40"
											: lvl === "DEBUG"
												? "text-slate-400 bg-slate-900 border-slate-800"
												: "text-emerald-400 bg-emerald-950/60 border-emerald-800/40";

								return (
									<div
										key={idx}
										className="flex items-start gap-1.5 leading-relaxed hover:bg-white/5 px-1 py-0.5 rounded transition-colors"
									>
										<span className="shrink-0 text-slate-500 selection:text-slate-300 text-[10px]">
											[
											{entry.timestamp.length > 8
												? entry.timestamp.includes("T")
													? (entry.timestamp.split("T")[1]?.slice(0, 8) ?? entry.timestamp)
													: entry.timestamp
												: entry.timestamp}
											]
										</span>
										<span
											className={cn(
												"shrink-0 rounded px-1.5 py-0.2 text-[9px] font-bold border",
												badgeColor,
											)}
										>
											{entry.level}
										</span>
										<span className="shrink-0 text-slate-400 font-semibold">{entry.target}:</span>
										<span className="break-all text-slate-100">{entry.message}</span>
									</div>
								);
							})}
						</div>
					) : (
						<div className="flex h-full flex-col items-center justify-center text-slate-500 py-8">
							<Terminal className="mb-2 h-8 w-8 opacity-40" />
							<p className="text-xs">{m.client_logs_empty()}</p>
						</div>
					)}
				</div>

				{/* Floating Jump to Bottom Button */}
				{!isAtBottom && filteredLogs.length > 0 ? (
					<button
						type="button"
						onClick={() => {
							setAutoScrollLogs(true);
							scrollToBottom(true);
						}}
						className="absolute bottom-3 right-3 z-10 flex items-center gap-1 rounded-full bg-primary hover:bg-primary/90 text-primary-foreground px-3 py-1 text-xs font-medium shadow-lg transition-all duration-150 backdrop-blur cursor-pointer"
					>
						<ArrowDown className="h-3 w-3" />
						<span>{m.client_logs_latest()}</span>
					</button>
				) : null}
			</div>
		</div>
	);
}
