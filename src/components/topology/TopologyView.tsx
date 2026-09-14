import {
	Activity,
	Cable,
	Cpu,
	GitFork,
	Laptop,
	Maximize2,
	Minus,
	Plus,
	RotateCcw,
	Server,
	Unplug,
} from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { formatBitsPerSecond } from "@/lib/format";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

import type {
	SelectedElement,
	TopologyNode,
	TopologyNodeType,
	TopologyPath,
} from "./topologyTypes";

interface TopologyViewProps {
	nodes: TopologyNode[];
	paths: TopologyPath[];
	selected: SelectedElement;
	onSelect: (element: SelectedElement) => void;
	canvasWidth: number;
	canvasHeight: number;
}

const LAYER_TITLES: Record<number, () => string> = {
	0: () => m.topology_layer_clients(),
	1: () => m.topology_layer_listeners(),
	2: () => m.topology_layer_routes(),
	3: () => m.topology_layer_core(),
	4: () => m.topology_layer_connectors(),
	5: () => m.topology_layer_upstreams(),
};

const NODE_WIDTH = 190;
const NODE_HEIGHT = 68;

function getNodeIcon(type: TopologyNodeType) {
	switch (type) {
		case "client":
			return <Laptop className="h-4 w-4 text-sky-500" />;
		case "listener":
			return <Cable className="h-4 w-4 text-amber-500" />;
		case "route":
			return <GitFork className="h-4 w-4 text-purple-500" />;
		case "core":
			return <Cpu className="h-4 w-4 text-emerald-500" />;
		case "connector":
			return <Unplug className="h-4 w-4 text-indigo-500" />;
		case "upstream":
			return <Server className="h-4 w-4 text-cyan-500" />;
		default:
			return <Activity className="h-4 w-4 text-muted-foreground" />;
	}
}

export function TopologyView({
	nodes,
	paths,
	selected,
	onSelect,
	canvasWidth,
	canvasHeight,
}: TopologyViewProps) {
	const containerRef = useRef<HTMLDivElement>(null);
	const [scale, setScale] = useState(1);
	const [pan, setPan] = useState({ x: 30, y: 30 });
	const [isDragging, setIsDragging] = useState(false);
	const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

	// Map nodes by id for quick path endpoint lookup
	const nodeMap = useRef(new Map<string, TopologyNode>());
	nodeMap.current = new Map(nodes.map((n) => [n.id, n]));

	// Fit canvas into container view initially
	useEffect(() => {
		if (containerRef.current) {
			const { clientWidth, clientHeight } = containerRef.current;
			if (clientWidth > 0 && clientHeight > 0) {
				const scaleX = (clientWidth - 60) / canvasWidth;
				const scaleY = (clientHeight - 60) / canvasHeight;
				const initialScale = Math.min(1, Math.max(0.4, Math.min(scaleX, scaleY)));
				setScale(initialScale);
				const initialX = Math.max(20, (clientWidth - canvasWidth * initialScale) / 2);
				const initialY = Math.max(20, (clientHeight - canvasHeight * initialScale) / 2);
				setPan({ x: initialX, y: initialY });
			}
		}
	}, [canvasWidth, canvasHeight]);

	// Mouse pan and zoom handlers
	const handleMouseDown = (e: React.MouseEvent) => {
		// Only pan if clicking on svg background or background wrapper
		if ((e.target as HTMLElement).closest(".topology-interactive")) {
			return;
		}
		setIsDragging(true);
		setDragStart({ x: e.clientX - pan.x, y: e.clientY - pan.y });
	};

	const handleMouseMove = (e: React.MouseEvent) => {
		if (!isDragging) return;
		setPan({
			x: e.clientX - dragStart.x,
			y: e.clientY - dragStart.y,
		});
	};

	const handleMouseUp = () => {
		setIsDragging(false);
	};

	const handleWheel = (e: React.WheelEvent) => {
		e.preventDefault();
		const zoomFactor = e.deltaY < 0 ? 1.08 : 0.92;
		setScale((prev) => Math.min(2.5, Math.max(0.3, prev * zoomFactor)));
	};

	const handleZoomIn = () => setScale((s) => Math.min(2.5, s * 1.2));
	const handleZoomOut = () => setScale((s) => Math.max(0.3, s * 0.8));
	const handleReset = () => {
		setScale(1);
		if (containerRef.current) {
			const { clientWidth, clientHeight } = containerRef.current;
			setPan({
				x: Math.max(20, (clientWidth - canvasWidth) / 2),
				y: Math.max(20, (clientHeight - canvasHeight) / 2),
			});
		} else {
			setPan({ x: 30, y: 30 });
		}
	};
	const handleFit = () => {
		if (containerRef.current) {
			const { clientWidth, clientHeight } = containerRef.current;
			const scaleX = (clientWidth - 60) / canvasWidth;
			const scaleY = (clientHeight - 60) / canvasHeight;
			const fitScale = Math.min(1.2, Math.max(0.35, Math.min(scaleX, scaleY)));
			setScale(fitScale);
			setPan({
				x: Math.max(20, (clientWidth - canvasWidth * fitScale) / 2),
				y: Math.max(20, (clientHeight - canvasHeight * fitScale) / 2),
			});
		}
	};

	// Determine highlighted IDs based on selected element
	const highlightedNodeIds = new Set<string>();
	const highlightedPathIds = new Set<string>();

	if (selected?.type === "node") {
		highlightedNodeIds.add(selected.node.id);
		paths.forEach((p) => {
			if (p.source === selected.node.id || p.target === selected.node.id) {
				highlightedPathIds.add(p.id);
				highlightedNodeIds.add(p.source);
				highlightedNodeIds.add(p.target);
			}
		});
	} else if (selected?.type === "path") {
		highlightedPathIds.add(selected.path.id);
		highlightedNodeIds.add(selected.path.source);
		highlightedNodeIds.add(selected.path.target);
	}

	return (
		<div
			ref={containerRef}
			className="relative h-[650px] w-full select-none overflow-hidden rounded-xl border border-border bg-card/40 backdrop-blur shadow-inner cursor-grab active:cursor-grabbing"
			onMouseDown={handleMouseDown}
			onMouseMove={handleMouseMove}
			onMouseUp={handleMouseUp}
			onMouseLeave={handleMouseUp}
			onWheel={handleWheel}
		>
			{/* Grid Pattern Background */}
			<div
				className="absolute inset-0 pointer-events-none opacity-[0.03] dark:opacity-[0.05]"
				style={{
					backgroundImage: `radial-gradient(circle, currentColor 1px, transparent 1px)`,
					backgroundSize: "24px 24px",
				}}
			/>

			{/* Floating Canvas Controls */}
			<div className="absolute top-3 right-3 z-20 flex items-center gap-1.5 rounded-lg border border-border/80 bg-card/90 p-1 backdrop-blur shadow-sm">
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={handleZoomIn}
					title={m.topology_zoom_in()}
					className="text-muted-foreground hover:text-foreground"
				>
					<Plus className="h-3.5 w-3.5" />
				</Button>
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={handleZoomOut}
					title={m.topology_zoom_out()}
					className="text-muted-foreground hover:text-foreground"
				>
					<Minus className="h-3.5 w-3.5" />
				</Button>
				<div className="h-4 w-px bg-border/80 mx-0.5" />
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={handleFit}
					title={m.topology_zoom_fit()}
					className="text-muted-foreground hover:text-foreground"
				>
					<Maximize2 className="h-3.5 w-3.5" />
				</Button>
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={handleReset}
					title={m.topology_zoom_reset()}
					className="text-muted-foreground hover:text-foreground"
				>
					<RotateCcw className="h-3.5 w-3.5" />
				</Button>
				<span className="px-1.5 text-[10px] font-mono text-muted-foreground">
					{Math.round(scale * 100)}%
				</span>
			</div>

			{/* SVG Network Canvas */}
			<svg
				width={canvasWidth}
				height={canvasHeight}
				className="absolute overflow-visible"
				style={{
					transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
					transformOrigin: "0 0",
				}}
			>
				<defs>
					{/* Flowing particle animation for active paths */}
					<style>
						{`
							@keyframes flowDash {
								from { stroke-dashoffset: 24; }
								to { stroke-dashoffset: 0; }
							}
							.animate-traffic-flow {
								animation: flowDash 1.2s linear infinite;
							}
							.animate-traffic-fast {
								animation: flowDash 0.7s linear infinite;
							}
						`}
					</style>

					{/* Standard Arrow Marker */}
					<marker
						id="arrow-end"
						viewBox="0 0 10 10"
						refX="8"
						refY="5"
						markerWidth="6"
						markerHeight="6"
						orient="auto-start-reverse"
					>
						<path
							d="M 0 1 L 10 5 L 0 9 z"
							fill="currentColor"
							className="text-muted-foreground/50"
						/>
					</marker>

					{/* Active Glowing Arrow Marker */}
					<marker
						id="arrow-end-active"
						viewBox="0 0 10 10"
						refX="8"
						refY="5"
						markerWidth="7"
						markerHeight="7"
						orient="auto-start-reverse"
					>
						<path d="M 0 1 L 10 5 L 0 9 z" fill="currentColor" className="text-emerald-500" />
					</marker>
				</defs>

				{/* Layer Column Backgrounds & Headers */}
				{[0, 1, 2, 3, 4, 5].map((layer) => {
					const x =
						layer === 0
							? 70
							: layer === 1
								? 330
								: layer === 2
									? 590
									: layer === 3
										? 850
										: layer === 4
											? 1110
											: 1370;
					const title = LAYER_TITLES[layer] ? LAYER_TITLES[layer]() : `Layer ${layer}`;
					return (
						<g key={`layer-header-${layer}`}>
							<rect
								x={x - 10}
								y={10}
								width={NODE_WIDTH + 20}
								height={26}
								rx={6}
								className="fill-muted/30 stroke-border/40"
							/>
							<text
								x={x + NODE_WIDTH / 2}
								y={27}
								textAnchor="middle"
								className="text-[11px] font-semibold tracking-wide fill-muted-foreground uppercase"
							>
								{title}
							</text>
						</g>
					);
				})}

				{/* Render Paths (Edges) */}
				{paths.map((path) => {
					const source = nodeMap.current.get(path.source);
					const target = nodeMap.current.get(path.target);
					if (!source || !target) return null;

					const isSelected = selected?.type === "path" && selected.path.id === path.id;
					const isHighlighted = highlightedPathIds.has(path.id);
					const hasTraffic = path.active || path.uplinkBps > 0 || path.downlinkBps > 0;
					const isFast = path.uplinkBps + path.downlinkBps > 500_000; // > 500 kbps

					// Start point at right-center of source node card
					const x1 = source.x + NODE_WIDTH;
					const y1 = source.y + NODE_HEIGHT / 2;

					// End point at left-center of target node card
					const x2 = target.x;
					const y2 = target.y + NODE_HEIGHT / 2;

					// Cubic Bézier control points
					const dx = Math.abs(x2 - x1);
					const c1x = x1 + dx * 0.48;
					const c1y = y1;
					const c2x = x2 - dx * 0.48;
					const c2y = y2;

					const pathD = `M ${x1} ${y1} C ${c1x} ${c1y}, ${c2x} ${c2y}, ${x2} ${y2}`;
					const midX = (x1 + x2) / 2;
					const midY = (y1 + y2) / 2;

					const totalBps = path.uplinkBps + path.downlinkBps;

					return (
						<g
							key={path.id}
							className="topology-interactive cursor-pointer group"
							onClick={(e) => {
								e.stopPropagation();
								onSelect({
									type: "path",
									path,
									sourceNode: source,
									targetNode: target,
								});
							}}
						>
							{/* Invisible wider hit area for easy mouse clicking */}
							<path d={pathD} fill="none" stroke="transparent" strokeWidth={20} />

							{/* Base Path Line */}
							<path
								d={pathD}
								fill="none"
								className={cn(
									"transition-colors duration-200",
									isSelected
										? "stroke-primary"
										: isHighlighted
											? "stroke-primary/80"
											: hasTraffic
												? "stroke-emerald-500/70"
												: "stroke-border/70 group-hover:stroke-muted-foreground/60",
								)}
								strokeWidth={isSelected ? 3 : isHighlighted ? 2.5 : hasTraffic ? 2 : 1.5}
								markerEnd={hasTraffic ? "url(#arrow-end-active)" : "url(#arrow-end)"}
							/>

							{/* Animated Flowing Particles / Dashes for Active Traffic */}
							{hasTraffic ? (
								<path
									d={pathD}
									fill="none"
									strokeDasharray="6 6"
									className={cn(
										"stroke-emerald-400 dark:stroke-emerald-300 pointer-events-none",
										isFast ? "animate-traffic-fast" : "animate-traffic-flow",
									)}
									strokeWidth={isSelected ? 3 : isFast ? 2.5 : 2}
								/>
							) : null}

							{/* Path Traffic Badge Pill (if carrying rate or selected) */}
							{hasTraffic || totalBps > 0 || isSelected ? (
								<foreignObject
									x={midX - 70}
									y={midY - 14}
									width={140}
									height={28}
									className="overflow-visible pointer-events-none"
								>
									<div className="flex items-center justify-center h-full w-full">
										<div
											className={cn(
												"inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-mono font-medium shadow-xs backdrop-blur border transition-all",
												isSelected
													? "bg-primary text-primary-foreground border-primary"
													: hasTraffic
														? "bg-emerald-950/80 text-emerald-300 border-emerald-500/40"
														: "bg-muted/90 text-muted-foreground border-border",
											)}
										>
											{totalBps > 0 ? (
												<span>{formatBitsPerSecond(totalBps)}</span>
											) : (
												<span>{path.activeSessions} conns</span>
											)}
											{path.savedRatio && path.savedRatio > 0 ? (
												<span className="text-[9px] text-emerald-400">
													-{(path.savedRatio * 100).toFixed(0)}%
												</span>
											) : null}
										</div>
									</div>
								</foreignObject>
							) : null}
						</g>
					);
				})}

				{/* Render Nodes */}
				{nodes.map((node) => {
					const isSelected = selected?.type === "node" && selected.node.id === node.id;
					const isHighlighted = highlightedNodeIds.has(node.id);
					const isActive = node.activeSessions > 0 || node.status === "active";

					return (
						<foreignObject
							key={node.id}
							x={node.x}
							y={node.y}
							width={NODE_WIDTH}
							height={NODE_HEIGHT}
							className="overflow-visible"
						>
							<div
								className={cn(
									"topology-interactive h-full w-full rounded-xl border bg-card/95 p-2.5 backdrop-blur shadow-sm cursor-pointer transition-all duration-200 select-none flex flex-col justify-between",
									isSelected
										? "ring-2 ring-primary border-primary bg-accent/30 shadow-md scale-[1.02]"
										: isHighlighted
											? "ring-1 ring-primary/60 border-primary/50"
											: "border-border/80 hover:border-primary/50 hover:bg-accent/10 hover:shadow-xs",
								)}
								onClick={(e) => {
									e.stopPropagation();
									onSelect({ type: "node", node });
								}}
							>
								{/* Top Row: Icon + Label + Status Dot */}
								<div className="flex items-center justify-between gap-1.5 min-w-0">
									<div className="flex items-center gap-2 min-w-0 flex-1">
										<div className="flex-none p-1 rounded-md bg-muted/60">
											{getNodeIcon(node.type)}
										</div>
										<span
											className="text-xs font-semibold text-foreground truncate"
											title={node.label}
										>
											{node.label}
										</span>
									</div>

									{/* Status Indicator */}
									<div className="flex items-center flex-none">
										<span className="relative flex h-2 w-2">
											{isActive ? (
												<span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" />
											) : null}
											<span
												className={cn(
													"relative inline-flex rounded-full h-2 w-2",
													isActive ? "bg-emerald-500" : "bg-muted-foreground/40",
												)}
											/>
										</span>
									</div>
								</div>

								{/* Bottom Row: Sublabel + Protocol Badge + Active Sessions Badge */}
								<div className="flex items-center justify-between gap-1.5 text-[10px] text-muted-foreground pt-1 border-t border-border/40">
									<span className="truncate font-mono" title={node.sublabel}>
										{node.sublabel || "—"}
									</span>

									<div className="flex items-center gap-1 flex-none">
										{node.protocol ? (
											<span className="rounded px-1 py-0.2 uppercase font-mono text-[9px] bg-muted text-muted-foreground">
												{node.protocol}
											</span>
										) : null}
										{node.activeSessions > 0 ? (
											<span className="rounded px-1 py-0.2 font-mono text-[9px] font-semibold bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
												{node.activeSessions}
											</span>
										) : null}
									</div>
								</div>
							</div>
						</foreignObject>
					);
				})}
			</svg>
		</div>
	);
}
