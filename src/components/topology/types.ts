import type { Edge, Node } from "@xyflow/react";
import type { OptimizerStatsSnapshot, ServiceSnapshot, SessionInfo } from "@/types/admin";

export type TopologyNodeType = "client" | "gateway" | "connector" | "service";

export interface ClientNodeData extends Record<string, unknown> {
	type: "client";
	clientId: string;
	ip: string;
	sessionCount: number;
	rawBytes: number;
	wireBytes: number;
	sessions: SessionInfo[];
}

export interface GatewayNodeData extends Record<string, unknown> {
	type: "gateway";
	nodeId: string;
	activeConnectionsCount: number;
	totalRawBytes: number;
	totalWireBytes: number;
	globalOptimizer?: OptimizerStatsSnapshot | null;
}

export interface ConnectorNodeData extends Record<string, unknown> {
	type: "connector";
	connectorId: string;
	remoteAddr: string;
	servicesCount: number;
	primary: boolean;
	services: ServiceSnapshot[];
	activeSessionsCount: number;
}

export interface ServiceNodeData extends Record<string, unknown> {
	type: "service";
	serviceName: string;
	proto: string;
	localAddr: string;
	remoteAddr?: string;
	masqueradeHost?: string;
	routeOnly: boolean;
	clientId: string;
	primary: boolean;
	optimizerStats?: OptimizerStatsSnapshot | null;
	activeSessionsCount: number;
}

export type TopologyCustomNodeData =
	| ClientNodeData
	| GatewayNodeData
	| ConnectorNodeData
	| ServiceNodeData;

export type TopologyCustomNode = Node<TopologyCustomNodeData, TopologyNodeType>;

export interface TrafficEdgeData extends Record<string, unknown> {
	active: boolean;
	sessionCount: number;
	label?: string;
	bytes?: number;
	proto?: string;
}

export type TopologyCustomEdge = Edge<TrafficEdgeData, "traffic">;

export type LayoutDirection = "LR" | "TB";

export interface TopologyFilterState {
	search: string;
	activeOnly: boolean;
	showClients: boolean;
	showConnectors: boolean;
	showServices: boolean;
}
