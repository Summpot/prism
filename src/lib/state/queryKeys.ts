import type { PanelConnection } from "@/lib/panelConnection";

export function connectionKey(connection: PanelConnection | null): string {
	if (!connection) {
		return "none";
	}
	return `${connection.kind ?? "bearer"}|${connection.baseUrl}|${connection.token}`;
}

export const queryKeys = {
	client: {
		all: ["client"] as const,
		status: ["client", "status"] as const,
		config: ["client", "config"] as const,
		logs: ["client", "logs"] as const,
	},
	admin: {
		all: ["admin"] as const,
		session: (connection: PanelConnection | null) =>
			["admin", "session", connectionKey(connection)] as const,
		status: (connection: PanelConnection | null) =>
			["admin", "status", connectionKey(connection)] as const,
		nodes: (connection: PanelConnection | null) =>
			["admin", "nodes", connectionKey(connection)] as const,
		nodeConfig: (connection: PanelConnection | null, nodeId: string) =>
			["admin", "nodeConfig", connectionKey(connection), nodeId] as const,
		connections: (connection: PanelConnection | null) =>
			["admin", "connections", connectionKey(connection)] as const,
		services: (connection: PanelConnection | null) =>
			["admin", "services", connectionKey(connection)] as const,
		optimizer: (connection: PanelConnection | null) =>
			["admin", "optimizer", connectionKey(connection)] as const,
		users: (connection: PanelConnection | null) =>
			["admin", "users", connectionKey(connection)] as const,
		tokens: (connection: PanelConnection | null) =>
			["admin", "tokens", connectionKey(connection)] as const,
		health: (connection: PanelConnection | null) =>
			["admin", "health", connectionKey(connection)] as const,
		configPath: (connection: PanelConnection | null) =>
			["admin", "configPath", connectionKey(connection)] as const,
		middlewares: (connection: PanelConnection | null, local: boolean) =>
			["admin", "middlewares", local ? "local" : connectionKey(connection)] as const,
	},
};
