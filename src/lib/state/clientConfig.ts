import { getClientConfig, saveClientConfig, saveClientProfiles } from "@/lib/client/clientIpc";
import { getQueryClient } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import type { ClientConfigResponse, ClientConfigState, ClientProfile } from "@/types/client";

export const EMPTY_CLIENT_CONFIG: ClientConfigResponse = {
	active_profile_id: null,
	active_config: {
		profile_name: "",
		server_addr: "127.0.0.1",
		transport: "auto",
		auth_token: "",
		listen_addr: "127.0.0.1:25565",
		fake_lan_broadcast: true,
		auto_connect_panel: true,
		auto_connect: true,
	},
	profiles: [],
	cumulative_stats: {
		raw_bytes: 0,
		wire_bytes: 0,
		saved_bytes: 0,
		saved_ratio: 0,
		sessions_count: 0,
	},
};

export function mergeActiveConfig(
	current: ClientConfigResponse,
	patch: Partial<ClientConfigState>,
	activeProfileId?: string | null,
): ClientConfigResponse {
	return {
		...current,
		active_profile_id: activeProfileId === undefined ? current.active_profile_id : activeProfileId,
		active_config: { ...current.active_config, ...patch },
	};
}

export function applyProfileSelection(
	current: ClientConfigResponse,
	profileId: string,
): ClientConfigResponse {
	const profile = current.profiles.find((item) => item.id === profileId);
	if (!profile) {
		return { ...current, active_profile_id: profileId };
	}
	return {
		...current,
		active_profile_id: profileId,
		active_config: {
			...current.active_config,
			profile_name: profile.name,
			server_addr: profile.server_addr,
			transport: profile.transport,
			auth_token: profile.auth_token,
			listen_addr: profile.listen_addr,
			fake_lan_broadcast: profile.fake_lan_broadcast,
		},
	};
}

export function upsertProfile(
	current: ClientConfigResponse,
	profile: ClientProfile,
): ClientConfigResponse {
	const idx = current.profiles.findIndex(
		(item) => item.id === profile.id || item.server_addr === profile.server_addr,
	);
	const profiles = [...current.profiles];
	if (idx >= 0) {
		profiles[idx] = profile;
	} else {
		profiles.push(profile);
	}
	return applyProfileSelection({ ...current, profiles }, profile.id);
}

export function removeProfile(current: ClientConfigResponse, id: string): ClientConfigResponse {
	const profiles = current.profiles.filter((item) => item.id !== id);
	if (current.active_profile_id !== id) {
		return { ...current, profiles };
	}
	const next = profiles[0];
	if (!next) {
		return { ...current, profiles, active_profile_id: null };
	}
	return applyProfileSelection({ ...current, profiles }, next.id);
}

function persistPayload(data: ClientConfigResponse) {
	return {
		active_profile_id: data.active_profile_id,
		active_config: {
			profile_name: data.active_config.profile_name,
			server_addr: data.active_config.server_addr,
			transport: data.active_config.transport,
			auth_token: data.active_config.auth_token,
			listen_addr: data.active_config.listen_addr,
			fake_lan_broadcast: data.active_config.fake_lan_broadcast,
			auto_connect_panel: data.active_config.auto_connect_panel,
			auto_connect: data.active_config.auto_connect,
			token_id: data.active_config.token_id,
			user_id: data.active_config.user_id,
			username: data.active_config.username,
			expires_at: data.active_config.expires_at ?? undefined,
		},
	};
}

let persistTimer: ReturnType<typeof setTimeout> | null = null;
let persistGen = 0;
let persistChain: Promise<void> = Promise.resolve();

async function persistActiveConfig(): Promise<void> {
	const gen = ++persistGen;
	const queryClient = getQueryClient();
	const data = queryClient.getQueryData<ClientConfigResponse>(queryKeys.client.config);
	if (!data) {
		return;
	}
	try {
		await saveClientConfig(persistPayload(data));
	} catch {
		if (gen === persistGen) {
			await queryClient.invalidateQueries({ queryKey: queryKeys.client.config });
		}
	}
}

export function scheduleConfigPersist(): void {
	if (persistTimer) {
		clearTimeout(persistTimer);
	}
	persistTimer = setTimeout(() => {
		persistTimer = null;
		persistChain = persistChain.then(() => persistActiveConfig());
	}, 500);
}

export async function flushConfigPersist(): Promise<void> {
	if (persistTimer) {
		clearTimeout(persistTimer);
		persistTimer = null;
	}
	await persistChain;
	await persistActiveConfig();
}

export function readClientConfig(): ClientConfigResponse {
	return (
		getQueryClient().getQueryData<ClientConfigResponse>(queryKeys.client.config) ??
		EMPTY_CLIENT_CONFIG
	);
}

export function writeClientConfig(
	updater: (current: ClientConfigResponse) => ClientConfigResponse,
): ClientConfigResponse | undefined {
	const queryClient = getQueryClient();
	let next: ClientConfigResponse | undefined;
	queryClient.setQueryData<ClientConfigResponse>(queryKeys.client.config, (current) => {
		if (!current) {
			return current;
		}
		next = updater(current);
		return next;
	});
	return next;
}

export function patchActiveConfig(
	patch: Partial<ClientConfigState>,
	activeProfileId?: string | null,
): void {
	writeClientConfig((current) => mergeActiveConfig(current, patch, activeProfileId));
	scheduleConfigPersist();
}

export async function ensureClientConfig(): Promise<ClientConfigResponse> {
	return getQueryClient().ensureQueryData({
		queryKey: queryKeys.client.config,
		queryFn: getClientConfig,
	});
}

export async function persistProfiles(profiles: ClientProfile[]): Promise<void> {
	await saveClientProfiles(profiles);
}

export function resetConfigPersistForTests(): void {
	if (persistTimer) {
		clearTimeout(persistTimer);
		persistTimer = null;
	}
	persistGen = 0;
	persistChain = Promise.resolve();
}
