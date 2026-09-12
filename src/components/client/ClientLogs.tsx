import { ArrowDown, Check, Copy, Search, Terminal } from "lucide-react";

import { useClientLogs } from "@/hooks/useClientLogs";
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
	} = useClientLogs();

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
					<div className="flex items-center rounded-lg border border-input p-0.5 text-[10px]">
						{(["ALL", "INFO", "WARN", "ERROR"] as const).map((lvl) => (
							<Button
								key={lvl}
								type="button"
								variant={logFilterLevel === lvl ? "default" : "ghost"}
								size="xs"
								onClick={() => setLogFilterLevel(lvl)}
								className="h-6 px-2 font-semibold"
							>
								{lvl}
							</Button>
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
							state: autoScrollLogs
								? isAtBottom
									? m.client_logs_on()
									: m.client_logs_paused()
								: m.client_logs_off(),
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
			<div className="relative flex-1 min-h-0 flex flex-col overflow-hidden rounded-xl border border-border bg-card shadow-xs">
				<div
					ref={logsContainerRef}
					onScroll={handleLogsScroll}
					className="flex-1 min-h-0 overflow-y-auto p-3 font-mono text-[11px] leading-relaxed text-foreground selection:bg-primary/30 scrollbar-thin"
				>
					{filteredLogs.length > 0 ? (
						<div className="flex flex-col gap-1">
							{filteredLogs.map((entry, idx) => {
								const lvl = entry.level.toUpperCase();
								const badgeColor =
									lvl === "ERROR"
										? "text-destructive bg-destructive/10 border-destructive/20"
										: lvl === "WARN"
											? "text-amber-600 dark:text-amber-400 bg-amber-500/10 border-amber-500/20"
											: lvl === "DEBUG"
												? "text-muted-foreground bg-muted border-border"
												: "text-emerald-600 dark:text-emerald-400 bg-emerald-500/10 border-emerald-500/20";

								return (
									<div
										key={idx}
										className="flex items-start gap-1.5 rounded px-1 py-0.5 leading-relaxed transition-colors hover:bg-muted/50"
									>
										<span className="shrink-0 text-[10px] text-muted-foreground">
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
												"shrink-0 rounded border px-1.5 py-0.2 text-[9px] font-bold",
												badgeColor,
											)}
										>
											{entry.level}
										</span>
										<span className="shrink-0 font-semibold text-muted-foreground">
											{entry.target}:
										</span>
										<span className="break-all text-foreground">{entry.message}</span>
									</div>
								);
							})}
						</div>
					) : (
						<div className="flex h-full flex-col items-center justify-center py-8 text-muted-foreground">
							<Terminal className="mb-2 h-8 w-8 opacity-40" />
							<p className="text-xs">{m.client_logs_empty()}</p>
						</div>
					)}
				</div>

				{!isAtBottom && filteredLogs.length > 0 ? (
					<Button
						type="button"
						size="xs"
						className="absolute right-3 bottom-3 z-10 rounded-full px-3 shadow-lg"
						onClick={() => {
							setAutoScrollLogs(true);
							scrollToBottom(true);
						}}
					>
						<ArrowDown className="h-3 w-3" />
						{m.client_logs_latest()}
					</Button>
				) : null}
			</div>
		</div>
	);
}
