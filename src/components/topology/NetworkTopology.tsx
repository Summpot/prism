import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	Background,
	BackgroundVariant,
	Controls,
	MiniMap,
	ReactFlow,
	type NodeMouseHandler,
	type NodeTypes,
	type EdgeTypes,
	useEdgesState,
	useNodesState,
} from "@xyflow/react";
import { Filter, Network } from "lucide-react";

import {
	CountChip,
	EmptyState,
	ErrorBanner,
	PageHeader,
	RefreshButton,
	SearchInput,
	ToggleChip,
} from "@/components/ui";
import {
	getConnections,
	getOptimizerStats,
	getTunnelServices,
	type ServiceSnapshot,
	type SessionInfo,
} from "@/lib/admin/adminApi";
import { useAdminQuery } from "@/hooks/useAdminQuery";
import type { PanelConnection } from "@/lib/panelConnection";
import { queryKeys } from "@/lib/state/queryKeys";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

import { ClientNode } from "./nodes/ClientNode";
import { ConnectorNode } from "./nodes/ConnectorNode";
import { GatewayNode } from "./nodes/GatewayNode";
import { ServiceNode } from "./nodes/ServiceNode";
import { TrafficEdge } from "./edges/TrafficEdge";
import { NodeInspectorDrawer } from "./NodeInspectorDrawer";
import { calculateTopologyLayout } from "./layout";
import type {
	ClientNodeData,
	ConnectorNodeData,
	GatewayNodeData,
	LayoutDirection,
	ServiceNodeData,
	TopologyCustomEdge,
	TopologyCustomNode,
	TrafficEdgeData,
} from "./types";

const nodeTypes: NodeTypes = {
	client: ClientNode as any,
	gateway: GatewayNode as any,
	connector: ConnectorNode as any,
	service: ServiceNode as any,
};

const edgeTypes: EdgeTypes = {
	traffic: TrafficEdge as any,
};

const EMPTY_CONNS: SessionInfo[] = [];
const EMPTY_SERVICES: ServiceSnapshot[] = [];

const FIT_VIEW_OPTIONS = { padding: 0.2 };
const PRO_OPTIONS = { hideAttribution: true };

const fetchConnections = (conn: PanelConnection) => getConnections(conn).catch(() => EMPTY_CONNS);
const fetchServices = (conn: PanelConnection) =>
	getTunnelServices(conn).catch(() => EMPTY_SERVICES);
const fetchOptimizer = (conn: PanelConnection) => getOptimizerStats(conn).catch(() => null);

export function NetworkTopology({ connection }: { connection: PanelConnection }) {
	const [autoRefresh, setAutoRefresh] = useState(true);
	const [direction, setDirection] = useState<LayoutDirection>("LR");
	const [activeOnly, setActiveOnly] = useState(false);
	const [query, setQuery] = useState("");
	const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

	const interval = autoRefresh ? 4_000 : false;

	// Query live status data
	const connsQuery = useAdminQuery(queryKeys.admin.connections(connection), fetchConnections, {
		refetchInterval: interval,
	});
	const servicesQuery = useAdminQuery(queryKeys.admin.services(connection), fetchServices, {
		refetchInterval: interval,
	});
	const optimizerQuery = useAdminQuery(queryKeys.admin.optimizer(connection), fetchOptimizer, {
		refetchInterval: interval,
	});

	const connsData = connsQuery.data ?? EMPTY_CONNS;
	const servicesData = servicesQuery.data ?? EMPTY_SERVICES;
	const optimizerData = optimizerQuery.data ?? null;

	const loading =
		(connsQuery.isFetching && !connsQuery.data) ||
		(servicesQuery.isFetching && !servicesQuery.data);

	const error =
		(connsQuery.errorMessage && !connsQuery.errorMessage.includes("method not negotiated")
			? connsQuery.errorMessage
			: null) ||
		(servicesQuery.errorMessage && !servicesQuery.errorMessage.includes("method not negotiated")
			? servicesQuery.errorMessage
			: null);

	const fetchData = () => {
		void connsQuery.refetch();
		void servicesQuery.refetch();
		void optimizerQuery.refetch();
	};

	// Generate nodes and edges from runtime data
	const { initialNodes, initialEdges } = useMemo(() => {
		const rawNodes: TopologyCustomNode[] = [];
		const rawEdges: TopologyCustomEdge[] = [];

		// 1. Group active connections by client host/IP
		const clientMap = new Map<string, SessionInfo[]>();
		for (const conn of connsData) {
			const hostPart = conn.client.includes(":")
				? conn.client.split(":")[0] || conn.client
				: conn.client;
			const list = clientMap.get(hostPart) ?? [];
			list.push(conn);
			clientMap.set(hostPart, list);
		}

		// 2. Gateway Node
		const gatewayId = "prism-gateway";

		const totalRaw = connsData.reduce((sum, c) => sum + (c.raw_bytes || 0), 0);
		const totalWire = connsData.reduce((sum, c) => sum + (c.wire_bytes || 0), 0);

		const gatewayNode: TopologyCustomNode = {
			id: gatewayId,
			type: "gateway",
			position: { x: 0, y: 0 },
			data: {
				type: "gateway",
				nodeId: "Prism Core Gateway",
				activeConnectionsCount: connsData.length,
				totalRawBytes: totalRaw,
				totalWireBytes: totalWire,
				globalOptimizer: optimizerData?.global,
			} satisfies GatewayNodeData,
		};
		rawNodes.push(gatewayNode);

		// 3. Client Nodes & Edges to Gateway
		let clientIdx = 0;
		for (const [clientIp, sessions] of clientMap.entries()) {
			const clientId = `client-${clientIdx++}`;
			const clientRaw = sessions.reduce((s, c) => s + (c.raw_bytes || 0), 0);
			const clientWire = sessions.reduce((s, c) => s + (c.wire_bytes || 0), 0);

			rawNodes.push({
				id: clientId,
				type: "client",
				position: { x: 0, y: 0 },
				data: {
					type: "client",
					clientId,
					ip: clientIp,
					sessionCount: sessions.length,
					rawBytes: clientRaw,
					wireBytes: clientWire,
					sessions,
				} satisfies ClientNodeData,
			});

			rawEdges.push({
				id: `edge-${clientId}-${gatewayId}`,
				source: clientId,
				target: gatewayId,
				type: "traffic",
				data: {
					active: sessions.length > 0,
					sessionCount: sessions.length,
					bytes: clientWire || clientRaw,
				} satisfies TrafficEdgeData,
			});
		}

		// 4. Group tunnel services by connector (client_id)
		const connectorMap = new Map<string, ServiceSnapshot[]>();
		for (const serviceSnap of servicesData) {
			const list = connectorMap.get(serviceSnap.client_id) ?? [];
			list.push(serviceSnap);
			connectorMap.set(serviceSnap.client_id, list);
		}

		// 5. Connector Nodes & Edges from Gateway
		let connectorIdx = 0;
		for (const [connectorIdKey, servicesList] of connectorMap.entries()) {
			const connectorNodeId = `connector-${connectorIdx++}`;
			const firstService = servicesList[0];

			// Count sessions flowing through this connector's services
			const connectorSessions = connsData.filter((c) =>
				servicesList.some(
					(s) =>
						s.service.name.toLowerCase() === (c.host || "").toLowerCase() ||
						(c.upstream || "").includes(s.service.name),
				),
			);

			rawNodes.push({
				id: connectorNodeId,
				type: "connector",
				position: { x: 0, y: 0 },
				data: {
					type: "connector",
					connectorId: connectorIdKey,
					remoteAddr: firstService?.remote || "Tunnel Agent",
					servicesCount: servicesList.length,
					primary: servicesList.some((s) => s.primary),
					services: servicesList,
					activeSessionsCount: connectorSessions.length,
				} satisfies ConnectorNodeData,
			});

			rawEdges.push({
				id: `edge-${gatewayId}-${connectorNodeId}`,
				source: gatewayId,
				target: connectorNodeId,
				type: "traffic",
				data: {
					active: connectorSessions.length > 0,
					sessionCount: connectorSessions.length,
					label: "Tunnel",
				} satisfies TrafficEdgeData,
			});

			// 6. Service Nodes under this connector
			for (const snap of servicesList) {
				const serviceNodeId = `svc-${snap.service.name}`;
				const serviceSessions = connsData.filter(
					(c) =>
						(c.host || "").toLowerCase() === snap.service.name.toLowerCase() ||
						(c.upstream || "").includes(snap.service.name),
				);
				const svcOptimizer = optimizerData?.services?.[snap.service.name];

				rawNodes.push({
					id: serviceNodeId,
					type: "service",
					position: { x: 0, y: 0 },
					data: {
						type: "service",
						serviceName: snap.service.name,
						proto: snap.service.proto,
						localAddr: snap.service.local_addr,
						remoteAddr: snap.service.remote_addr,
						masqueradeHost: snap.service.masquerade_host,
						routeOnly: snap.service.route_only,
						clientId: snap.client_id,
						primary: snap.primary,
						optimizerStats: svcOptimizer,
						activeSessionsCount: serviceSessions.length,
					} satisfies ServiceNodeData,
				});

				rawEdges.push({
					id: `edge-${connectorNodeId}-${serviceNodeId}`,
					source: connectorNodeId,
					target: serviceNodeId,
					type: "traffic",
					data: {
						active: serviceSessions.length > 0,
						sessionCount: serviceSessions.length,
						proto: snap.service.proto,
					} satisfies TrafficEdgeData,
				});
			}
		}

		return {
			initialNodes: rawNodes,
			initialEdges: rawEdges,
		};
	}, [connsData, nodesData, optimizerData, servicesData]);

	// Filter nodes based on user filter controls
	const filteredElements = useMemo(() => {
		const needle = query.trim().toLowerCase();

		let filteredNodes = initialNodes.filter((node) => {
			if (activeOnly) {
				if (node.data.type === "client" && node.data.sessionCount === 0) return false;
				if (node.data.type === "service" && node.data.activeSessionsCount === 0) return false;
				if (node.data.type === "connector" && node.data.activeSessionsCount === 0) return false;
			}

			if (!needle) return true;

			switch (node.data.type) {
				case "client":
					return node.data.ip.toLowerCase().includes(needle);
				case "gateway":
					return node.data.nodeId.toLowerCase().includes(needle);
				case "connector":
					return (
						node.data.connectorId.toLowerCase().includes(needle) ||
						node.data.remoteAddr.toLowerCase().includes(needle)
					);
				case "service":
					return (
						node.data.serviceName.toLowerCase().includes(needle) ||
						node.data.localAddr.toLowerCase().includes(needle) ||
						(node.data.remoteAddr ?? "").toLowerCase().includes(needle)
					);
				default:
					return true;
			}
		});

		// Always keep the gateway node so topology stays connected
		if (!filteredNodes.some((n) => n.data.type === "gateway")) {
			const gw = initialNodes.find((n) => n.data.type === "gateway");
			if (gw) filteredNodes.push(gw);
		}

		const visibleNodeIds = new Set(filteredNodes.map((n) => n.id));
		const filteredEdges = initialEdges.filter(
			(e) => visibleNodeIds.has(e.source) && visibleNodeIds.has(e.target),
		);

		return calculateTopologyLayout(filteredNodes, filteredEdges, direction);
	}, [activeOnly, direction, initialEdges, initialNodes, query]);

	const [nodes, setNodes, onNodesChange] = useNodesState<TopologyCustomNode>([]);
	const [edges, setEdges, onEdgesChange] = useEdgesState<TopologyCustomEdge>([]);

	const prevNodesRef = useRef<TopologyCustomNode[]>([]);
	const prevEdgesRef = useRef<TopologyCustomEdge[]>([]);
	const prevDirectionRef = useRef(direction);

	useEffect(() => {
		const directionChanged = prevDirectionRef.current !== direction;
		prevDirectionRef.current = direction;

		const targetNodes = filteredElements.nodes;
		const targetEdges = filteredElements.edges;

		const prevNodes = prevNodesRef.current;
		const prevEdges = prevEdgesRef.current;

		const nodesEqual =
			!directionChanged &&
			prevNodes.length === targetNodes.length &&
			prevNodes.every((pn, i) => {
				const tn = targetNodes[i];
				return pn.id === tn?.id && pn.data === tn?.data;
			});

		if (!nodesEqual) {
			prevNodesRef.current = targetNodes;
			setNodes((currentNodes) => {
				const posMap = directionChanged
					? null
					: new Map(currentNodes.map((n) => [n.id, n.position]));
				return targetNodes.map((node) => {
					const existingPos = posMap?.get(node.id);
					return existingPos ? { ...node, position: existingPos } : node;
				});
			});
		}

		const edgesEqual =
			prevEdges.length === targetEdges.length &&
			prevEdges.every((pe, i) => {
				const te = targetEdges[i];
				return pe.id === te?.id && pe.data === te?.data;
			});

		if (!edgesEqual) {
			prevEdgesRef.current = targetEdges;
			setEdges(targetEdges);
		}
	}, [direction, filteredElements.edges, filteredElements.nodes, setEdges, setNodes]);

	const onNodeClick: NodeMouseHandler = useCallback((_, node) => {
		setSelectedNodeId(node.id);
	}, []);

	const onPaneClick = useCallback(() => {
		setSelectedNodeId(null);
	}, []);

	const selectedNode = useMemo(() => {
		if (!selectedNodeId) return null;
		return nodes.find((n) => n.id === selectedNodeId) ?? null;
	}, [nodes, selectedNodeId]);

	return (
		<div className="flex h-full flex-1 min-h-0 flex-col space-y-4">
			<PageHeader
				eyebrow={m.topology_eyebrow()}
				title={m.topology_title()}
				description={m.topology_description()}
				actions={
					<>
						<ToggleChip active={autoRefresh} onClick={() => setAutoRefresh((v) => !v)}>
							{m.admin_auto_refresh({ state: autoRefresh ? m.admin_on() : m.admin_off() })}
						</ToggleChip>
						<RefreshButton onClick={fetchData} loading={loading} />
						<CountChip icon={<Network className="h-4 w-4" />}>
							{loading
								? m.common_loading()
								: m.topology_sessions_count({ count: connsData.length })}
						</CountChip>
					</>
				}
			/>

			{error ? <ErrorBanner message={error} onRetry={fetchData} /> : null}

			{/* Filter & Canvas Controls Bar */}
			<div className="flex flex-wrap items-center justify-between gap-2.5 rounded-xl border border-border bg-card p-2.5 shadow-xs">
				<div className="flex flex-1 items-center gap-2 min-w-64 max-w-md">
					<SearchInput
						value={query}
						onChange={setQuery}
						placeholder={m.topology_filter_placeholder()}
					/>
				</div>

				<div className="flex flex-wrap items-center gap-2">
					<div className="flex items-center rounded-lg border border-input p-0.5 text-xs">
						<button
							type="button"
							onClick={() => setDirection("LR")}
							className={cn(
								"rounded px-2 py-1 font-medium transition cursor-pointer text-xs",
								direction === "LR"
									? "bg-primary text-primary-foreground font-semibold"
									: "text-muted-foreground hover:text-foreground",
							)}
						>
							{m.topology_layout_horizontal()}
						</button>
						<button
							type="button"
							onClick={() => setDirection("TB")}
							className={cn(
								"rounded px-2 py-1 font-medium transition cursor-pointer text-xs",
								direction === "TB"
									? "bg-primary text-primary-foreground font-semibold"
									: "text-muted-foreground hover:text-foreground",
							)}
						>
							{m.topology_layout_vertical()}
						</button>
					</div>

					<ToggleChip active={activeOnly} onClick={() => setActiveOnly((v) => !v)}>
						<Filter className="h-3 w-3" />
						{activeOnly ? m.topology_filter_active() : m.topology_filter_all()}
					</ToggleChip>
				</div>
			</div>

			{/* Main Canvas Area */}
			<div className="relative flex-1 min-h-[500px] w-full overflow-hidden rounded-2xl border border-border bg-card shadow-sm">
				{nodes.length === 0 && !loading ? (
					<div className="flex h-full items-center justify-center p-8">
						<EmptyState
							title={m.topology_no_data()}
							description={m.topology_no_data_hint()}
							icon={<Network className="h-8 w-8 text-muted-foreground/60" />}
						/>
					</div>
				) : (
					<ReactFlow
						nodes={nodes}
						edges={edges}
						onNodesChange={onNodesChange}
						onEdgesChange={onEdgesChange}
						nodeTypes={nodeTypes}
						edgeTypes={edgeTypes}
						onNodeClick={onNodeClick}
						onPaneClick={onPaneClick}
						fitView
						fitViewOptions={FIT_VIEW_OPTIONS}
						minZoom={0.2}
						maxZoom={2}
						proOptions={PRO_OPTIONS}
					>
						<Background
							variant={BackgroundVariant.Dots}
							gap={20}
							size={1.5}
							className="opacity-60"
						/>
						<Controls showInteractive={false} className="!m-3 !border-border !bg-card !shadow-md" />
						<MiniMap
							nodeStrokeWidth={3}
							nodeColor={(node) => {
								switch (node.type) {
									case "client":
										return "var(--primary)";
									case "gateway":
										return "#6366f1";
									case "connector":
										return "#0ea5e9";
									case "service":
										return "#10b981";
									default:
										return "#64748b";
								}
							}}
							className="!m-3 !border-border !bg-card"
							maskColor="rgba(0, 0, 0, 0.2)"
						/>
					</ReactFlow>
				)}

				{/* Node Detail Inspector Drawer */}
				<NodeInspectorDrawer node={selectedNode} onClose={() => setSelectedNodeId(null)} />
			</div>
		</div>
	);
}
