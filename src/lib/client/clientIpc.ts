import { invokeTauri } from "@/lib/desktopWindow";
import type { MiddlewareItem } from "@/types/admin";
import type {
	ClientConfigResponse,
	ClientConfigState,
	ClientLogEntry,
	ClientProfile,
	ClientStatusResponse,
	StartClientPayload,
} from "@/types/client";

export function getClientStatus(): Promise<ClientStatusResponse> {
	return invokeTauri<ClientStatusResponse>("client_status");
}

export function getClientConfig(): Promise<ClientConfigResponse> {
	return invokeTauri<ClientConfigResponse>("client_get_config");
}

export async function saveClientConfig(payload: {
	active_profile_id?: string | null;
	active_config?: Partial<ClientConfigState>;
}): Promise<{ ok: boolean }> {
	await invokeTauri("client_save_config", { payload });
	return { ok: true };
}

export async function resetClientStats(): Promise<{ ok: boolean }> {
	await invokeTauri("client_reset_stats");
	return { ok: true };
}

export async function startClient(payload: StartClientPayload): Promise<{ ok: boolean }> {
	await invokeTauri("client_start", { payload });
	return { ok: true };
}

export async function stopClient(): Promise<{ ok: boolean }> {
	await invokeTauri("client_stop");
	return { ok: true };
}

export function getClientProfiles(): Promise<ClientProfile[]> {
	return invokeTauri<ClientProfile[]>("client_get_profiles");
}

export async function saveClientProfiles(profiles: ClientProfile[]): Promise<{ ok: boolean }> {
	await invokeTauri("client_save_profiles", { profiles });
	return { ok: true };
}

export function getClientLogs(limit = 200): Promise<ClientLogEntry[]> {
	return invokeTauri<ClientLogEntry[]>("client_logs", { limit });
}

export async function clearClientLogs(): Promise<{ ok: boolean }> {
	await invokeTauri("client_clear_logs");
	return { ok: true };
}

export function listLocalMiddlewares(): Promise<MiddlewareItem[]> {
	return invokeTauri<MiddlewareItem[]>("client_list_middlewares");
}

export function updateLocalMiddlewareConfig(
	name: string,
	config: Record<string, unknown>,
): Promise<{ status: string; name: string; config: Record<string, unknown> }> {
	return invokeTauri("client_update_middleware_config", { name, config });
}

export function resetLocalMiddlewareConfig(
	name: string,
): Promise<{ status: string; name: string; reset: boolean }> {
	return invokeTauri("client_reset_middleware_config", { name });
}
