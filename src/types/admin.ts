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

export interface AuthProvidersResponse {
	github_enabled: boolean;
	github_client_id?: string | null;
	mode: string;
	providers?: string[];
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

export interface GitHubLoginUrlResponse {
	url: string;
}

export interface ConfigFieldSchema {
	key: string;
	field_type: "u8" | "u16" | "u32" | "i32" | "i64" | "bool" | "string" | "list_string" | string;
	label: string;
	description: string;
	default_value: any;
}

export interface MiddlewareConfigSchema {
	name: string;
	fields: ConfigFieldSchema[];
}

export interface MiddlewareItem {
	name: string;
	schema: MiddlewareConfigSchema | null;
	effective_config: Record<string, any>;
}
