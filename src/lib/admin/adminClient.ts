import type { PanelConnection } from "@/lib/panelConnection";

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

export async function adminRequest<T>(
	connection: PanelConnection,
	path: string,
	init?: RequestInit,
): Promise<T> {
	const kind = connection.kind ?? "bearer";
	const authHeaders: Record<string, string> = {};

	if (kind === "desktop-token") {
		if (connection.token?.trim()) {
			authHeaders["X-Prism-Desktop-Token"] = connection.token.trim();
		}
	} else if (kind === "console-cookie") {
		// Browser sends prism_console_session cookie automatically; credentials: "include"
	} else if (connection.token?.trim()) {
		authHeaders["Authorization"] = `Bearer ${connection.token.trim()}`;
	}

	const response = await fetch(`${connection.baseUrl}${path}`, {
		...init,
		credentials: kind === "console-cookie" ? "include" : (init?.credentials ?? "same-origin"),
		headers: {
			"Content-Type": "application/json",
			...authHeaders,
			...init?.headers,
		},
	});

	if (!response.ok) {
		const text = await response.text();
		let message = text || `Request failed with status ${response.status}`;
		try {
			const parsed = JSON.parse(text) as { error?: string };
			if (parsed.error) {
				message = parsed.error;
			}
		} catch {
			// keep raw text
		}
		throw new AdminApiError(message, response.status);
	}

	if (response.status === 204) {
		return undefined as T;
	}

	const text = await response.text();
	if (!text) {
		return undefined as T;
	}

	return JSON.parse(text) as T;
}
