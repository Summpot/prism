import { exchangeGitHubCode, getAuthProviders, getGitHubLoginUrl } from "@/lib/admin/adminApi";
import { resetClientStats, startClient, stopClient } from "@/lib/client/clientIpc";
import { parseDeepLink } from "@/lib/deepLink";
import { openExternalUrl } from "@/lib/desktopWindow";
import {
	TUNNEL_ADMIN_CONNECTION,
	isTunnelAdminConnection,
	normalizeBaseUrl,
	tunnelAdminConnection,
} from "@/lib/panelConnection";
import { SUPPORTED_LINK_PROTOCOLS, resolveRemoteConnection } from "@/lib/prismLink";
import {
	ensureClientConfig,
	flushConfigPersist,
	patchActiveConfig,
	persistProfiles,
	readClientConfig,
	writeClientConfig,
	upsertProfile,
	removeProfile,
	applyProfileSelection,
} from "@/lib/state/clientConfig";
import { useClientUiStore } from "@/lib/state/clientUiStore";
import { usePanelStore } from "@/lib/state/panelStore";
import { getQueryClient } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import {
	invalidateClientQueries,
	refreshClientStatus,
	waitForClientConnected,
} from "@/lib/state/clientRuntime";
import { sessionIsAdmin, shouldAdoptTunnelPanelConnection } from "@/lib/state/session";
import type { AuthSessionResponse } from "@/types/admin";
import type { ClientProfile } from "@/types/client";
import { m } from "@/paraglide/messages";

export type LoginSessionExtra = {
	token_id?: string;
	user_id?: string;
	username?: string;
	display_name?: string | null;
	avatar_url?: string | null;
	role?: string;
	expires_at?: number | null;
	panelUrl?: string;
};

async function withAction<T>(fn: () => Promise<T>): Promise<T | undefined> {
	const ui = useClientUiStore.getState();
	ui.setActionLoading(true);
	ui.setError(null);
	try {
		return await fn();
	} catch (err) {
		ui.setError(err instanceof Error ? err.message : String(err));
		return undefined;
	} finally {
		ui.setActionLoading(false);
	}
}

function applySessionSnapshot(session: AuthSessionResponse): void {
	const connection = usePanelStore.getState().connection;
	getQueryClient().setQueryData(queryKeys.admin.session(connection), session);
}

export function syncTunnelPanelConnection(): void {
	const config = readClientConfig().active_config;
	const token = config.auth_token || "";
	const panel = usePanelStore.getState();
	if (
		!shouldAdoptTunnelPanelConnection(panel.connection, config.auto_connect_panel ?? true, token)
	) {
		return;
	}
	const current = panel.connection;
	if (current && isTunnelAdminConnection(current) && current.token === token) {
		return;
	}
	panel.saveConnection(tunnelAdminConnection(token));
}

export async function persistTunnelAuth(token: string, extra?: LoginSessionExtra): Promise<void> {
	patchActiveConfig({
		auth_token: token,
		token_id: extra?.token_id,
		user_id: extra?.user_id,
		username: extra?.username,
		expires_at: extra?.expires_at ?? undefined,
	});
	const config = readClientConfig().active_config;
	if (config.auto_connect_panel && token) {
		usePanelStore.getState().saveConnection(tunnelAdminConnection(token));
	}
	if (extra?.user_id || extra?.username || extra?.role) {
		const role = extra.role?.toLowerCase() ?? "";
		applySessionSnapshot({
			authenticated: true,
			user_id: extra.user_id ?? null,
			username: extra.username ?? null,
			display_name: extra.display_name ?? extra.username ?? null,
			avatar_url: extra.avatar_url ?? null,
			role: extra.role ?? null,
			is_admin: role === "admin",
		});
	}
	await flushConfigPersist();
}

async function startFromActiveConfig(overrides?: {
	serverAddr?: string;
	transport?: string;
	authToken?: string;
	listenAddr?: string;
	profileName?: string;
}): Promise<void> {
	await flushConfigPersist();
	const snapshot = readClientConfig();
	const cfg = snapshot.active_config;
	await startClient({
		server_addr: overrides?.serverAddr ?? cfg.server_addr,
		transport: overrides?.transport ?? cfg.transport,
		auth_token: overrides?.authToken ?? cfg.auth_token,
		listen_addr: overrides?.listenAddr ?? cfg.listen_addr,
		fake_lan_broadcast: cfg.fake_lan_broadcast,
		profile_id: snapshot.active_profile_id || undefined,
		profile_name: overrides?.profileName ?? (cfg.profile_name || undefined),
	});
}

export async function connectClient(): Promise<void> {
	await withAction(async () => {
		await startFromActiveConfig();
		await waitForClientConnected();
		await invalidateClientQueries();
		const token = readClientConfig().active_config.auth_token;
		if (token) {
			syncTunnelPanelConnection();
			const connection = usePanelStore.getState().connection;
			await getQueryClient().invalidateQueries({
				queryKey: queryKeys.admin.session(connection),
			});
		}
	});
}

export async function disconnectClient(): Promise<void> {
	await withAction(async () => {
		await stopClient();
		await refreshClientStatus();
		await getQueryClient().invalidateQueries({ queryKey: queryKeys.client.logs });
		await getQueryClient().invalidateQueries({ queryKey: queryKeys.client.config });
	});
}

export async function resetStats(): Promise<void> {
	try {
		await resetClientStats();
		await Promise.all([
			getQueryClient().invalidateQueries({ queryKey: queryKeys.client.status }),
			getQueryClient().invalidateQueries({ queryKey: queryKeys.client.config }),
		]);
	} catch (err) {
		useClientUiStore.getState().setError(err instanceof Error ? err.message : String(err));
	}
}

export async function redetectProviders(overrideUrl?: string): Promise<void> {
	const ui = useClientUiStore.getState();
	ui.setCheckingProviders(true);
	ui.setProvidersError(null);
	try {
		const target = overrideUrl?.trim()
			? { baseUrl: normalizeBaseUrl(overrideUrl), token: "" }
			: TUNNEL_ADMIN_CONNECTION;
		const providers = await getAuthProviders(target);
		ui.setProvidersResult(providers);
		if (overrideUrl?.trim()) {
			ui.setAuthServerUrl(normalizeBaseUrl(overrideUrl));
		}
	} catch (err) {
		ui.setProvidersError(err instanceof Error ? err.message : m.client_probe_failed());
	} finally {
		ui.setCheckingProviders(false);
	}
}

export async function startGitHubAuthWithUrl(
	_targetAuthUrl: string,
	targetServerAddr?: string,
): Promise<void> {
	const ui = useClientUiStore.getState();
	ui.setAuthError(null);
	ui.setOauthLoading(true);
	try {
		if (targetServerAddr) {
			patchActiveConfig({ server_addr: targetServerAddr });
		}
		if (typeof window !== "undefined") {
			window.localStorage.removeItem("prism_pending_auth_url");
			window.sessionStorage.removeItem("prism_pending_auth_url");
		}
		const res = await getGitHubLoginUrl(TUNNEL_ADMIN_CONNECTION);
		if (res.url) {
			ui.setOauthWaitingCallback(true);
			await openExternalUrl(res.url);
		}
	} catch (err) {
		ui.setAuthError(err instanceof Error ? err.message : String(err));
		ui.setOauthWaitingCallback(false);
	} finally {
		ui.setOauthLoading(false);
	}
}

export async function finalizeLogin(
	token: string,
	extra?: LoginSessionExtra,
): Promise<{ goAdmin: boolean }> {
	const ui = useClientUiStore.getState();
	if (extra?.role?.toLowerCase() === "admin") {
		ui.setLoginAdminUnlocked(true);
	}
	ui.resetAuthFlow();
	ui.setActionLoading(true);
	try {
		await persistTunnelAuth(token, extra);
		try {
			await startFromActiveConfig({ authToken: token });
		} catch (err) {
			ui.setError(err instanceof Error ? err.message : String(err));
		}
		const status = await waitForClientConnected();
		await invalidateClientQueries();
		let goAdmin = false;
		if (status?.state === "connected") {
			const connection = usePanelStore.getState().connection;
			await getQueryClient().invalidateQueries({
				queryKey: queryKeys.admin.session(connection),
			});
			const session = getQueryClient().getQueryData<AuthSessionResponse>(
				queryKeys.admin.session(connection),
			);
			if (sessionIsAdmin(session) && session?.authenticated) {
				goAdmin = true;
			} else if (extra?.role?.toLowerCase() === "admin") {
				goAdmin = true;
			}
		}
		ui.setRemoteLinkInput("");
		ui.setManualCallbackInput("");
		return { goAdmin };
	} finally {
		ui.setActionLoading(false);
	}
}

export async function handleManualOAuthCallback(input: string): Promise<{ goAdmin: boolean }> {
	const raw = input.trim();
	if (!raw) {
		return { goAdmin: false };
	}

	const ui = useClientUiStore.getState();
	const deep = parseDeepLink(raw);
	if (deep.kind === "auth") {
		return finalizeLogin(deep.token, {
			user_id: deep.userId,
			username: deep.username,
			role: deep.role,
		});
	}

	let code = "";
	if (deep.kind === "auth-code") {
		code = deep.code;
	} else if (raw.includes("code=")) {
		const match = raw.match(/[?&]code=([a-zA-Z0-9_-]+)/);
		if (match) {
			code = match[1];
		}
	} else if (/^[a-zA-Z0-9_-]{16,64}$/.test(raw)) {
		code = raw;
	}

	if (!code) {
		ui.setAuthError(m.client_invalid_github_callback());
		return { goAdmin: false };
	}

	ui.setActionLoading(true);
	ui.setOauthExchanging(true);
	ui.setOauthWaitingCallback(false);
	ui.setAuthError(null);
	ui.setProvidersError(null);

	try {
		const deviceId = (await ensureClientConfig()).device_id || undefined;
		const res = await exchangeGitHubCode(TUNNEL_ADMIN_CONNECTION, code, deviceId);
		return await finalizeLogin(res.token, {
			token_id: res.token_id,
			user_id: res.user?.id,
			username: res.user?.username,
			display_name: res.user?.display_name,
			avatar_url: res.user?.avatar_url,
			role: res.user?.role,
			expires_at: res.expires_at_unix_ms,
		});
	} catch (err) {
		ui.setAuthError(err instanceof Error ? err.message : String(err));
		return { goAdmin: false };
	} finally {
		ui.setActionLoading(false);
		ui.setCheckingProviders(false);
		ui.setOauthExchanging(false);
	}
}

export async function connectFromLink(customLink?: string): Promise<{ goAdmin: boolean }> {
	const ui = useClientUiStore.getState();
	const config = readClientConfig();
	let raw = (customLink ?? ui.remoteLinkInput).trim();
	if (!raw && config.active_config.server_addr) {
		raw = `${ui.linkProtocol}${config.active_config.server_addr}`;
	}

	const deep = parseDeepLink(raw);
	if (deep.kind === "auth-code" || deep.kind === "auth") {
		return handleManualOAuthCallback(raw);
	}

	if (raw && !raw.includes("://")) {
		raw = `${ui.linkProtocol}${raw}`;
	}
	if (!raw) {
		ui.setError(m.client_link_required());
		return { goAdmin: false };
	}

	const resolved = resolveRemoteConnection(raw);
	const targetServerAddr = resolved.serverAddr;
	const targetTransport = resolved.transport || "auto";
	const matched = SUPPORTED_LINK_PROTOCOLS.find((p) => p.transport === targetTransport);
	if (matched) {
		ui.setLinkProtocol(matched.value);
	}

	patchActiveConfig({
		server_addr: targetServerAddr,
		transport: targetTransport,
		...(resolved.name ? { profile_name: resolved.name } : {}),
		...(resolved.listenAddr ? { listen_addr: resolved.listenAddr } : {}),
	});

	ui.setAuthError(null);
	ui.setProvidersError(null);
	ui.setProvidersResult(null);
	ui.setCheckingProviders(true);
	ui.setActionLoading(true);

	try {
		await startFromActiveConfig({
			serverAddr: targetServerAddr,
			transport: targetTransport,
			listenAddr: resolved.listenAddr,
			profileName: resolved.name,
		}).catch((err) => {
			console.warn("Tunnel client start attempt:", err);
		});

		const latestStatus = await waitForClientConnected();
		await invalidateClientQueries();

		const authToken = readClientConfig().active_config.auth_token;
		if (authToken && latestStatus?.state === "connected") {
			syncTunnelPanelConnection();
			const connection = usePanelStore.getState().connection;
			await getQueryClient().invalidateQueries({
				queryKey: queryKeys.admin.session(connection),
			});
			const session = getQueryClient().getQueryData<AuthSessionResponse>(
				queryKeys.admin.session(connection),
			);
			if (session?.authenticated) {
				ui.setProvidersResult(null);
				ui.setProvidersError(null);
				return { goAdmin: sessionIsAdmin(session) };
			}
		}

		let providers = null;
		let probeErr: string | null = null;
		try {
			providers = await getAuthProviders(TUNNEL_ADMIN_CONNECTION);
		} catch (err) {
			probeErr = err instanceof Error ? err.message : m.client_probe_failed();
		}
		const hasValidProviders = Boolean(
			providers &&
			(providers.github_enabled || (providers.providers && providers.providers.length > 0)),
		);
		ui.setProvidersResult(providers);
		ui.setProvidersError(hasValidProviders ? null : probeErr);
		return { goAdmin: false };
	} catch (err) {
		ui.setError(err instanceof Error ? err.message : m.client_connection_failed());
		ui.setProvidersResult(null);
		return { goAdmin: false };
	} finally {
		ui.setCheckingProviders(false);
		ui.setActionLoading(false);
	}
}

export async function selectProfile(id: string): Promise<void> {
	writeClientConfig((current) => applyProfileSelection(current, id));
	const next = readClientConfig();
	const matched = SUPPORTED_LINK_PROTOCOLS.find(
		(item) => item.transport === next.active_config.transport,
	);
	if (matched) {
		useClientUiStore.getState().setLinkProtocol(matched.value);
	}
	await flushConfigPersist();
}

export async function saveActiveProfile(): Promise<void> {
	const current = readClientConfig();
	const cfg = current.active_config;
	const id = current.active_profile_id || `profile-${Date.now()}`;
	const profile: ClientProfile = {
		id,
		name: cfg.profile_name || cfg.server_addr,
		server_addr: cfg.server_addr,
		transport: cfg.transport,
		auth_token: cfg.auth_token,
		listen_addr: cfg.listen_addr,
		fake_lan_broadcast: cfg.fake_lan_broadcast,
	};
	const next = writeClientConfig((snap) => upsertProfile(snap, profile));
	if (next) {
		await persistProfiles(next.profiles);
	}
	await flushConfigPersist();
}

export async function deleteProfile(id: string): Promise<void> {
	const next = writeClientConfig((snap) => removeProfile(snap, id));
	if (next) {
		await persistProfiles(next.profiles);
	}
	await flushConfigPersist();
}

export function applyImportedProfile(profile: Partial<ClientProfile>): void {
	if (!profile.server_addr) {
		return;
	}
	const apply = () => {
		patchActiveConfig({
			server_addr: profile.server_addr,
			...(profile.transport ? { transport: profile.transport } : {}),
			...(profile.name ? { profile_name: profile.name } : {}),
			...(profile.listen_addr ? { listen_addr: profile.listen_addr } : {}),
			...(profile.auth_token ? { auth_token: profile.auth_token } : {}),
			...(typeof profile.fake_lan_broadcast === "boolean"
				? { fake_lan_broadcast: profile.fake_lan_broadcast }
				: {}),
		});
		useClientUiStore.getState().setRemoteLinkInput(profile.server_addr!);
		if (profile.transport) {
			const matched = SUPPORTED_LINK_PROTOCOLS.find((p) => p.transport === profile.transport);
			if (matched) {
				useClientUiStore.getState().setLinkProtocol(matched.value);
			}
		}
	};
	if (getQueryClient().getQueryData(queryKeys.client.config)) {
		apply();
		return;
	}
	void ensureClientConfig().then(apply);
}

export async function signOut(): Promise<void> {
	patchActiveConfig({
		auth_token: "",
		token_id: undefined,
		user_id: undefined,
		username: undefined,
	});
	usePanelStore.getState().clearConnection();
	useClientUiStore.getState().clearProviders();
	useClientUiStore.getState().setLoginAdminUnlocked(false);
	await flushConfigPersist();
	const status = getQueryClient().getQueryData<{ running?: boolean }>(queryKeys.client.status);
	if (status?.running) {
		await disconnectClient();
	}
}
