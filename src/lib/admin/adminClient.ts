import { invokeTauri, isDesktopApp } from "@/lib/desktopWindow";
import { isTunnelAdminConnection, type PanelConnection } from "@/lib/panelConnection";

export class AdminApiError extends Error {
	status: number;

	constructor(message: string, status: number) {
		super(message);
		this.name = "AdminApiError";
		this.status = status;
	}
}

// Backwards compatibility alias
export const ManagementApiError = AdminApiError;

interface AdminIpcResponse {
	status: number;
	body: string;
}

interface AdminRpcIpcResponse {
	ok: boolean;
	body: unknown;
	status: number;
	code?: string | null;
	message?: string | null;
}

function authHeadersFor(connection: PanelConnection): Record<string, string> {
	const kind = connection.kind ?? "bearer";
	const headers: Record<string, string> = {};
	if (kind === "desktop-token") {
		if (connection.token?.trim()) {
			headers["X-Prism-Desktop-Token"] = connection.token.trim();
		}
	} else if (kind === "console-cookie" || kind === "tunnel-admin") {
		if (kind === "tunnel-admin" && connection.token?.trim()) {
			headers["Authorization"] = `Bearer ${connection.token.trim()}`;
		}
	} else if (connection.token?.trim()) {
		headers["Authorization"] = `Bearer ${connection.token.trim()}`;
	}
	return headers;
}

function throwIfNotOk(status: number, text: string): void {
	if (status >= 200 && status < 300) return;
	let message = text || `Request failed with status ${status}`;
	try {
		const parsed = JSON.parse(text) as { error?: string };
		if (parsed.error) {
			message = parsed.error;
		}
	} catch {
		// keep raw text
	}
	throw new AdminApiError(message, status);
}

function parseBody<T>(status: number, text: string): T {
	if (status === 204 || !text) {
		return undefined as T;
	}
	return JSON.parse(text) as T;
}

function decodePath(path: string): { params: string[] } {
	const trimmed = path.startsWith("/") ? path : `/${path}`;
	const params = trimmed.split("/").filter(Boolean).map(decodeURIComponent);
	return { params };
}

/** Map leftover HTTP paths onto `$admin` RPC method names. Local adapter only. */
export function httpToAdminRpc(
	path: string,
	method: string,
	body?: string,
): { method: string; payload: Record<string, unknown> } {
	const verb = method.toUpperCase();
	const { params } = decodePath(path);
	const json = (): Record<string, unknown> => {
		if (!body) return {};
		try {
			const parsed = JSON.parse(body) as unknown;
			if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
				return parsed as Record<string, unknown>;
			}
		} catch {
			// ignore
		}
		return {};
	};

	if (params[0] === "health") return { method: "health", payload: {} };
	if (params[0] === "config") return { method: "config_path", payload: {} };
	if (params[0] === "conns") return { method: "connections", payload: {} };
	if (params[0] === "tunnel" && params[1] === "services") {
		return { method: "tunnel_services", payload: {} };
	}
	if (params[0] === "stats" && params[1] === "optimizer") {
		return { method: "optimizer_stats", payload: {} };
	}
	if (params[0] === "reload") return { method: "reload", payload: {} };

	if (params[0] === "auth" && params[1] === "providers") {
		return { method: "auth.providers", payload: {} };
	}
	if (params[0] === "auth" && params[1] === "github" && params[2] === "login") {
		return { method: "auth.github.login", payload: {} };
	}
	if (params[0] === "auth" && params[1] === "github" && params[2] === "exchange") {
		return { method: "auth.github.exchange", payload: json() };
	}
	if (params[0] === "auth" && params[1] === "session") {
		return { method: "auth.session", payload: {} };
	}
	if (params[0] === "auth" && params[1] === "tokens" && params.length === 2 && verb === "GET") {
		return { method: "auth.tokens.list", payload: {} };
	}
	if (params[0] === "auth" && params[1] === "tokens" && params.length === 2 && verb === "POST") {
		return { method: "auth.tokens.create", payload: json() };
	}
	if (params[0] === "auth" && params[1] === "tokens" && params.length === 3 && verb === "DELETE") {
		return { method: "auth.tokens.revoke", payload: { token_id: params[2] } };
	}

	if (params[0] === "managed" && params[1] === "status") {
		return { method: "managed.status", payload: {} };
	}
	if (params[0] === "managed" && params[1] === "nodes" && params.length === 2) {
		return { method: "managed.nodes", payload: {} };
	}
	if (params[0] === "managed" && params[1] === "nodes" && params.length === 3) {
		return { method: "managed.node", payload: { node_id: params[2] } };
	}
	if (
		params[0] === "managed" &&
		params[1] === "nodes" &&
		params[3] === "config" &&
		verb === "GET"
	) {
		return { method: "managed.node.config", payload: { node_id: params[2] } };
	}
	if (
		params[0] === "managed" &&
		params[1] === "nodes" &&
		params[3] === "config" &&
		verb === "PUT"
	) {
		return {
			method: "managed.node.config.put",
			payload: { node_id: params[2], ...json() },
		};
	}
	if (params[0] === "managed" && params[1] === "users" && params.length === 2) {
		return { method: "managed.users", payload: {} };
	}
	if (params[0] === "managed" && params[1] === "users" && params.length === 3 && verb === "PUT") {
		return {
			method: "managed.user.put",
			payload: { user_id: params[2], ...json() },
		};
	}

	if (params[0] === "middlewares" && params.length === 1) {
		return { method: "middlewares.list", payload: {} };
	}
	if (params[0] === "middlewares" && params[2] === "schema") {
		return { method: "middlewares.schema", payload: { name: params[1] } };
	}
	if (
		params[0] === "middlewares" &&
		params[2] === "config" &&
		params[3] === "reset" &&
		verb === "POST"
	) {
		return { method: "middlewares.config.reset", payload: { name: params[1] } };
	}
	if (params[0] === "middlewares" && params[2] === "config" && verb === "GET") {
		return { method: "middlewares.config", payload: { name: params[1] } };
	}
	if (params[0] === "middlewares" && params[2] === "config" && verb === "PUT") {
		return {
			method: "middlewares.config.put",
			payload: { name: params[1], config: json() },
		};
	}

	throw new AdminApiError(`unsupported in-band admin path ${verb} ${path}`, 400);
}

export async function adminRequest<T>(
	connection: PanelConnection,
	path: string,
	init?: RequestInit,
): Promise<T> {
	const kind = connection.kind ?? "bearer";
	const authHeaders = authHeadersFor(connection);
	const method = (init?.method ?? "GET").toString().toUpperCase();
	const body = typeof init?.body === "string" ? init.body : undefined;
	const headers: Record<string, string> = {
		"Content-Type": "application/json",
		...authHeaders,
	};
	if (init?.headers && !Array.isArray(init.headers) && !(init.headers instanceof Headers)) {
		Object.assign(headers, init.headers);
	}

	if (isDesktopApp() && isTunnelAdminConnection(connection)) {
		const rpc = httpToAdminRpc(path, method, body);
		const result = await invokeTauri<AdminRpcIpcResponse>("admin_rpc", {
			payload: {
				method: rpc.method,
				payload: rpc.payload,
				token: connection.token || null,
			},
		});
		const text =
			typeof result.body === "string" ? result.body : JSON.stringify(result.body ?? "");
		throwIfNotOk(result.status || (result.ok ? 200 : 500), result.message ? JSON.stringify({ error: result.message }) : text);
		if (result.body === undefined || result.body === null || result.body === "") {
			return undefined as T;
		}
		return result.body as T;
	}

	if (isDesktopApp()) {
		const result = await invokeTauri<AdminIpcResponse>("admin_request", {
			payload: {
				base_url: connection.baseUrl || "",
				path,
				method,
				headers,
				body: body ?? null,
			},
		});
		throwIfNotOk(result.status, result.body);
		return parseBody<T>(result.status, result.body);
	}

	const response = await fetch(`${connection.baseUrl}${path}`, {
		...init,
		credentials: kind === "console-cookie" ? "include" : (init?.credentials ?? "same-origin"),
		headers: {
			...headers,
			...init?.headers,
		},
	});

	const text = await response.text();
	throwIfNotOk(response.status, text);
	return parseBody<T>(response.status, text);
}
