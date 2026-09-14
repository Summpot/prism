import { describe, expect, it } from "vitest";
import { calculateTopologyLayout } from "./layout";
import type { TopologyCustomEdge, TopologyCustomNode } from "./types";

describe("calculateTopologyLayout", () => {
	it("positions nodes in 4 distinct horizontal layers for LR layout", () => {
		const nodes: TopologyCustomNode[] = [
			{
				id: "client-1",
				type: "client",
				position: { x: 0, y: 0 },
				data: {
					type: "client",
					clientId: "client-1",
					ip: "192.168.1.100",
					sessionCount: 1,
					rawBytes: 1000,
					wireBytes: 800,
					sessions: [],
				},
			},
			{
				id: "gateway-1",
				type: "gateway",
				position: { x: 0, y: 0 },
				data: {
					type: "gateway",
					nodeId: "prism-core",
					desiredRevision: 1,
					appliedRevision: 1,
					pendingRestart: false,
					restartReasons: [],
					activeConnectionsCount: 1,
					totalRawBytes: 1000,
					totalWireBytes: 800,
				},
			},
			{
				id: "connector-1",
				type: "connector",
				position: { x: 0, y: 0 },
				data: {
					type: "connector",
					connectorId: "conn-agent-1",
					remoteAddr: "10.0.0.5:54321",
					servicesCount: 1,
					primary: true,
					services: [],
					activeSessionsCount: 1,
				},
			},
			{
				id: "service-1",
				type: "service",
				position: { x: 0, y: 0 },
				data: {
					type: "service",
					serviceName: "mc-server",
					proto: "tcp",
					localAddr: "127.0.0.1:25565",
					routeOnly: false,
					clientId: "conn-agent-1",
					primary: true,
					activeSessionsCount: 1,
				},
			},
		];

		const edges: TopologyCustomEdge[] = [
			{ id: "e1", source: "client-1", target: "gateway-1", type: "traffic" },
			{ id: "e2", source: "gateway-1", target: "connector-1", type: "traffic" },
			{ id: "e3", source: "connector-1", target: "service-1", type: "traffic" },
		];

		const result = calculateTopologyLayout(nodes, edges, "LR");

		expect(result.nodes).toHaveLength(4);
		const client = result.nodes.find((n) => n.id === "client-1");
		const gateway = result.nodes.find((n) => n.id === "gateway-1");
		const connector = result.nodes.find((n) => n.id === "connector-1");
		const service = result.nodes.find((n) => n.id === "service-1");

		expect(client).toBeDefined();
		expect(gateway).toBeDefined();
		expect(connector).toBeDefined();
		expect(service).toBeDefined();

		// Check x coordinate progression
		expect(client!.position.x).toBeLessThan(gateway!.position.x);
		expect(gateway!.position.x).toBeLessThan(connector!.position.x);
		expect(connector!.position.x).toBeLessThan(service!.position.x);
	});

	it("positions nodes in vertical layers for TB layout", () => {
		const nodes: TopologyCustomNode[] = [
			{
				id: "client-1",
				type: "client",
				position: { x: 0, y: 0 },
				data: {
					type: "client",
					clientId: "client-1",
					ip: "192.168.1.100",
					sessionCount: 1,
					rawBytes: 1000,
					wireBytes: 800,
					sessions: [],
				},
			},
			{
				id: "gateway-1",
				type: "gateway",
				position: { x: 0, y: 0 },
				data: {
					type: "gateway",
					nodeId: "prism-core",
					desiredRevision: 1,
					appliedRevision: 1,
					pendingRestart: false,
					restartReasons: [],
					activeConnectionsCount: 1,
					totalRawBytes: 1000,
					totalWireBytes: 800,
				},
			},
		];

		const result = calculateTopologyLayout(nodes, [], "TB");
		const client = result.nodes.find((n) => n.id === "client-1");
		const gateway = result.nodes.find((n) => n.id === "gateway-1");

		expect(client!.position.y).toBeLessThan(gateway!.position.y);
	});

	it("handles empty nodes without throwing or generating NaN", () => {
		const result = calculateTopologyLayout([], []);
		expect(result.nodes).toEqual([]);
		expect(result.edges).toEqual([]);
	});
});
