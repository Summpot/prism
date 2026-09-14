import type { SessionInfo } from "@/types/admin";

export type TopologyNodeType = "client" | "listener" | "route" | "core" | "connector" | "upstream";

export interface TopologyNode {
	id: string;
	label: string;
	sublabel?: string;
	type: TopologyNodeType;
	protocol?: string; // "tcp" | "udp" | "tunnel" | "quic" | "h3" | string
	status: "active" | "idle" | "offline";
	activeSessions: number;
	layer: number; // 0 to 5
	x: number;
	y: number;
	details?: Record<string, any>;
}

export interface TopologyPath {
	id: string;
	source: string; // source node id
	target: string; // target node id
	protocol?: string;
	activeSessions: number;
	uplinkBps: number; // bits per second
	downlinkBps: number;
	totalRawBytes: number;
	totalWireBytes: number;
	savedRatio?: number;
	netGainMs?: number;
	sessions: SessionInfo[];
	active: boolean; // has traffic or active sessions
}

export interface TopologySummary {
	totalNodes: number;
	activeNodes: number;
	totalPaths: number;
	activePaths: number;
	totalSessions: number;
	ingressRateBps: number;
	egressRateBps: number;
	totalRawBytes: number;
	totalWireBytes: number;
	savedRatio: number;
}

export interface TopologyFilter {
	protocol: "all" | "tcp" | "udp" | "tunnel";
	activeOnly: boolean;
	searchQuery: string;
}

export type SelectedElement =
	| { type: "node"; node: TopologyNode }
	| {
			type: "path";
			path: TopologyPath;
			sourceNode?: TopologyNode;
			targetNode?: TopologyNode;
	  }
	| null;
