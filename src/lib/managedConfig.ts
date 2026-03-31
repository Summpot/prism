import type {
	ManagedConfigDocument,
	ManagedRouteDocument,
	ManagedTunnelClientDocument,
	ManagedTunnelDocument,
	ManagedTunnelEndpointDocument,
	ManagedTunnelServiceDocument,
} from "@/lib/managementApi";

export interface ConfigIssue {
	path: string;
	message: string;
}

function trimList(values: string[]) {
	return values.map((value) => value.trim()).filter(Boolean);
}

export function createEmptyManagedConfig(): ManagedConfigDocument {
	return {
		listeners: [],
		routes: [],
		max_header_bytes: 65536,
		proxy_protocol_v2: false,
		buffer_size: 32768,
		upstream_dial_timeout_ms: 5000,
		timeouts: {
			handshake_timeout_ms: 3000,
			idle_timeout_ms: 0,
		},
		tunnel: undefined,
	};
}

export function normalizeManagedRoute(route: ManagedRouteDocument): ManagedRouteDocument {
	return {
		hosts: trimList(route.hosts),
		upstreams: trimList(route.upstreams),
		middlewares: trimList(route.middlewares).map((value) =>
			value.toLowerCase().replaceAll("-", "_"),
		),
		strategy: route.strategy.trim() || "sequential",
	};
}

function normalizeTunnel(tunnel: ManagedTunnelDocument | null | undefined) {
	if (!tunnel) {
		return undefined;
	}

	const endpoints = tunnel.endpoints
		.map((endpoint) => ({
			listen_addr: endpoint.listen_addr.trim(),
			transport: endpoint.transport.trim() || "tcp",
			quic: endpoint.quic
				? {
						cert_file: endpoint.quic.cert_file?.trim() || "",
						key_file: endpoint.quic.key_file?.trim() || "",
					}
				: undefined,
		}))
		.filter((endpoint) => endpoint.listen_addr);

	const services = tunnel.services
		.map((service) => ({
			name: service.name.trim(),
			proto: service.proto.trim() || "tcp",
			local_addr: service.local_addr.trim(),
			route_only: service.route_only,
			remote_addr: service.remote_addr.trim(),
			masquerade_host: service.masquerade_host.trim(),
		}))
		.filter((service) => service.name || service.local_addr || service.remote_addr);

	const client = tunnel.client?.server_addr.trim()
		? {
				server_addr: tunnel.client.server_addr.trim(),
				transport: tunnel.client.transport.trim() || "tcp",
				dial_timeout_ms: tunnel.client.dial_timeout_ms ?? 5000,
				quic: tunnel.client.quic
					? {
							server_name: tunnel.client.quic.server_name?.trim() || "",
							insecure_skip_verify: tunnel.client.quic.insecure_skip_verify,
						}
					: undefined,
			}
		: undefined;

	return {
		auth_token: tunnel.auth_token.trim(),
		auto_listen_services: tunnel.auto_listen_services,
		endpoints,
		client,
		services,
	};
}

export function normalizeManagedConfig(doc: ManagedConfigDocument): ManagedConfigDocument {
	return {
		listeners: doc.listeners
			.map((listener) => ({
				listen_addr: listener.listen_addr.trim(),
				protocol: listener.protocol.trim() || "tcp",
				upstream: listener.upstream.trim(),
			}))
			.filter((listener) => listener.listen_addr || listener.upstream),
		routes: doc.routes.map(normalizeManagedRoute).filter((route) => route.hosts.length > 0),
		max_header_bytes: Math.max(0, doc.max_header_bytes || 0),
		proxy_protocol_v2: Boolean(doc.proxy_protocol_v2),
		buffer_size: Math.max(0, doc.buffer_size || 0),
		upstream_dial_timeout_ms: Math.max(0, doc.upstream_dial_timeout_ms || 0),
		timeouts: doc.timeouts
			? {
					handshake_timeout_ms: Math.max(0, doc.timeouts.handshake_timeout_ms || 0),
					idle_timeout_ms: Math.max(0, doc.timeouts.idle_timeout_ms || 0),
				}
			: undefined,
		tunnel: normalizeTunnel(doc.tunnel),
	};
}

export function validateManagedConfig(doc: ManagedConfigDocument): ConfigIssue[] {
	const normalized = normalizeManagedConfig(doc);
	const issues: ConfigIssue[] = [];

	normalized.listeners.forEach((listener, index) => {
		if (!listener.listen_addr) {
			issues.push({
				path: `listeners.${index}.listen_addr`,
				message: "Listener address is required.",
			});
		}

		if (listener.protocol === "udp" && !listener.upstream) {
			issues.push({
				path: `listeners.${index}.upstream`,
				message: "UDP listeners require an upstream.",
			});
		}
	});

	normalized.routes.forEach((route, index) => {
		if (route.hosts.length === 0) {
			issues.push({
				path: `routes.${index}.hosts`,
				message: "At least one route host is required.",
			});
		}
		if (route.upstreams.length === 0) {
			issues.push({
				path: `routes.${index}.upstreams`,
				message: "At least one route upstream is required.",
			});
		}
		if (route.middlewares.length === 0) {
			issues.push({
				path: `routes.${index}.middlewares`,
				message: "Routing routes require at least one middleware.",
			});
		}
	});

	normalized.tunnel?.services.forEach((service, index) => {
		if (!service.name) {
			issues.push({
				path: `tunnel.services.${index}.name`,
				message: "Tunnel service name is required.",
			});
		}
		if (!service.local_addr) {
			issues.push({
				path: `tunnel.services.${index}.local_addr`,
				message: "Tunnel service local address is required.",
			});
		}
		if (service.route_only && service.remote_addr) {
			issues.push({
				path: `tunnel.services.${index}.remote_addr`,
				message: "route_only services must not declare remote_addr.",
			});
		}
	});

	return issues;
}

export function formatIssuesByPath(issues: ConfigIssue[]) {
	return issues.reduce<Record<string, string[]>>((acc, issue) => {
		acc[issue.path] = [...(acc[issue.path] ?? []), issue.message];
		return acc;
	}, {});
}

export function createEmptyTunnel(): ManagedTunnelDocument {
	return {
		auth_token: "",
		auto_listen_services: false,
		endpoints: [],
		client: null,
		services: [],
	};
}

export function createEmptyTunnelEndpoint(): ManagedTunnelEndpointDocument {
	return { listen_addr: "", transport: "tcp", quic: null };
}

export function createEmptyTunnelService(): ManagedTunnelServiceDocument {
	return {
		name: "",
		proto: "tcp",
		local_addr: "",
		route_only: false,
		remote_addr: "",
		masquerade_host: "",
	};
}

export function createEmptyTunnelClient(): ManagedTunnelClientDocument {
	return {
		server_addr: "",
		transport: "tcp",
		dial_timeout_ms: 5000,
		quic: null,
	};
}
