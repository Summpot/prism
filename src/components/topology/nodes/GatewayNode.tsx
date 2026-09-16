import { memo } from "react";
import { Handle, type NodeProps, Position } from "@xyflow/react";
import { CheckCircle2, Server, Zap } from "lucide-react";
import { Badge } from "@/components/ui";
import { formatBytes, formatPercentage } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { GatewayNodeData } from "../types";

export const GatewayNode = memo(function GatewayNode({
	data,
	selected,
}: NodeProps & { data: GatewayNodeData }) {
	return (
		<div
			className={cn(
				"relative w-72 rounded-xl border bg-card p-3 shadow-md transition-all select-none hover:shadow-lg",
				selected
					? "border-primary ring-2 ring-primary/30 shadow-primary/15"
					: "border-border hover:border-primary/50",
			)}
		>
			<div className="flex items-center justify-between gap-2 pb-2 border-b border-border/50">
				<div className="flex items-center gap-2 min-w-0">
					<div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary text-primary-foreground font-bold transition-colors">
						<Server className="h-4 w-4" />
					</div>
					<div className="min-w-0">
						<div className="flex items-center gap-1.5">
							<span className="text-xs font-bold text-foreground truncate max-w-40">
								{data.nodeId}
							</span>
						</div>
						<div className="text-[10px] text-muted-foreground truncate font-mono">
							Prism Gateway
						</div>
					</div>
				</div>

				<div className="flex items-center gap-1">
					<Badge tone="ok" className="text-[9px] px-1 py-0 h-4 gap-0.5">
						<CheckCircle2 className="h-2.5 w-2.5" />
						Online
					</Badge>
				</div>
			</div>

			<div className="mt-2.5 grid grid-cols-2 gap-2 text-[10px] font-mono">
				<div className="rounded bg-muted/40 p-1.5">
					<div className="text-[9px] uppercase text-muted-foreground">Sessions</div>
					<div className="font-bold text-foreground text-xs">{data.activeConnectionsCount}</div>
				</div>
				<div className="rounded bg-muted/40 p-1.5">
					<div className="text-[9px] uppercase text-muted-foreground flex items-center gap-0.5">
						<Zap className="h-2.5 w-2.5 text-emerald-500" />
						Savings
					</div>
					<div className="font-bold text-emerald-500 text-xs truncate">
						{data.globalOptimizer && data.globalOptimizer.raw_bytes > 0
							? `${formatBytes(data.globalOptimizer.saved_bytes)} (${formatPercentage(data.globalOptimizer.saved_ratio)})`
							: "0 B"}
					</div>
				</div>
			</div>

			<Handle
				type="target"
				position={Position.Left}
				className="!w-2.5 !h-2.5 !bg-primary !border-2 !border-background transition-transform hover:!scale-125"
			/>
			<Handle
				type="source"
				position={Position.Right}
				className="!w-2.5 !h-2.5 !bg-primary !border-2 !border-background transition-transform hover:!scale-125"
			/>
		</div>
	);
});
