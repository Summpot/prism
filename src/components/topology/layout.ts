import type { LayoutDirection, TopologyCustomEdge, TopologyCustomNode } from "./types";

interface LayoutDimensions {
	nodeWidth: number;
	nodeHeight: number;
	layerSpacing: number;
	nodeSpacing: number;
}

const DEFAULT_DIMENSIONS: LayoutDimensions = {
	nodeWidth: 260,
	nodeHeight: 120,
	layerSpacing: 380,
	nodeSpacing: 140,
};

/**
 * Calculates a layered hierarchical DAG layout for topology nodes.
 * Automatically aligns and centers nodes across 4 layers:
 * Layer 0: Inbound Clients
 * Layer 1: Prism Gateway / Core
 * Layer 2: Private Connectors
 * Layer 3: Target Backend Services
 */
export function calculateTopologyLayout(
	nodes: TopologyCustomNode[],
	edges: TopologyCustomEdge[],
	direction: LayoutDirection = "LR",
	dims: LayoutDimensions = DEFAULT_DIMENSIONS,
): { nodes: TopologyCustomNode[]; edges: TopologyCustomEdge[] } {
	// Group nodes by topology layer
	const layers: Record<number, TopologyCustomNode[]> = {
		0: [],
		1: [],
		2: [],
		3: [],
	};

	for (const node of nodes) {
		switch (node.type) {
			case "client":
				layers[0].push(node);
				break;
			case "gateway":
				layers[1].push(node);
				break;
			case "connector":
				layers[2].push(node);
				break;
			case "service":
				layers[3].push(node);
				break;
			default:
				layers[1].push(node);
				break;
		}
	}

	// Calculate maximum number of nodes in any single layer to center other layers
	const maxCount = Math.max(
		layers[0].length,
		layers[1].length,
		layers[2].length,
		layers[3].length,
		1,
	);

	const layoutNodes: TopologyCustomNode[] = [];

	if (direction === "LR") {
		const totalSpan = maxCount * dims.nodeSpacing;

		for (let layerIdx = 0; layerIdx <= 3; layerIdx++) {
			const layerNodes = layers[layerIdx];
			const count = layerNodes.length;
			if (count === 0) continue;

			const layerSpan = count * dims.nodeSpacing;
			const startY = (totalSpan - layerSpan) / 2;
			const posX = layerIdx * dims.layerSpacing;

			layerNodes.forEach((node, index) => {
				const posY = startY + index * dims.nodeSpacing;
				layoutNodes.push({
					...node,
					position: { x: posX, y: posY },
					targetPosition: (layerIdx === 0 ? undefined : ("left" as const)) as any,
					sourcePosition: (layerIdx === 3 ? undefined : ("right" as const)) as any,
				});
			});
		}
	} else {
		// TB layout
		const totalSpan = maxCount * (dims.nodeWidth + 60);

		for (let layerIdx = 0; layerIdx <= 3; layerIdx++) {
			const layerNodes = layers[layerIdx];
			const count = layerNodes.length;
			if (count === 0) continue;

			const layerSpan = count * (dims.nodeWidth + 60);
			const startX = (totalSpan - layerSpan) / 2;
			const posY = layerIdx * (dims.nodeHeight + 160);

			layerNodes.forEach((node, index) => {
				const posX = startX + index * (dims.nodeWidth + 60);
				layoutNodes.push({
					...node,
					position: { x: posX, y: posY },
					targetPosition: (layerIdx === 0 ? undefined : ("top" as const)) as any,
					sourcePosition: (layerIdx === 3 ? undefined : ("bottom" as const)) as any,
				});
			});
		}
	}

	return {
		nodes: layoutNodes,
		edges,
	};
}
