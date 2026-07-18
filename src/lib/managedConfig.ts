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

export interface ManagedConfigSummary {
	listeners: number;
	tcpListeners: number;
	udpListeners: number;
	hostnameRoutingListeners: number;
	routes: number;
	tunnelEnabled: boolean;
	tunnelEndpoints: number;
	tunnelServices: number;
	hasTunnelClient: boolean;
	proxyProtocol: boolean;
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

export function summarizeManagedConfig(doc: ManagedConfigDocument): ManagedConfigSummary {
	const normalized = normalizeManagedConfig(doc);
	const tcpListeners = normalized.listeners.filter((listener) => listener.protocol === "tcp");
	return {
		listeners: normalized.listeners.length,
		tcpListeners: tcpListeners.length,
		udpListeners: normalized.listeners.filter((listener) => listener.protocol === "udp").length,
		hostnameRoutingListeners: tcpListeners.filter((listener) => !listener.upstream).length,
		routes: normalized.routes.length,
		tunnelEnabled: Boolean(normalized.tunnel),
		tunnelEndpoints: normalized.tunnel?.endpoints.length ?? 0,
		tunnelServices: normalized.tunnel?.services.length ?? 0,
		hasTunnelClient: Boolean(normalized.tunnel?.client),
		proxyProtocol: normalized.proxy_protocol_v2,
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

		if (listener.protocol !== "tcp" && listener.protocol !== "udp") {
			issues.push({
				path: `listeners.${index}.protocol`,
				message: "Protocol must be tcp or udp.",
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
		if (!["sequential", "random", "round-robin"].includes(route.strategy)) {
			issues.push({
				path: `routes.${index}.strategy`,
				message: "Strategy must be sequential, random, or round-robin.",
			});
		}
	});

	if (normalized.routes.length > 0) {
		const hasHostnameRouting = normalized.listeners.some(
			(listener) => listener.protocol === "tcp" && !listener.upstream,
		);
		if (!hasHostnameRouting) {
			issues.push({
				path: "listeners",
				message:
					"Hostname routes need at least one TCP listener with an empty upstream (hostname-routing mode).",
			});
		}
	}

	const tunnel = normalized.tunnel;
	if (tunnel) {
		tunnel.endpoints.forEach((endpoint, index) => {
			if (!endpoint.listen_addr) {
				issues.push({
					path: `tunnel.endpoints.${index}.listen_addr`,
					message: "Tunnel endpoint address is required.",
				});
			}
			if (!["tcp", "udp", "quic"].includes(endpoint.transport)) {
				issues.push({
					path: `tunnel.endpoints.${index}.transport`,
					message: "Transport must be tcp, udp, or quic.",
				});
			}
		});

		if (doc.tunnel?.client && !tunnel.client) {
			issues.push({
				path: "tunnel.client.server_addr",
				message: "Tunnel client server address is required.",
			});
		}

		if (tunnel.client && !["tcp", "udp", "quic"].includes(tunnel.client.transport)) {
			issues.push({
				path: "tunnel.client.transport",
				message: "Client transport must be tcp, udp, or quic.",
			});
		}

		tunnel.services.forEach((service, index) => {
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
			if (!["tcp", "udp"].includes(service.proto)) {
				issues.push({
					path: `tunnel.services.${index}.proto`,
					message: "Service protocol must be tcp or udp.",
				});
			}
			if (service.route_only && service.remote_addr) {
				issues.push({
					path: `tunnel.services.${index}.remote_addr`,
					message: "route_only services must not declare remote_addr.",
				});
			}
		});
	}

	return issues;
}

export function formatIssuesByPath(issues: ConfigIssue[]) {
	return issues.reduce<Record<string, string[]>>((acc, issue) => {
		acc[issue.path] = [...(acc[issue.path] ?? []), issue.message];
		return acc;
	}, {});
}

export function managedConfigFingerprint(doc: ManagedConfigDocument) {
	return JSON.stringify(normalizeManagedConfig(doc));
}

export function parseManagedConfigJson(
	raw: string,
): { ok: true; value: ManagedConfigDocument } | { ok: false; error: string } {
	let parsed: unknown;
	try {
		parsed = JSON.parse(raw);
	} catch (error) {
		return {
			ok: false,
			error: error instanceof Error ? error.message : "Invalid JSON",
		};
	}

	if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
		return { ok: false, error: "Config root must be a JSON object." };
	}

	const value = parsed as Partial<ManagedConfigDocument>;
	const base = createEmptyManagedConfig();
	const doc: ManagedConfigDocument = {
		listeners: Array.isArray(value.listeners) ? value.listeners : base.listeners,
		routes: Array.isArray(value.routes) ? value.routes : base.routes,
		max_header_bytes:
			typeof value.max_header_bytes === "number" ? value.max_header_bytes : base.max_header_bytes,
		proxy_protocol_v2: Boolean(value.proxy_protocol_v2),
		buffer_size: typeof value.buffer_size === "number" ? value.buffer_size : base.buffer_size,
		upstream_dial_timeout_ms:
			typeof value.upstream_dial_timeout_ms === "number"
				? value.upstream_dial_timeout_ms
				: base.upstream_dial_timeout_ms,
		timeouts: value.timeouts ?? base.timeouts,
		tunnel: value.tunnel === null ? undefined : (value.tunnel ?? undefined),
	};

	return { ok: true, value: normalizeManagedConfig(doc) };
}

export function moveItem<T>(items: T[], from: number, to: number): T[] {
	if (from === to || from < 0 || to < 0 || from >= items.length || to >= items.length) {
		return items;
	}
	const next = [...items];
	const [item] = next.splice(from, 1);
	next.splice(to, 0, item);
	return next;
}

export function createEmptyTunnel(): ManagedTunnelDocument {
	return {
		auth_token: "",
		auto_listen_services: true,
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

export function createEmptyRoute(): ManagedRouteDocument {
	return {
		hosts: [""],
		upstreams: [""],
		middlewares: ["minecraft_handshake"],
		strategy: "sequential",
	};
}
