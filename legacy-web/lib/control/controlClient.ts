import { invokeTauri, isTauriContext } from "@/lib/appWindow";
import { isTunnelControlConnection, type PanelConnection } from "@/lib/panelConnection";

export class ControlApiError extends Error {
	status: number;

	constructor(message: string, status: number) {
		super(message);
		this.name = "ControlApiError";
		this.status = status;
	}
}

interface HttpIpcResponse {
	status: number;
	body: string;
}

interface ControlRpcIpcResponse {
	ok: boolean;
	body: unknown;
	status: number;
	code?: string | null;
	message?: string | null;
}

export interface ControlInvocation {
	rpc: {
		method: string;
		payload?: Record<string, unknown>;
	};
	http?: {
		path: string;
		method?: string;
		body?: unknown;
	};
}

function authHeadersFor(connection: PanelConnection): Record<string, string> {
	const kind = connection.kind ?? "bearer";
	const headers: Record<string, string> = {};
	if (kind === "console-cookie" || kind === "tunnel-control") {
		if (connection.token?.trim()) {
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
	throw new ControlApiError(message, status);
}

function parseBody<T>(status: number, text: string): T {
	if (status === 204 || !text) {
		return undefined as T;
	}
	return JSON.parse(text) as T;
}

/**
 * Invoke in-band `$control` RPC directly through Tauri IPC.
 * Performs direct method dispatch without pseudo-REST translation.
 */
export async function invokeControlRpc<T>(
	method: string,
	payload: Record<string, unknown> = {},
	token?: string | null,
): Promise<T> {
	const result = await invokeTauri<ControlRpcIpcResponse>("control_rpc", {
		payload: {
			method,
			payload,
			token: token || null,
		},
	});

	if (!result.ok) {
		const message = result.message || "Control RPC request failed";
		throw new ControlApiError(message, result.status || 500);
	}

	return result.body as T;
}

/**
 * Perform out-of-band HTTP request to remote admin endpoint.
 */
export async function invokeHttp<T>(
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

	if (isTauriContext()) {
		const result = await invokeTauri<HttpIpcResponse>("admin_request", {
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

/**
 * High-level control plane request dispatcher:
 * - If connection is tunnel-control: directly calls in-band RPC via invokeControlRpc.
 * - Otherwise: calls remote HTTP admin endpoint.
 */
export async function requestControl<T>(
	connection: PanelConnection,
	invocation: ControlInvocation,
): Promise<T> {
	if (isTauriContext() && isTunnelControlConnection(connection)) {
		return invokeControlRpc<T>(
			invocation.rpc.method,
			invocation.rpc.payload ?? {},
			connection.token,
		);
	}

	const http = invocation.http ?? {
		path: `/${invocation.rpc.method.replace(/\./g, "/")}`,
		method: "POST",
		body: invocation.rpc.payload,
	};

	const bodyStr =
		typeof http.body === "string"
			? http.body
			: http.body !== undefined
				? JSON.stringify(http.body)
				: undefined;

	return invokeHttp<T>(connection, http.path, {
		method: http.method ?? (bodyStr ? "POST" : "GET"),
		body: bodyStr,
	});
}
