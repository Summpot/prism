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
	websocket?: {
		cert_file?: string | null;
		key_file?: string | null;
		url_path?: string | null;
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
	websocket?: {
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

// Aliases for modern domain terminology
export type NodeProxyListener = ManagedProxyListenerDocument;
export type NodeRoute = ManagedRouteDocument;
export type NodeTimeouts = ManagedTimeoutsDocument;
export type NodeTunnelEndpoint = ManagedTunnelEndpointDocument;
export type NodeTunnelClient = ManagedTunnelClientDocument;
export type NodeTunnelService = ManagedTunnelServiceDocument;
export type NodeTunnelConfig = ManagedTunnelDocument;
export type NodeConfigDocument = ManagedConfigDocument;

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

export type NodeSnapshot = ManagedNodeSnapshot;

export interface ManagedNodeConfigResponse {
	node: ManagedNodeSnapshot;
	desired_config?: ManagedConfigDocument | null;
}

export interface ManagementStatusResponse {
	state_path: string;
	node_count: number;
}
