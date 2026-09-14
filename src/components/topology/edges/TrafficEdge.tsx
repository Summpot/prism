import { memo } from "react";
import { BaseEdge, EdgeLabelRenderer, type EdgeProps, getBezierPath } from "@xyflow/react";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { TrafficEdgeData } from "../types";

export const TrafficEdge = memo(function TrafficEdge({
	id,
	sourceX,
	sourceY,
	targetX,
	targetY,
	sourcePosition,
	targetPosition,
	style = {},
	markerEnd,
	data,
	selected,
}: EdgeProps & { data?: TrafficEdgeData }) {
	const [edgePath, labelX, labelY] = getBezierPath({
		sourceX,
		sourceY,
		sourcePosition,
		targetX,
		targetY,
		targetPosition,
	});

	const isActive = data?.active ?? false;
	const sessionCount = data?.sessionCount ?? 0;
	const bytes = data?.bytes;

	return (
		<>
			<BaseEdge
				id={id}
				path={edgePath}
				markerEnd={markerEnd}
				style={{
					...style,
					strokeWidth: selected ? 2.5 : isActive ? 2 : 1.5,
					stroke: selected ? "var(--primary)" : isActive ? "var(--primary)" : "var(--border)",
					strokeDasharray: isActive ? "5 5" : undefined,
					animation: isActive ? "traffic-dash 1.2s linear infinite" : undefined,
				}}
			/>

			{(sessionCount > 0 || data?.label || bytes !== undefined) && (
				<EdgeLabelRenderer>
					<div
						style={{
							position: "absolute",
							transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
							pointerEvents: "all",
						}}
						className={cn(
							"flex items-center gap-1 rounded-full border px-2 py-0.5 text-[9px] font-mono shadow-xs backdrop-blur-md transition-all select-none",
							selected
								? "border-primary bg-background text-primary ring-1 ring-primary"
								: isActive
									? "border-primary/40 bg-card/95 text-foreground font-semibold"
									: "border-border bg-card/85 text-muted-foreground",
						)}
					>
						{sessionCount > 0 ? (
							<>
								<span
									className={cn(
										"h-1.5 w-1.5 rounded-full",
										isActive ? "bg-emerald-500 animate-ping" : "bg-muted-foreground",
									)}
								/>
								<span>
									{sessionCount} {sessionCount === 1 ? "stream" : "streams"}
								</span>
							</>
						) : null}

						{bytes && bytes > 0 ? (
							<span className="text-muted-foreground">· {formatBytes(bytes)}</span>
						) : null}

						{data?.label ? <span>{data.label}</span> : null}
					</div>
				</EdgeLabelRenderer>
			)}
		</>
	);
});
