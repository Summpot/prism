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

	if (isDesktopApp()) {
		const result = await invokeTauri<AdminIpcResponse>("admin_request", {
			payload: {
				base_url: connection.baseUrl || "",
				path,
				method,
				headers,
				body: body ?? null,
				via_tunnel: isTunnelAdminConnection(connection),
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
