import { memo } from "react";
import { Handle, type NodeProps, Position } from "@xyflow/react";
import { Gamepad2, Unplug, Zap } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { formatBytes, formatPercentage } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { ServiceNodeData } from "../types";

export const ServiceNode = memo(function ServiceNode({
	data,
	selected,
}: NodeProps & { data: ServiceNodeData }) {
	const hasOptimizer = !!data.optimizerStats && data.optimizerStats.raw_bytes > 0;
	const isMinecraft =
		data.serviceName.toLowerCase().includes("mc") ||
		data.serviceName.toLowerCase().includes("minecraft") ||
		data.localAddr.includes("25565");

	return (
		<div
			className={cn(
				"relative w-64 rounded-xl border bg-card p-3 shadow-sm transition-all select-none hover:shadow-md",
				selected
					? "border-primary ring-2 ring-primary/30 shadow-primary/10"
					: "border-border hover:border-primary/50",
			)}
		>
			<div className="flex items-center justify-between gap-2 pb-2 border-b border-border/50">
				<div className="flex items-center gap-2 min-w-0">
					<div
						className={cn(
							"flex h-7 w-7 items-center justify-center rounded-lg text-xs font-semibold",
							isMinecraft
								? "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
								: "bg-amber-500/15 text-amber-600 dark:text-amber-400",
						)}
					>
						{isMinecraft ? <Gamepad2 className="h-4 w-4" /> : <Unplug className="h-4 w-4" />}
					</div>
					<div className="min-w-0">
						<span className="text-xs font-bold text-foreground block truncate">
							{data.serviceName}
						</span>
						<span className="text-[10px] text-muted-foreground font-mono truncate block">
							{data.localAddr}
						</span>
					</div>
				</div>

				<Badge variant="secondary" className="font-mono text-[9px] uppercase px-1.5 py-0 h-4">
					{data.proto}
				</Badge>
			</div>

			<div className="mt-2.5 flex items-center justify-between text-[10px] font-mono">
				{hasOptimizer ? (
					<div className="flex items-center gap-1 text-emerald-500 font-semibold truncate">
						<Zap className="h-3 w-3 flex-none" />
						<span>
							-{formatPercentage(data.optimizerStats?.saved_ratio ?? 0)} (
							{formatBytes(data.optimizerStats?.saved_bytes ?? 0)})
						</span>
					</div>
				) : (
					<span className="text-muted-foreground truncate">Direct Passthrough</span>
				)}

				<span className="text-[9px] font-mono text-muted-foreground/80 flex-none">
					{data.activeSessionsCount > 0 ? `${data.activeSessionsCount} active` : "Idle"}
				</span>
			</div>

			<Handle
				type="target"
				position={Position.Left}
				className="!w-2.5 !h-2.5 !bg-primary !border-2 !border-background transition-transform hover:!scale-125"
			/>
		</div>
	);
});
