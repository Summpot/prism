import type { PanelConnection } from "@/lib/panelConnection";

export interface ManagedProxyListenerDocument {
	listen_addr: string;
	protocol: string;
	upstream: string;
}

export interface ManagedRouteDocument {
	hosts: string[];
	upstreams: string[];
	middlewares: string[];
	strategy: string;
}

export interface ManagedTimeoutsDocument {
	handshake_timeout_ms?: number | null;
	idle_timeout_ms?: number | null;
}

export interface ManagedTunnelEndpointDocument {
	listen_addr: string;
	transport: string;
	quic?: {
		cert_file?: string | null;
		key_file?: string | null;
	} | null;
}

export interface ManagedTunnelClientDocument {
	server_addr: string;
	transport: string;
	dial_timeout_ms?: number | null;
	quic?: {
		server_name?: string | null;
		insecure_skip_verify: boolean;
	} | null;
}

export interface ManagedTunnelServiceDocument {
	name: string;
	proto: string;
	local_addr: string;
	route_only: boolean;
	remote_addr: string;
	masquerade_host: string;
}

export interface ManagedTunnelDocument {
	auth_token: string;
	auto_listen_services: boolean;
	endpoints: ManagedTunnelEndpointDocument[];
	client?: ManagedTunnelClientDocument | null;
	services: ManagedTunnelServiceDocument[];
}

export interface ManagedConfigDocument {
	listeners: ManagedProxyListenerDocument[];
	routes: ManagedRouteDocument[];
	max_header_bytes: number;
	proxy_protocol_v2: boolean;
	buffer_size: number;
	upstream_dial_timeout_ms: number;
	timeouts?: ManagedTimeoutsDocument | null;
	tunnel?: ManagedTunnelDocument | null;
}

export interface ManagedNodeSnapshot {
	node_id: string;
	connection_mode?: "active" | "passive" | null;
	agent_url?: string | null;
	desired_revision: number;
	applied_revision: number;
	pending_restart: boolean;
	restart_reasons: string[];
	last_apply_error?: string | null;
	last_seen_unix_ms: number;
	last_apply_attempt_unix_ms: number;
	last_apply_success_unix_ms: number;
}

export interface ManagedNodeConfigResponse {
	node: ManagedNodeSnapshot;
	desired_config?: ManagedConfigDocument | null;
}

export interface ManagementStatusResponse {
	state_path: string;
	node_count: number;
}

export interface SessionInfo {
	id: string;
	client: string;
	host: string;
	upstream: string;
	started_at_unix_ms: number;
}

export interface RegisteredService {
	name: string;
	proto: string;
	local_addr: string;
	route_only: boolean;
	remote_addr: string;
	masquerade_host: string;
}

export interface ServiceSnapshot {
	service: RegisteredService;
	client_id: string;
	remote: string;
	primary: boolean;
}

export interface ReloadResponse {
	seq: number;
}

export class ManagementApiError extends Error {
	status: number;

	constructor(message: string, status: number) {
		super(message);
		this.name = "ManagementApiError";
		this.status = status;
	}
}

async function apiRequest<T>(
	connection: PanelConnection,
	path: string,
	init?: RequestInit,
): Promise<T> {
	const response = await fetch(`${connection.baseUrl}${path}`, {
		...init,
		headers: {
			"Content-Type": "application/json",
			Authorization: `Bearer ${connection.token}`,
			...init?.headers,
		},
	});

	if (!response.ok) {
		const text = await response.text();
		throw new ManagementApiError(
			text || `Request failed with status ${response.status}`,
			response.status,
		);
	}

	return (await response.json()) as T;
}

export function getManagementStatus(connection: PanelConnection) {
	return apiRequest<ManagementStatusResponse>(connection, "/managed/status");
}

export function getManagedNodes(connection: PanelConnection) {
	return apiRequest<ManagedNodeSnapshot[]>(connection, "/managed/nodes");
}

export function getManagedNode(connection: PanelConnection, nodeId: string) {
	return apiRequest<ManagedNodeSnapshot>(
		connection,
		`/managed/nodes/${encodeURIComponent(nodeId)}`,
	);
}

export function getManagedNodeConfig(connection: PanelConnection, nodeId: string) {
	return apiRequest<ManagedNodeConfigResponse>(
		connection,
		`/managed/nodes/${encodeURIComponent(nodeId)}/config`,
	);
}

export function updateManagedNodeConfig(
	connection: PanelConnection,
	nodeId: string,
	desiredConfig: ManagedConfigDocument,
) {
	return apiRequest<ManagedNodeConfigResponse>(
		connection,
		`/managed/nodes/${encodeURIComponent(nodeId)}/config`,
		{
			method: "PUT",
			body: JSON.stringify({ desired_config: desiredConfig }),
		},
	);
}

export function getConnections(connection: PanelConnection) {
	return apiRequest<SessionInfo[]>(connection, "/conns");
}

export function getTunnelServices(connection: PanelConnection) {
	return apiRequest<ServiceSnapshot[]>(connection, "/tunnel/services");
}

export function triggerReload(connection: PanelConnection) {
	return apiRequest<ReloadResponse>(connection, "/reload", {
		method: "POST",
	});
}
