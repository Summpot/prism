import { Link } from "@tanstack/react-router";
import { ArrowRight, Globe, Radio, Server, Unplug, X, Zap } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { formatBytes, formatDuration, formatPercentage } from "@/lib/format";
import { m } from "@/paraglide/messages";
import type {
	ClientNodeData,
	ConnectorNodeData,
	GatewayNodeData,
	ServiceNodeData,
	TopologyCustomNode,
} from "./types";

interface NodeInspectorDrawerProps {
	node: TopologyCustomNode | null;
	onClose: () => void;
}

export function NodeInspectorDrawer({ node, onClose }: NodeInspectorDrawerProps) {
	if (!node) return null;

	const data = node.data;

	return (
		<div className="absolute right-0 top-0 bottom-0 z-20 w-full sm:w-96 max-w-full border-l border-border bg-card/95 backdrop-blur-md shadow-2xl flex flex-col transition-all duration-300 animate-in slide-in-from-right-10">
			{/* Drawer Header */}
			<div className="flex items-center justify-between p-4 border-b border-border/60">
				<div className="flex items-center gap-2 min-w-0">
					{renderNodeIcon(node)}
					<div className="min-w-0">
						<div className="flex items-center gap-1.5">
							<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4 uppercase font-mono">
								{getNodeTypeLabel(node)}
							</Badge>
						</div>
						<h3 className="text-sm font-bold text-foreground truncate mt-0.5">
							{getNodeTitle(node)}
						</h3>
					</div>
				</div>
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={onClose}
					className="h-7 w-7 text-muted-foreground hover:text-foreground cursor-pointer"
				>
					<X className="h-4 w-4" />
				</Button>
			</div>

			{/* Drawer Body */}
			<div className="flex-1 min-h-0 overflow-y-auto p-4 space-y-4 text-xs scrollbar-thin">
				{/* Node Specific Details */}
				{data.type === "client" && <ClientInspectorDetails data={data} />}
				{data.type === "gateway" && <GatewayInspectorDetails data={data} />}
				{data.type === "connector" && <ConnectorInspectorDetails data={data} />}
				{data.type === "service" && <ServiceInspectorDetails data={data} />}
			</div>
		</div>
	);
}

function renderNodeIcon(node: TopologyCustomNode) {
	switch (node.data.type) {
		case "client":
			return (
				<div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary flex-none">
					<Globe className="h-4 w-4" />
				</div>
			);
		case "gateway":
			return (
				<div className="flex h-8 w-8 items-center justify-center rounded-lg bg-indigo-500/15 text-indigo-600 flex-none">
					<Server className="h-4 w-4" />
				</div>
			);
		case "connector":
			return (
				<div className="flex h-8 w-8 items-center justify-center rounded-lg bg-sky-500/15 text-sky-600 flex-none">
					<Radio className="h-4 w-4" />
				</div>
			);
		case "service":
			return (
				<div className="flex h-8 w-8 items-center justify-center rounded-lg bg-emerald-500/15 text-emerald-600 flex-none">
					<Unplug className="h-4 w-4" />
				</div>
			);
	}
}

function getNodeTypeLabel(node: TopologyCustomNode): string {
	switch (node.data.type) {
		case "client":
			return m.topology_node_type_client();
		case "gateway":
			return m.topology_node_type_gateway();
		case "connector":
			return m.topology_node_type_connector();
		case "service":
			return m.topology_node_type_service();
	}
}

function getNodeTitle(node: TopologyCustomNode): string {
	switch (node.data.type) {
		case "client":
			return node.data.ip;
		case "gateway":
			return node.data.nodeId;
		case "connector":
			return node.data.connectorId;
		case "service":
			return node.data.serviceName;
	}
}

function ClientInspectorDetails({ data }: { data: ClientNodeData }) {
	return (
		<div className="space-y-4">
			<div className="rounded-lg border border-border bg-muted/30 p-3 space-y-2">
				<div className="text-[10px] uppercase font-bold text-muted-foreground">
					{m.topology_inspector_metrics()}
				</div>
				<div className="grid grid-cols-2 gap-2 font-mono">
					<div>
						<span className="text-[10px] text-muted-foreground block">Active Streams</span>
						<span className="text-sm font-bold text-foreground">{data.sessionCount}</span>
					</div>
					<div>
						<span className="text-[10px] text-muted-foreground block">Transfer Bytes</span>
						<span className="text-sm font-bold text-foreground">
							{formatBytes(data.wireBytes || data.rawBytes)}
						</span>
					</div>
				</div>
			</div>

			<div className="space-y-2">
				<div className="flex items-center justify-between">
					<span className="text-[11px] font-bold text-foreground">
						{m.topology_inspector_sessions()} ({data.sessions.length})
					</span>
					<Link
						to="/admin/connections"
						className="text-[10px] text-primary hover:underline inline-flex items-center gap-0.5"
					>
						<span>View all</span>
						<ArrowRight className="h-3 w-3" />
					</Link>
				</div>

				{data.sessions.length === 0 ? (
					<div className="rounded border border-dashed border-border p-4 text-center text-muted-foreground text-[11px]">
						{m.topology_inspector_no_sessions()}
					</div>
				) : (
					<div className="space-y-2">
						{data.sessions.map((sess) => (
							<div
								key={sess.id}
								className="rounded-lg border border-border bg-card p-2.5 font-mono text-[11px] space-y-1"
							>
								<div className="flex items-center justify-between">
									<span className="font-bold text-foreground truncate max-w-40">
										{sess.host || "Direct"}
									</span>
									<span className="text-[10px] text-muted-foreground">
										{formatDuration(sess.started_at_unix_ms)}
									</span>
								</div>
								<div className="text-muted-foreground truncate text-[10px]">➔ {sess.upstream}</div>
								<div className="flex items-center justify-between pt-1 border-t border-border/40 text-[10px] text-muted-foreground">
									<span>{formatBytes(sess.raw_bytes || 0)} raw</span>
									<span>{formatBytes(sess.wire_bytes || 0)} wire</span>
								</div>
							</div>
						))}
					</div>
				)}
			</div>
		</div>
	);
}

function GatewayInspectorDetails({ data }: { data: GatewayNodeData }) {
	return (
		<div className="space-y-4">
			<div className="rounded-lg border border-border bg-muted/30 p-3 space-y-2.5">
				<div className="flex items-center justify-between">
					<span className="text-[10px] uppercase font-bold text-muted-foreground">
						Prism Edge Gateway
					</span>
					<Badge tone="ok" className="text-[9px] px-1 py-0 h-4 font-mono">
						Online
					</Badge>
				</div>

				<div className="text-[11px] text-muted-foreground font-mono">
					Standalone Mode · Reverse Proxy & Multiplexed Tunnel Core
				</div>
			</div>

			<div className="rounded-lg border border-border bg-muted/30 p-3 space-y-2">
				<div className="text-[10px] uppercase font-bold text-muted-foreground">
					Global Optimizer Stats
				</div>
				{data.globalOptimizer && data.globalOptimizer.raw_bytes > 0 ? (
					<div className="space-y-1.5 font-mono text-[11px]">
						<div className="flex items-center justify-between">
							<span className="text-muted-foreground">Saved Bytes:</span>
							<span className="font-bold text-emerald-500">
								{formatBytes(data.globalOptimizer.saved_bytes)} (
								{formatPercentage(data.globalOptimizer.saved_ratio)})
							</span>
						</div>
						<div className="flex items-center justify-between">
							<span className="text-muted-foreground">Net Gain:</span>
							<span className="font-bold text-foreground">
								{data.globalOptimizer.net_gain_ms.toFixed(1)}ms
							</span>
						</div>
						<div className="flex items-center justify-between">
							<span className="text-muted-foreground">Link Rate:</span>
							<span className="font-bold text-foreground">
								{(data.globalOptimizer.link_rate_bps / 1_000_000).toFixed(1)} Mbps
							</span>
						</div>
					</div>
				) : (
					<div className="text-[10px] text-muted-foreground">No active compression stats.</div>
				)}
			</div>

			<div className="pt-2 flex flex-col gap-2">
				<Button variant="outline" size="sm" render={<Link to="/admin/connections" />}>
					<span>View Active Connections</span>
					<ArrowRight className="h-3.5 w-3.5" />
				</Button>
				<Button variant="outline" size="sm" render={<Link to="/admin/tunnel-services" />}>
					<span>View Tunnel Services</span>
					<ArrowRight className="h-3.5 w-3.5" />
				</Button>
			</div>
		</div>
	);
}

function ConnectorInspectorDetails({ data }: { data: ConnectorNodeData }) {
	return (
		<div className="space-y-4">
			<div className="rounded-lg border border-border bg-muted/30 p-3 space-y-2">
				<div className="text-[10px] uppercase font-bold text-muted-foreground">Connector Info</div>
				<div className="space-y-1 font-mono text-[11px]">
					<div className="flex items-center justify-between">
						<span className="text-muted-foreground">Remote:</span>
						<span className="font-bold text-foreground">{data.remoteAddr}</span>
					</div>
					<div className="flex items-center justify-between">
						<span className="text-muted-foreground">Role:</span>
						<Badge variant="secondary" className="text-[9px] px-1 py-0 h-3.5">
							{data.primary ? "Primary Host" : "Secondary Host"}
						</Badge>
					</div>
				</div>
			</div>

			<div className="space-y-2">
				<span className="text-[11px] font-bold text-foreground">
					Registered Services ({data.services.length})
				</span>
				<div className="space-y-2">
					{data.services.map((snapshot) => (
						<div
							key={snapshot.service.name}
							className="rounded-lg border border-border bg-card p-2.5 font-mono text-[11px] space-y-1"
						>
							<div className="flex items-center justify-between">
								<span className="font-bold text-foreground">{snapshot.service.name}</span>
								<Badge variant="outline" className="text-[9px] px-1 py-0 h-3.5 uppercase">
									{snapshot.service.proto}
								</Badge>
							</div>
							<div className="text-muted-foreground text-[10px]">
								Local: {snapshot.service.local_addr}
							</div>
							{snapshot.service.remote_addr ? (
								<div className="text-muted-foreground text-[10px]">
									Exposed: {snapshot.service.remote_addr}
								</div>
							) : null}
						</div>
					))}
				</div>
			</div>

			<div className="pt-2">
				<Button
					variant="outline"
					size="sm"
					className="w-full"
					render={<Link to="/admin/connectors" />}
				>
					<span>{m.topology_inspector_open_connector()}</span>
					<ArrowRight className="h-3.5 w-3.5" />
				</Button>
			</div>
		</div>
	);
}

function ServiceInspectorDetails({ data }: { data: ServiceNodeData }) {
	return (
		<div className="space-y-4">
			<div className="rounded-lg border border-border bg-muted/30 p-3 space-y-2">
				<div className="text-[10px] uppercase font-bold text-muted-foreground">Service Profile</div>
				<div className="space-y-1 font-mono text-[11px]">
					<div className="flex items-center justify-between">
						<span className="text-muted-foreground">Protocol:</span>
						<span className="font-bold text-foreground uppercase">{data.proto}</span>
					</div>
					<div className="flex items-center justify-between">
						<span className="text-muted-foreground">Local Endpoint:</span>
						<span className="font-bold text-foreground">{data.localAddr}</span>
					</div>
					{data.remoteAddr ? (
						<div className="flex items-center justify-between">
							<span className="text-muted-foreground">Public Listener:</span>
							<span className="font-bold text-foreground">{data.remoteAddr}</span>
						</div>
					) : null}
					{data.masqueradeHost ? (
						<div className="flex items-center justify-between">
							<span className="text-muted-foreground">Masquerade:</span>
							<span className="font-bold text-foreground">{data.masqueradeHost}</span>
						</div>
					) : null}
				</div>
			</div>

			{data.optimizerStats && data.optimizerStats.raw_bytes > 0 ? (
				<div className="rounded-lg border border-border bg-muted/30 p-3 space-y-2">
					<div className="text-[10px] uppercase font-bold text-muted-foreground flex items-center gap-1">
						<Zap className="h-3 w-3 text-emerald-500" />
						<span>Optimizer Lane Performance</span>
					</div>
					<div className="space-y-1 font-mono text-[11px]">
						<div className="flex items-center justify-between">
							<span className="text-muted-foreground">Saved:</span>
							<span className="font-bold text-emerald-500">
								{formatBytes(data.optimizerStats.saved_bytes)} (
								{formatPercentage(data.optimizerStats.saved_ratio)})
							</span>
						</div>
						<div className="flex items-center justify-between">
							<span className="text-muted-foreground">Wire / Raw:</span>
							<span className="text-foreground">
								{formatBytes(data.optimizerStats.wire_bytes)} /{" "}
								{formatBytes(data.optimizerStats.raw_bytes)}
							</span>
						</div>
					</div>
				</div>
			) : null}

			<div className="pt-2">
				<Button
					variant="outline"
					size="sm"
					className="w-full"
					render={<Link to="/admin/tunnel-services" />}
				>
					<span>View Tunnel Services</span>
					<ArrowRight className="h-3.5 w-3.5" />
				</Button>
			</div>
		</div>
	);
}
