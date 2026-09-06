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
	raw_bytes?: number;
	wire_bytes?: number;
	uplink_raw_bytes?: number;
	uplink_wire_bytes?: number;
	downlink_raw_bytes?: number;
	downlink_wire_bytes?: number;
	est_latency_improvement_ms?: number;
	est_latency_degradation_ms?: number;
}

export interface DirectionStatsSnapshot {
	raw_bytes: number;
	wire_bytes: number;
	saved_bytes: number;
	saved_ratio: number;
	batches: number;
	compression_time_us: number;
	decompression_time_us: number;
	est_transfer_time_saved_ms: number;
	est_processing_time_ms: number;
	net_latency_saved_ms: number;
}

export interface OptimizerStatsSnapshot {
	raw_bytes: number;
	wire_bytes: number;
	saved_bytes: number;
	saved_ratio: number;
	urgent_batches: number;
	timer_batches: number;
	threshold_batches: number;
	uplink?: DirectionStatsSnapshot;
	downlink?: DirectionStatsSnapshot;
	compression_time_us?: number;
	decompression_time_us?: number;
	batching_delay_us?: number;
	est_transfer_time_saved_ms?: number;
	est_processing_time_ms?: number;
	net_latency_saved_ms?: number;
}

export interface OptimizerOverviewResponse {
	global: OptimizerStatsSnapshot;
	services: Record<string, OptimizerStatsSnapshot>;
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

export interface HealthResponse {
	ok: boolean;
}

export interface ConfigPathResponse {
	path: string;
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
			...(connection.token?.trim() ? { Authorization: `Bearer ${connection.token.trim()}` } : {}),
			...init?.headers,
		},
	});

	if (!response.ok) {
		const text = await response.text();
		let message = text || `Request failed with status ${response.status}`;
		try {
			const parsed = JSON.parse(text) as { error?: string };
			if (parsed.error) {
				message = parsed.error;
			}
		} catch {
			// keep raw text
		}
		throw new ManagementApiError(message, response.status);
	}

	if (response.status === 204) {
		return undefined as T;
	}

	const text = await response.text();
	if (!text) {
		return undefined as T;
	}

	return JSON.parse(text) as T;
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

export function getOptimizerStats(connection: PanelConnection) {
	return apiRequest<OptimizerOverviewResponse>(connection, "/stats/optimizer");
}

export function getTunnelServices(connection: PanelConnection) {
	return apiRequest<ServiceSnapshot[]>(connection, "/tunnel/services");
}

export function triggerReload(connection: PanelConnection) {
	return apiRequest<ReloadResponse>(connection, "/reload", {
		method: "POST",
	});
}

export function getHealth(connection: PanelConnection) {
	return apiRequest<HealthResponse>(connection, "/health");
}

export function getConfigPath(connection: PanelConnection) {
	return apiRequest<ConfigPathResponse>(connection, "/config");
}

export type ClientOptimizerStats = OptimizerStatsSnapshot;

export interface ClientRegisteredService {
	name: string;
	proto: string;
	local_addr: string;
	route_only: boolean;
	remote_addr: string;
	masquerade_host: string;
	middleware?: string | null;
}

export interface ClientStatusResponse {
	running: boolean;
	state: string; // "idle" | "connecting" | "connected" | "disconnected"
	server_addr: string;
	transport: string;
	listen_addr: string;
	fake_lan_broadcast: boolean;
	known_services: ClientRegisteredService[];
	stats: ClientOptimizerStats;
	admin_url?: string | null;
}

export interface StartClientPayload {
	server_addr: string;
	transport?: string;
	auth_token?: string;
	listen_addr?: string;
	fake_lan_broadcast?: boolean;
	motd_prefix?: string;
}

export interface ClientProfile {
	id: string;
	name: string;
	server_addr: string;
	transport: string;
	auth_token: string;
	listen_addr: string;
	fake_lan_broadcast: boolean;
}

export function getClientStatus(connection: PanelConnection) {
	return apiRequest<ClientStatusResponse>(connection, "/client/status");
}

export function startClient(connection: PanelConnection, payload: StartClientPayload) {
	return apiRequest<{ ok: boolean }>(connection, "/client/start", {
		method: "POST",
		body: JSON.stringify(payload),
	});
}

export function stopClient(connection: PanelConnection) {
	return apiRequest<{ ok: boolean }>(connection, "/client/stop", {
		method: "POST",
	});
}

export function getClientProfiles(connection: PanelConnection) {
	return apiRequest<ClientProfile[]>(connection, "/client/profiles");
}

export function saveClientProfiles(connection: PanelConnection, profiles: ClientProfile[]) {
	return apiRequest<{ ok: boolean }>(connection, "/client/profiles", {
		method: "POST",
		body: JSON.stringify(profiles),
	});
}

export interface ClientLogEntry {
	timestamp: string;
	level: string;
	target: string;
	message: string;
}

export function getClientLogs(connection: PanelConnection, limit = 200) {
	return apiRequest<ClientLogEntry[]>(connection, `/client/logs?limit=${limit}`);
}

export function clearClientLogs(connection: PanelConnection) {
	return apiRequest<{ ok: boolean }>(connection, "/client/logs", {
		method: "DELETE",
	});
}

export interface AuthProvidersResponse {
	github_enabled: boolean;
	github_client_id?: string | null;
	mode: string;
}

export interface DeviceCodeResponse {
	device_code: string;
	user_code: string;
	verification_uri: string;
	expires_in: number;
	interval: number;
}

export interface DevicePollResult {
	status: "pending" | "slow_down" | "expired" | "denied" | "complete";
	token?: string;
	user?: UserRecord;
}

export interface AuthSessionResponse {
	authenticated: boolean;
	user_id?: string | null;
	username?: string | null;
	display_name?: string | null;
	avatar_url?: string | null;
	role?: string | null;
	is_admin: boolean;
}

export interface UserRecord {
	id: string;
	username: string;
	display_name?: string | null;
	avatar_url?: string | null;
	role: "admin" | "member" | "disabled";
	service_rules: string[];
	created_at_unix_ms: number;
	last_login_unix_ms: number;
}

export interface TokenRecord {
	id: string;
	user_id: string;
	token_type: "client" | "admin" | "connector";
	name: string;
	service_rules?: string[] | null;
	created_at_unix_ms: number;
	expires_at_unix_ms?: number | null;
	last_used_unix_ms: number;
}

export interface CreateTokenPayload {
	name: string;
	token_type: "client" | "admin" | "connector";
	expires_in_days?: number | null;
}

export interface CreateTokenResponse {
	raw_token: string;
	record: TokenRecord;
}

export function getAuthProviders(baseUrl: string): Promise<AuthProvidersResponse> {
	return fetch(`${baseUrl}/auth/providers`)
		.then(async (res) => {
			if (!res.ok) {
				return { github_enabled: false, github_client_id: null, mode: "token" };
			}
			const data = (await res.json()) as Record<string, unknown>;
			return {
				github_enabled: Boolean(data.github_enabled ?? data.github),
				github_client_id: (data.github_client_id as string) ?? null,
				mode: (data.mode as string) ?? "token",
			};
		})
		.catch(() => ({ github_enabled: false, github_client_id: null, mode: "token" }));
}

export function requestDeviceCode(connection: PanelConnection) {
	return apiRequest<DeviceCodeResponse>(connection, "/auth/device/code", {
		method: "POST",
	});
}

export function pollDeviceCode(connection: PanelConnection, deviceCode: string) {
	return apiRequest<DevicePollResult>(connection, "/auth/device/poll", {
		method: "POST",
		body: JSON.stringify({ device_code: deviceCode }),
	});
}

export function getAuthSession(connection: PanelConnection) {
	return apiRequest<AuthSessionResponse>(connection, "/auth/session");
}

export function listAuthTokens(connection: PanelConnection) {
	return apiRequest<TokenRecord[]>(connection, "/auth/tokens");
}

export function createAuthToken(connection: PanelConnection, payload: CreateTokenPayload) {
	return apiRequest<CreateTokenResponse>(connection, "/auth/tokens", {
		method: "POST",
		body: JSON.stringify(payload),
	});
}

export function revokeAuthToken(connection: PanelConnection, tokenId: string) {
	return apiRequest<{ ok: boolean }>(connection, `/auth/tokens/${encodeURIComponent(tokenId)}`, {
		method: "DELETE",
	});
}

export function listManagedUsers(connection: PanelConnection) {
	return apiRequest<UserRecord[]>(connection, "/managed/users");
}

export function updateManagedUser(
	connection: PanelConnection,
	userId: string,
	payload: { role?: string; service_rules?: string[] },
) {
	return apiRequest<UserRecord>(connection, `/managed/users/${encodeURIComponent(userId)}`, {
		method: "PUT",
		body: JSON.stringify(payload),
	});
}
