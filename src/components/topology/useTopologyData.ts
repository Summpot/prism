import { useEffect, useMemo, useRef, useState } from "react";

import { useAdminQuery } from "@/hooks/useAdminQuery";
import {
	getConnections,
	getManagedNodes,
	getOptimizerStats,
	getTunnelServices,
	listMiddlewares,
	type ManagedNodeSnapshot,
	type MiddlewareItem,
	type OptimizerOverviewResponse,
	type ServiceSnapshot,
	type SessionInfo,
} from "@/lib/managementApi";
import type { PanelConnection } from "@/lib/panelConnection";
import { queryKeys } from "@/lib/state/queryKeys";

import type {
	TopologyFilter,
	TopologyNode,
	TopologyNodeType,
	TopologyPath,
	TopologySummary,
} from "./topologyTypes";

const LAYER_X: Record<number, number> = {
	0: 80, // Clients
	1: 340, // Listeners
	2: 600, // Routes / Middlewares
	3: 860, // Prism Core / Cluster Nodes
	4: 1120, // Connectors
	5: 1380, // Upstreams / Services
};

const DEFAULT_HEIGHT = 650;
const NODE_SPACING_Y = 110;

interface PathRateSnapshot {
	timestamp: number;
	rawBytes: number;
	wireBytes: number;
	uplinkBytes: number;
	downlinkBytes: number;
}

export function useTopologyData(
	connection: PanelConnection | null,
	interval: number | false = 3000,
	filter: TopologyFilter = { protocol: "all", activeOnly: false, searchQuery: "" },
) {
	const prevSnapshotsRef = useRef<Map<string, PathRateSnapshot>>(new Map());
	const [calculatedRates, setCalculatedRates] = useState<
		Map<string, { uplinkBps: number; downlinkBps: number }>
	>(new Map());

	// 1. Fetch live connections
	const connsQuery = useAdminQuery(
		queryKeys.admin.connections(connection),
		(conn) => getConnections(conn).catch(() => [] as SessionInfo[]),
		{ refetchInterval: interval },
	);

	// 2. Fetch tunnel services
	const servicesQuery = useAdminQuery(
		queryKeys.admin.services(connection),
		(conn) => getTunnelServices(conn).catch(() => [] as ServiceSnapshot[]),
		{ refetchInterval: interval },
	);

	// 3. Fetch optimizer metrics
	const optimizerQuery = useAdminQuery(
		queryKeys.admin.optimizer(connection),
		(conn) => getOptimizerStats(conn).catch(() => null as OptimizerOverviewResponse | null),
		{ refetchInterval: interval },
	);

	// 4. Fetch managed cluster nodes (if any)
	const nodesQuery = useAdminQuery(
		queryKeys.admin.nodes(connection),
		(conn) => getManagedNodes(conn).catch(() => [] as ManagedNodeSnapshot[]),
		{ refetchInterval: interval ? interval * 2 : false },
	);

	// 5. Fetch loaded middlewares
	const middlewaresQuery = useAdminQuery(
		["admin", "middlewares", connection?.baseUrl],
		(conn) => listMiddlewares(conn).catch(() => [] as MiddlewareItem[]),
		{ refetchInterval: interval ? interval * 4 : false },
	);

	const sessions = connsQuery.data ?? [];
	const tunnelServices = servicesQuery.data ?? [];
	const optimizerStats = optimizerQuery.data ?? null;
	const clusterNodes = nodesQuery.data ?? [];
	const middlewares = middlewaresQuery.data ?? [];

	const loading =
		connsQuery.isFetching && !connsQuery.data && servicesQuery.isFetching && !servicesQuery.data;
	const error = connsQuery.errorMessage || servicesQuery.errorMessage;

	const refetchAll = () => {
		void connsQuery.refetch();
		void servicesQuery.refetch();
		void optimizerQuery.refetch();
		void nodesQuery.refetch();
		void middlewaresQuery.refetch();
	};

	// Calculate rate deltas on each polling update
	useEffect(() => {
		const now = Date.now();
		const currentSnapshots = new Map<string, PathRateSnapshot>();
		const newRates = new Map<string, { uplinkBps: number; downlinkBps: number }>();

		// Aggregate current session bytes by path key (client->listener, listener->route, etc.)
		sessions.forEach((s) => {
			const raw = s.raw_bytes ?? 0;
			const wire = s.wire_bytes ?? raw;
			const up = s.uplink_raw_bytes ?? Math.floor(raw / 2);
			const down = s.downlink_raw_bytes ?? Math.floor(raw / 2);

			const sid = s.id;
			currentSnapshots.set(sid, {
				timestamp: now,
				rawBytes: raw,
				wireBytes: wire,
				uplinkBytes: up,
				downlinkBytes: down,
			});

			const prev = prevSnapshotsRef.current.get(sid);
			if (prev && now > prev.timestamp) {
				const deltaSec = Math.max(0.5, (now - prev.timestamp) / 1000);
				const deltaUp = Math.max(0, up - prev.uplinkBytes);
				const deltaDown = Math.max(0, down - prev.downlinkBytes);
				newRates.set(sid, {
					uplinkBps: Math.round((deltaUp * 8) / deltaSec),
					downlinkBps: Math.round((deltaDown * 8) / deltaSec),
				});
			} else {
				newRates.set(sid, { uplinkBps: 0, downlinkBps: 0 });
			}
		});

		prevSnapshotsRef.current = currentSnapshots;
		setCalculatedRates(newRates);
	}, [sessions]);

	// Build the graph model (Nodes & Paths)
	const { nodes, paths, summary, canvasWidth, canvasHeight } = useMemo(() => {
		const nodeMap = new Map<string, TopologyNode>();
		const pathMap = new Map<string, TopologyPath>();

		// Helper to add or retrieve node
		const ensureNode = (
			id: string,
			label: string,
			type: TopologyNodeType,
			layer: number,
			props: Partial<TopologyNode> = {},
		): TopologyNode => {
			let node = nodeMap.get(id);
			if (!node) {
				node = {
					id,
					label,
					type,
					layer,
					status: "idle",
					activeSessions: 0,
					x: LAYER_X[layer] ?? 0,
					y: 0,
					...props,
				};
				nodeMap.set(id, node);
			} else {
				if (props.protocol && !node.protocol) node.protocol = props.protocol;
				if (props.sublabel && !node.sublabel) node.sublabel = props.sublabel;
				if (props.details) node.details = { ...node.details, ...props.details };
			}
			return node;
		};

		// Helper to add or retrieve edge/path
		const addPath = (
			sourceId: string,
			targetId: string,
			session?: SessionInfo,
			protocol?: string,
			customRate?: { uplinkBps: number; downlinkBps: number },
		): TopologyPath => {
			const pathId = `${sourceId}->${targetId}`;
			let path = pathMap.get(pathId);
			if (!path) {
				path = {
					id: pathId,
					source: sourceId,
					target: targetId,
					protocol: protocol ?? "tcp",
					activeSessions: 0,
					uplinkBps: 0,
					downlinkBps: 0,
					totalRawBytes: 0,
					totalWireBytes: 0,
					sessions: [],
					active: false,
				};
				pathMap.set(pathId, path);
			}

			if (session) {
				path.sessions.push(session);
				path.activeSessions += 1;
				path.totalRawBytes += session.raw_bytes ?? 0;
				path.totalWireBytes += session.wire_bytes ?? session.raw_bytes ?? 0;

				const rates = customRate || calculatedRates.get(session.id);
				if (rates) {
					path.uplinkBps += rates.uplinkBps;
					path.downlinkBps += rates.downlinkBps;
				}
				if (path.activeSessions > 0 || path.uplinkBps > 0 || path.downlinkBps > 0) {
					path.active = true;
				}
			}

			return path;
		};

		// 1. Setup Central Core Nodes
		const coreNode = ensureNode("prism:core", "Prism Core", "core", 3, {
			sublabel: "Reverse Proxy & Tunnel",
			status: "active",
			protocol: "l4",
		});

		// If cluster nodes exist, add them into Core layer
		clusterNodes.forEach((cn) => {
			ensureNode(`node:${cn.node_id}`, cn.node_id, "core", 3, {
				sublabel: cn.agent_url || "Cluster Worker",
				status: cn.pending_restart ? "idle" : "active",
				details: { ...cn },
			});
			// Connect Prism Core to cluster workers
			addPath("prism:core", `node:${cn.node_id}`, undefined, "control");
		});

		// 2. Setup Tunnel Services & Connectors (Registered Services)
		tunnelServices.forEach((ts) => {
			const svcName = ts.service.name;
			const svcProto = ts.service.proto.toLowerCase();
			const clientId = ts.client_id || "connector";
			const connectorId = `connector:${clientId}`;
			const serviceNodeId = `service:${svcName}`;

			// Connector Node (Layer 4)
			ensureNode(connectorId, clientId, "connector", 4, {
				sublabel: ts.remote || "Edge Agent",
				protocol: svcProto,
				status: "active",
				details: {
					clientId,
					remote: ts.remote,
					primary: ts.primary,
				},
			});

			// Target Service Node (Layer 5)
			const serviceOpt = optimizerStats?.services[svcName];
			ensureNode(serviceNodeId, svcName, "upstream", 5, {
				sublabel: ts.service.local_addr,
				protocol: svcProto,
				status: "active",
				details: {
					...ts.service,
					optimizer: serviceOpt,
				},
			});

			// Path: Core -> Connector
			const coreToConn = addPath("prism:core", connectorId, undefined, "tunnel");
			if (serviceOpt) {
				coreToConn.savedRatio = serviceOpt.saved_ratio;
				coreToConn.netGainMs = serviceOpt.net_gain_ms;
				if (serviceOpt.link_rate_bps > 0) {
					coreToConn.uplinkBps = Math.max(
						coreToConn.uplinkBps,
						Math.round(serviceOpt.link_rate_bps),
					);
				}
			}

			// Path: Connector -> Target Service
			addPath(connectorId, serviceNodeId, undefined, svcProto);

			// Inbound Listener for this service (if remote_addr specified)
			if (ts.service.remote_addr && !ts.service.route_only) {
				const lnId = `listener:${ts.service.remote_addr}`;
				ensureNode(lnId, ts.service.remote_addr, "listener", 1, {
					sublabel: `Auto-listen (${svcProto.toUpperCase()})`,
					protocol: svcProto,
					status: "active",
				});
				// Listener -> Core
				addPath(lnId, "prism:core", undefined, svcProto);
			}
		});

		// 3. Process Live Sessions
		sessions.forEach((s) => {
			const clientEndpoint = s.client || "unknown:0";
			const [clientHost, clientPort] = clientEndpoint.split(":");
			const clientNodeId = `client:${clientHost || clientEndpoint}`;

			// Client Node (Layer 0)
			const clientNode = ensureNode(clientNodeId, clientHost || "Client", "client", 0, {
				sublabel: clientPort ? `:${clientPort}` : undefined,
				status: "active",
			});
			clientNode.activeSessions += 1;
			clientNode.status = "active";

			// Determine Inbound Listener (Layer 1)
			let listenerAddr = ":25565";
			let proto = "tcp";
			if (s.upstream.startsWith("tunnel:")) {
				const svcName = s.upstream.slice(7);
				const matchedSvc = tunnelServices.find((t) => t.service.name === svcName);
				if (matchedSvc?.service.remote_addr) {
					listenerAddr = matchedSvc.service.remote_addr;
				}
				if (matchedSvc?.service.proto) {
					proto = matchedSvc.service.proto;
				}
			}
			const listenerId = `listener:${listenerAddr}`;
			const listenerNode = ensureNode(listenerId, listenerAddr, "listener", 1, {
				sublabel: `${proto.toUpperCase()} Port`,
				protocol: proto,
				status: "active",
			});
			listenerNode.activeSessions += 1;
			listenerNode.status = "active";

			// Determine Route & Middleware (Layer 2)
			const routeHost = s.host ? s.host : "default";
			const routeId = `route:${routeHost}`;
			const matchedMw = middlewares.map((m) => m.name).join(", ");
			const routeNode = ensureNode(routeId, routeHost, "route", 2, {
				sublabel: matchedMw ? `mw: ${matchedMw}` : "Direct Route",
				protocol: proto,
				status: "active",
			});
			routeNode.activeSessions += 1;
			routeNode.status = "active";

			// Core Node update
			coreNode.activeSessions += 1;

			// Determine Target Upstream / Connector (Layer 4 & 5)
			let targetNodeId = "";
			if (s.upstream.startsWith("tunnel:")) {
				const svcName = s.upstream.slice(7);
				targetNodeId = `service:${svcName}`;
				const matchedSvc = tunnelServices.find((t) => t.service.name === svcName);
				const connectorId = matchedSvc ? `connector:${matchedSvc.client_id}` : "connector:default";

				const connNode = ensureNode(
					connectorId,
					matchedSvc?.client_id || "Connector",
					"connector",
					4,
					{
						protocol: "tunnel",
						status: "active",
					},
				);
				connNode.activeSessions += 1;

				const targetSvcNode = ensureNode(targetNodeId, svcName, "upstream", 5, {
					sublabel: matchedSvc?.service.local_addr || "tunnel service",
					protocol: proto,
					status: "active",
				});
				targetSvcNode.activeSessions += 1;

				// Paths:
				// Client -> Listener
				addPath(clientNodeId, listenerId, s, proto);
				// Listener -> Route
				addPath(listenerId, routeId, s, proto);
				// Route -> Core
				addPath(routeId, "prism:core", s, proto);
				// Core -> Connector
				addPath("prism:core", connectorId, s, "tunnel");
				// Connector -> Target Service
				addPath(connectorId, targetNodeId, s, proto);
			} else {
				// Direct Upstream
				targetNodeId = `upstream:${s.upstream || "127.0.0.1"}`;
				const upstreamNode = ensureNode(targetNodeId, s.upstream || "Upstream", "upstream", 5, {
					sublabel: "Backend Server",
					protocol: proto,
					status: "active",
				});
				upstreamNode.activeSessions += 1;

				// Paths:
				// Client -> Listener
				addPath(clientNodeId, listenerId, s, proto);
				// Listener -> Route
				addPath(listenerId, routeId, s, proto);
				// Route -> Core
				addPath(routeId, "prism:core", s, proto);
				// Core -> Direct Upstream
				addPath("prism:core", targetNodeId, s, proto);
			}
		});

		// Fallback default listener if completely empty
		if (nodeMap.size <= 1) {
			const lnId = "listener::25565";
			ensureNode(lnId, ":25565", "listener", 1, {
				sublabel: "TCP Port",
				protocol: "tcp",
			});
			ensureNode("route:default", "Default Route", "route", 2, {
				sublabel: "L4 Passthrough",
				protocol: "tcp",
			});
			addPath(lnId, "route:default", undefined, "tcp");
			addPath("route:default", "prism:core", undefined, "tcp");
		}

		// Apply Filters
		let filteredNodesList = Array.from(nodeMap.values());
		let filteredPathsList = Array.from(pathMap.values());

		if (filter.protocol !== "all") {
			filteredPathsList = filteredPathsList.filter(
				(p) => !p.protocol || p.protocol.toLowerCase().includes(filter.protocol),
			);
			const validNodeIds = new Set<string>();
			filteredPathsList.forEach((p) => {
				validNodeIds.add(p.source);
				validNodeIds.add(p.target);
			});
			filteredNodesList = filteredNodesList.filter(
				(n) => n.id === "prism:core" || validNodeIds.has(n.id),
			);
		}

		if (filter.activeOnly) {
			filteredPathsList = filteredPathsList.filter((p) => p.active || p.activeSessions > 0);
			const activeNodeIds = new Set<string>();
			filteredPathsList.forEach((p) => {
				activeNodeIds.add(p.source);
				activeNodeIds.add(p.target);
			});
			filteredNodesList = filteredNodesList.filter(
				(n) => n.id === "prism:core" || activeNodeIds.has(n.id),
			);
		}

		if (filter.searchQuery.trim()) {
			const query = filter.searchQuery.trim().toLowerCase();
			const matchedNodeIds = new Set(
				filteredNodesList
					.filter(
						(n) =>
							n.label.toLowerCase().includes(query) ||
							(n.sublabel && n.sublabel.toLowerCase().includes(query)),
					)
					.map((n) => n.id),
			);
			filteredNodesList = filteredNodesList.filter((n) => matchedNodeIds.has(n.id));
			filteredPathsList = filteredPathsList.filter(
				(p) => matchedNodeIds.has(p.source) && matchedNodeIds.has(p.target),
			);
		}

		// Calculate vertical layout positioning (centering each column)
		const layerNodesMap = new Map<number, TopologyNode[]>();
		filteredNodesList.forEach((n) => {
			const list = layerNodesMap.get(n.layer) ?? [];
			list.push(n);
			layerNodesMap.set(n.layer, list);
		});

		let maxNodesInLayer = 1;
		layerNodesMap.forEach((list) => {
			if (list.length > maxNodesInLayer) maxNodesInLayer = list.length;
		});

		const computedHeight = Math.max(DEFAULT_HEIGHT, maxNodesInLayer * NODE_SPACING_Y + 140);

		layerNodesMap.forEach((list, layer) => {
			const n = list.length;
			const totalHeight = (n - 1) * NODE_SPACING_Y;
			const startY = Math.max(60, (computedHeight - totalHeight) / 2);

			list.forEach((node, idx) => {
				node.x = LAYER_X[layer] ?? 100;
				node.y = startY + idx * NODE_SPACING_Y;
			});
		});

		// Compute Global Summary
		let totalRaw = 0;
		let totalWire = 0;
		let totalIngress = 0;
		let totalEgress = 0;

		filteredPathsList.forEach((p) => {
			totalRaw += p.totalRawBytes;
			totalWire += p.totalWireBytes;
			totalIngress += p.uplinkBps;
			totalEgress += p.downlinkBps;
		});

		const savedRatio = totalRaw > 0 && totalRaw > totalWire ? (totalRaw - totalWire) / totalRaw : 0;

		const summary: TopologySummary = {
			totalNodes: filteredNodesList.length,
			activeNodes: filteredNodesList.filter((n) => n.activeSessions > 0).length,
			totalPaths: filteredPathsList.length,
			activePaths: filteredPathsList.filter((p) => p.active).length,
			totalSessions: sessions.length,
			ingressRateBps: totalIngress,
			egressRateBps: totalEgress,
			totalRawBytes: totalRaw,
			totalWireBytes: totalWire,
			savedRatio,
		};

		return {
			nodes: filteredNodesList,
			paths: filteredPathsList,
			summary,
			canvasWidth: 1540,
			canvasHeight: computedHeight,
		};
	}, [
		sessions,
		tunnelServices,
		optimizerStats,
		clusterNodes,
		middlewares,
		filter,
		calculatedRates,
	]);

	return {
		nodes,
		paths,
		summary,
		loading,
		error,
		refetch: refetchAll,
		canvasWidth,
		canvasHeight,
		optimizerStats,
	};
}
