import { memo } from "react";
import { Handle, type NodeProps, Position } from "@xyflow/react";
import { Cable, Radio } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { ConnectorNodeData } from "../types";

export const ConnectorNode = memo(function ConnectorNode({
	data,
	selected,
}: NodeProps & { data: ConnectorNodeData }) {
	const isActive = data.activeSessionsCount > 0;

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
					<div className="flex h-7 w-7 items-center justify-center rounded-lg bg-indigo-500/15 text-indigo-600 dark:text-indigo-400">
						<Radio className="h-4 w-4" />
					</div>
					<div className="min-w-0">
						<div className="flex items-center gap-1.5">
							<span className="text-xs font-bold text-foreground truncate max-w-28">
								{data.connectorId}
							</span>
							{data.primary ? (
								<Badge
									variant="secondary"
									className="text-[9px] px-1 py-0 h-3.5 bg-primary/10 text-primary"
								>
									Primary
								</Badge>
							) : null}
						</div>
						<div className="text-[10px] text-muted-foreground truncate font-mono">
							{data.remoteAddr || "Direct Agent"}
						</div>
					</div>
				</div>

				<Badge variant="outline" className="text-[9px] px-1.5 py-0 h-4 font-mono">
					{data.servicesCount} {data.servicesCount === 1 ? "svc" : "svcs"}
				</Badge>
			</div>

			<div className="mt-2.5 flex items-center justify-between text-[10px] font-mono text-muted-foreground">
				<div className="flex items-center gap-1">
					<Cable className={cn("h-3 w-3", isActive ? "text-emerald-500" : "")} />
					<span>{data.activeSessionsCount} active streams</span>
				</div>
				<span className="text-[9px] uppercase tracking-wider text-muted-foreground">Tunnel</span>
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
