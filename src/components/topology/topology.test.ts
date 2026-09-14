import { describe, expect, it } from "vitest";

import type { TopologyNode, TopologyPath } from "./topologyTypes";

describe("Topology Data Structures", () => {
	it("correctly models node hierarchy and path relationships", () => {
		const clientNode: TopologyNode = {
			id: "client:192.168.1.100",
			label: "192.168.1.100",
			sublabel: ":54321",
			type: "client",
			status: "active",
			activeSessions: 1,
			layer: 0,
			x: 80,
			y: 100,
		};

		const listenerNode: TopologyNode = {
			id: "listener::25565",
			label: ":25565",
			sublabel: "TCP Port",
			type: "listener",
			protocol: "tcp",
			status: "active",
			activeSessions: 1,
			layer: 1,
			x: 340,
			y: 100,
		};

		const path: TopologyPath = {
			id: `${clientNode.id}->${listenerNode.id}`,
			source: clientNode.id,
			target: listenerNode.id,
			protocol: "tcp",
			activeSessions: 1,
			uplinkBps: 2_400_000,
			downlinkBps: 8_500_000,
			totalRawBytes: 15_000_000,
			totalWireBytes: 9_000_000,
			savedRatio: 0.4,
			sessions: [
				{
					id: "sess-1",
					client: "192.168.1.100:54321",
					host: "mc.example.com",
					upstream: "127.0.0.1:25566",
					started_at_unix_ms: Date.now() - 60000,
					raw_bytes: 15_000_000,
					wire_bytes: 9_000_000,
				},
			],
			active: true,
		};

		expect(path.source).toBe("client:192.168.1.100");
		expect(path.target).toBe("listener::25565");
		expect(path.active).toBe(true);
		expect(path.uplinkBps + path.downlinkBps).toBe(10_900_000);
		expect(path.savedRatio).toBeCloseTo(0.4);
		expect(path.sessions.length).toBe(1);
	});
});
