import { adminRequest } from "@/lib/admin/adminClient";
import type { PanelConnection } from "@/lib/panelConnection";
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
import type {
	ManagedConfigDocument,
	ManagedNodeConfigResponse,
	ManagedNodeSnapshot,
	ManagementStatusResponse,
} from "@/types/cluster";

export function getManagementStatus(connection: PanelConnection) {
	return adminRequest<ManagementStatusResponse>(connection, "/managed/status");
}

export function getManagedNodes(connection: PanelConnection) {
	return adminRequest<ManagedNodeSnapshot[]>(connection, "/managed/nodes");
}

export function getManagedNode(connection: PanelConnection, nodeId: string) {
	return adminRequest<ManagedNodeSnapshot>(
		connection,
		`/managed/nodes/${encodeURIComponent(nodeId)}`,
	);
}

export function getManagedNodeConfig(connection: PanelConnection, nodeId: string) {
	return adminRequest<ManagedNodeConfigResponse>(
		connection,
		`/managed/nodes/${encodeURIComponent(nodeId)}/config`,
	);
}

export function updateManagedNodeConfig(
	connection: PanelConnection,
	nodeId: string,
	desiredConfig: ManagedConfigDocument,
) {
	return adminRequest<ManagedNodeConfigResponse>(
		connection,
		`/managed/nodes/${encodeURIComponent(nodeId)}/config`,
		{
			method: "PUT",
			body: JSON.stringify({ desired_config: desiredConfig }),
		},
	);
}

export function getConnections(connection: PanelConnection) {
	return adminRequest<SessionInfo[]>(connection, "/conns");
}

export function getOptimizerStats(connection: PanelConnection) {
	return adminRequest<OptimizerOverviewResponse>(connection, "/stats/optimizer");
}

export function getTunnelServices(connection: PanelConnection) {
	return adminRequest<ServiceSnapshot[]>(connection, "/tunnel/services");
}

export function triggerReload(connection: PanelConnection) {
	return adminRequest<ReloadResponse>(connection, "/reload", {
		method: "POST",
	});
}

export function getHealth(connection: PanelConnection) {
	return adminRequest<HealthResponse>(connection, "/health");
}

export function getConfigPath(connection: PanelConnection) {
	return adminRequest<ConfigPathResponse>(connection, "/config");
}

export function getAuthProviders(baseUrl: string): Promise<AuthProvidersResponse> {
	return fetch(`${baseUrl}/auth/providers`)
		.then(async (res) => {
			if (!res.ok) {
				return {
					github_enabled: false,
					github_client_id: null,
					mode: "token",
					providers: [],
				};
			}
			const data = (await res.json()) as Record<string, unknown>;
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
		})
		.catch(() => ({
			github_enabled: false,
			github_client_id: null,
			mode: "token",
			providers: [],
		}));
}

export function getGitHubLoginUrl(connection: PanelConnection) {
	return adminRequest<GitHubLoginUrlResponse>(connection, "/auth/github/login");
}

export function exchangeGitHubCode(
	connection: PanelConnection,
	code: string,
	deviceId?: string | null,
) {
	return adminRequest<{
		token: string;
		user: UserRecord;
		token_id: string;
		expires_at_unix_ms?: number | null;
	}>(connection, "/auth/github/exchange", {
		method: "POST",
		body: JSON.stringify({
			code,
			...(deviceId ? { device_id: deviceId } : {}),
		}),
	});
}

export function getAuthSession(connection: PanelConnection) {
	return adminRequest<AuthSessionResponse>(connection, "/auth/session");
}

export function listAuthTokens(connection: PanelConnection) {
	return adminRequest<TokenRecord[]>(connection, "/auth/tokens");
}

export function createAuthToken(connection: PanelConnection, payload: CreateTokenPayload) {
	return adminRequest<CreateTokenResponse>(connection, "/auth/tokens", {
		method: "POST",
		body: JSON.stringify(payload),
	});
}

export function revokeAuthToken(connection: PanelConnection, tokenId: string) {
	return adminRequest<{ ok: boolean }>(connection, `/auth/tokens/${encodeURIComponent(tokenId)}`, {
		method: "DELETE",
	});
}

export function listManagedUsers(connection: PanelConnection) {
	return adminRequest<UserRecord[]>(connection, "/managed/users");
}

export function updateManagedUser(
	connection: PanelConnection,
	userId: string,
	payload: { role?: string; service_rules?: string[] },
) {
	return adminRequest<UserRecord>(connection, `/managed/users/${encodeURIComponent(userId)}`, {
		method: "PUT",
		body: JSON.stringify(payload),
	});
}

export function listMiddlewares(connection: PanelConnection) {
	return adminRequest<MiddlewareItem[]>(connection, "/middlewares");
}

export function getMiddlewareSchema(connection: PanelConnection, name: string) {
	return adminRequest<MiddlewareConfigSchema>(
		connection,
		`/middlewares/${encodeURIComponent(name)}/schema`,
	);
}

export function getMiddlewareConfig(connection: PanelConnection, name: string) {
	return adminRequest<Record<string, any>>(
		connection,
		`/middlewares/${encodeURIComponent(name)}/config`,
	);
}

export function updateMiddlewareConfig(
	connection: PanelConnection,
	name: string,
	config: Record<string, any>,
) {
	return adminRequest<{ status: string; name: string; config: Record<string, any> }>(
		connection,
		`/middlewares/${encodeURIComponent(name)}/config`,
		{
			method: "PUT",
			body: JSON.stringify(config),
		},
	);
}

export function resetMiddlewareConfig(connection: PanelConnection, name: string) {
	return adminRequest<{ status: string; name: string; reset: boolean }>(
		connection,
		`/middlewares/${encodeURIComponent(name)}/config/reset`,
		{
			method: "POST",
		},
	);
}
