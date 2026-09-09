import type { OptimizerStatsSnapshot } from "./admin";

export interface CumulativeStats {
	raw_bytes: number;
	wire_bytes: number;
	saved_bytes: number;
	saved_ratio: number;
	sessions_count: number;
	last_session_at?: string | null;
}

export type ClientOptimizerStats = OptimizerStatsSnapshot & {
	sessions_count?: number;
};

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
	active_profile_id?: string | null;
	cumulative_stats?: CumulativeStats | null;
}

export interface ClientConfigState {
	profile_name: string;
	server_addr: string;
	transport: string;
	auth_token: string;
	listen_addr: string;
	fake_lan_broadcast: boolean;
	auto_connect_panel: boolean;
}

export interface ClientConfigResponse {
	active_profile_id: string | null;
	active_config: ClientConfigState;
	profiles: ClientProfile[];
	cumulative_stats: CumulativeStats;
}

export interface StartClientPayload {
	server_addr: string;
	transport?: string;
	auth_token?: string;
	listen_addr?: string;
	fake_lan_broadcast?: boolean;
	motd_prefix?: string;
	profile_id?: string;
	profile_name?: string;
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

export interface ClientLogEntry {
	timestamp: string;
	level: string;
	target: string;
	message: string;
}
