import { requestControl } from "@/lib/control/controlClient";
import { normalizeBaseUrl, type PanelConnection } from "@/lib/panelConnection";
import type {
	AuthProvidersResponse,
	AuthSessionResponse,
	ConfigPathResponse,
	CreateTokenPayload,
	CreateTokenResponse,
	GitHubLoginUrlResponse,
	HealthResponse,
	MiddlewareConfigSchema,
	MiddlewareItem,
	OptimizerOverviewResponse,
	ReloadResponse,
	ServiceSnapshot,
	SessionInfo,
	TokenRecord,
	UserRecord,
} from "@/types/admin";

export type { ServiceSnapshot, SessionInfo, TokenRecord, UserRecord } from "@/types/admin";

export function getConnections(connection: PanelConnection) {
	return requestControl<SessionInfo[]>(connection, {
		rpc: { method: "connections" },
		http: { path: "/conns", method: "GET" },
	});
}

export function getOptimizerStats(connection: PanelConnection) {
	return requestControl<OptimizerOverviewResponse>(connection, {
		rpc: { method: "optimizer_stats" },
		http: { path: "/stats/optimizer", method: "GET" },
	});
}

export function getTunnelServices(connection: PanelConnection) {
	return requestControl<ServiceSnapshot[]>(connection, {
		rpc: { method: "tunnel_services" },
		http: { path: "/tunnel/services", method: "GET" },
	});
}

export function triggerReload(connection: PanelConnection) {
	return requestControl<ReloadResponse>(connection, {
		rpc: { method: "reload" },
		http: { path: "/reload", method: "POST" },
	});
}

export function getHealth(connection: PanelConnection) {
	return requestControl<HealthResponse>(connection, {
		rpc: { method: "health" },
		http: { path: "/health", method: "GET" },
	});
}

export function getConfigPath(connection: PanelConnection) {
	return requestControl<ConfigPathResponse>(connection, {
		rpc: { method: "config_path" },
		http: { path: "/config", method: "GET" },
	});
}

export async function getAuthProviders(
	target: PanelConnection | string,
): Promise<AuthProvidersResponse> {
	const connection: PanelConnection =
		typeof target === "string" ? { baseUrl: normalizeBaseUrl(target), token: "" } : target;
	const data = await requestControl<Record<string, unknown>>(connection, {
		rpc: { method: "auth.providers" },
		http: { path: "/auth/providers", method: "GET" },
	});
	const github_enabled = Boolean(data.github_enabled ?? data.github);
	const mode = (data.mode as string) ?? "token";
	let providers = Array.isArray(data.providers) ? (data.providers as string[]) : [];
	if (providers.length === 0) {
		if (github_enabled && mode !== "token") providers.push("github");
	}
	return {
		github_enabled,
		github_client_id: (data.github_client_id as string) ?? null,
		mode,
		providers,
	};
}

export function getGitHubLoginUrl(connection: PanelConnection, state?: string) {
	return requestControl<GitHubLoginUrlResponse>(connection, {
		rpc: { method: "auth.github.login", payload: state ? { state } : {} },
		http: {
			path: state ? `/auth/github/login?state=${encodeURIComponent(state)}` : "/auth/github/login",
			method: "GET",
		},
	});
}

export function exchangeGitHubCode(
	connection: PanelConnection,
	code: string,
	deviceId?: string | null,
	state?: string | null,
) {
	const payload: Record<string, unknown> = {
		code,
		...(deviceId ? { device_id: deviceId } : {}),
		...(state ? { state } : {}),
	};
	return requestControl<{
		token: string;
		user: UserRecord;
		token_id: string;
		expires_at_unix_ms?: number | null;
	}>(connection, {
		rpc: { method: "auth.github.exchange", payload },
		http: { path: "/auth/github/exchange", method: "POST", body: payload },
	});
}

export function getAuthSession(connection: PanelConnection) {
	return requestControl<AuthSessionResponse>(connection, {
		rpc: { method: "auth.session" },
		http: { path: "/auth/session", method: "GET" },
	});
}

export function listAuthTokens(connection: PanelConnection) {
	return requestControl<TokenRecord[]>(connection, {
		rpc: { method: "auth.tokens.list" },
		http: { path: "/auth/tokens", method: "GET" },
	});
}

export function createAuthToken(connection: PanelConnection, payload: CreateTokenPayload) {
	return requestControl<CreateTokenResponse>(connection, {
		rpc: {
			method: "auth.tokens.create",
			payload: {
				name: payload.name,
				token_type: payload.token_type,
				...(payload.expires_in_days ? { expires_in_days: payload.expires_in_days } : {}),
			},
		},
		http: { path: "/auth/tokens", method: "POST", body: payload },
	});
}

export function revokeAuthToken(connection: PanelConnection, tokenId: string) {
	return requestControl<{ ok: boolean }>(connection, {
		rpc: { method: "auth.tokens.revoke", payload: { token_id: tokenId } },
		http: { path: `/auth/tokens/${encodeURIComponent(tokenId)}`, method: "DELETE" },
	});
}

export function listUsers(connection: PanelConnection) {
	return requestControl<UserRecord[]>(connection, {
		rpc: { method: "auth.users" },
		http: { path: "/auth/users", method: "GET" },
	});
}

export function updateUser(
	connection: PanelConnection,
	userId: string,
	payload: { role?: string; service_rules?: string[] },
) {
	return requestControl<UserRecord>(connection, {
		rpc: { method: "auth.user.put", payload: { user_id: userId, ...payload } },
		http: { path: `/auth/users/${encodeURIComponent(userId)}`, method: "PUT", body: payload },
	});
}

export const listManagedUsers = listUsers;
export const updateManagedUser = updateUser;

export function listMiddlewares(connection: PanelConnection) {
	return requestControl<MiddlewareItem[]>(connection, {
		rpc: { method: "middlewares.list" },
		http: { path: "/middlewares", method: "GET" },
	});
}

export function getMiddlewareSchema(connection: PanelConnection, name: string) {
	return requestControl<MiddlewareConfigSchema>(connection, {
		rpc: { method: "middlewares.schema", payload: { name } },
		http: { path: `/middlewares/${encodeURIComponent(name)}/schema`, method: "GET" },
	});
}

export function getMiddlewareConfig(connection: PanelConnection, name: string) {
	return requestControl<Record<string, any>>(connection, {
		rpc: { method: "middlewares.config", payload: { name } },
		http: { path: `/middlewares/${encodeURIComponent(name)}/config`, method: "GET" },
	});
}

export function updateMiddlewareConfig(
	connection: PanelConnection,
	name: string,
	config: Record<string, any>,
) {
	return requestControl<{ status: string; name: string; config: Record<string, any> }>(
		connection,
		{
			rpc: { method: "middlewares.config.put", payload: { name, config } },
			http: { path: `/middlewares/${encodeURIComponent(name)}/config`, method: "PUT", body: config },
		},
	);
}

export function resetMiddlewareConfig(connection: PanelConnection, name: string) {
	return requestControl<{ status: string; name: string; reset: boolean }>(connection, {
		rpc: { method: "middlewares.config.reset", payload: { name } },
		http: { path: `/middlewares/${encodeURIComponent(name)}/config/reset`, method: "POST" },
	});
}
