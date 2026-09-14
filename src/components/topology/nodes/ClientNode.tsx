import { memo } from "react";
import { Handle, type NodeProps, Position } from "@xyflow/react";
import { Activity, Globe, Users } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { ClientNodeData } from "../types";

export const ClientNode = memo(function ClientNode({
	data,
	selected,
}: NodeProps & { data: ClientNodeData }) {
	const isActive = data.sessionCount > 0;

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
							isActive ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground",
						)}
					>
						{data.sessionCount > 1 ? <Users className="h-4 w-4" /> : <Globe className="h-4 w-4" />}
					</div>
					<div className="min-w-0">
						<span className="text-xs font-bold text-foreground block truncate">{data.ip}</span>
						<span className="text-[10px] text-muted-foreground">
							{data.sessionCount} {data.sessionCount === 1 ? "Session" : "Sessions"}
						</span>
					</div>
				</div>
				<Badge
					variant={isActive ? "default" : "secondary"}
					className="text-[9px] px-1.5 py-0 h-4 font-mono uppercase"
				>
					{isActive ? "Active" : "Idle"}
				</Badge>
			</div>

			<div className="mt-2.5 flex items-center justify-between text-[10px] font-mono text-muted-foreground">
				<div className="flex items-center gap-1">
					<Activity className={cn("h-3 w-3", isActive ? "text-emerald-500 animate-pulse" : "")} />
					<span>{formatBytes(data.wireBytes || data.rawBytes)}</span>
				</div>
				<span className="text-[10px] text-muted-foreground">Inbound</span>
			</div>

			<Handle
				type="source"
				position={Position.Right}
				className="!w-2.5 !h-2.5 !bg-primary !border-2 !border-background transition-transform hover:!scale-125"
			/>
			<Handle
				type="target"
				position={Position.Left}
				className="!w-2.5 !h-2.5 !bg-muted-foreground !border-2 !border-background transition-transform hover:!scale-125"
			/>
		</div>
	);
});
