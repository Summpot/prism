import type { ClientProfile } from "./managementApi";

/**
 * Encodes a server profile into a shareable `prism://` link.
 */
export function encodePrismLink(profile: Partial<ClientProfile>): string {
	const server = profile.server_addr || "127.0.0.1:7000";
	const params = new URLSearchParams();

	if (profile.name) {
		params.set("name", profile.name);
	}
	if (profile.transport && profile.transport !== "quic") {
		params.set("transport", profile.transport);
	}
	if (profile.auth_token) {
		params.set("token", profile.auth_token);
	}
	if (profile.listen_addr && profile.listen_addr !== "127.0.0.1:25565") {
		params.set("listen", profile.listen_addr);
	}
	if (profile.fake_lan_broadcast === false) {
		params.set("fake_lan", "0");
	}

	const queryString = params.toString();
	return `prism://${server}${queryString ? `?${queryString}` : ""}`;
}

/**
 * Parses a `prism://` link or base64-encoded profile string.
 */
export function parsePrismLink(raw: string): Partial<ClientProfile> | null {
	const trimmed = raw.trim();
	if (!trimmed) {
		return null;
	}

	// Try base64 encoded JSON
	if (trimmed.startsWith("prism://base64/")) {
		try {
			const b64 = trimmed.slice("prism://base64/".length);
			const json = atob(b64);
			const parsed = JSON.parse(json) as Partial<ClientProfile>;
			if (parsed.server_addr) {
				return parsed;
			}
		} catch {
			// fallback
		}
	}

	// Standard prism:// URI
	if (trimmed.startsWith("prism://")) {
		try {
			// Replace scheme with https so URL can parse it
			const fakeUrl = new URL(trimmed.replace(/^prism:\/\//i, "https://"));
			const server_addr = fakeUrl.host;
			if (!server_addr) {
				return null;
			}

			const name = fakeUrl.searchParams.get("name") || "";
			const transport = fakeUrl.searchParams.get("transport") || "quic";
			const auth_token = fakeUrl.searchParams.get("token") || "";
			const listen_addr = fakeUrl.searchParams.get("listen") || "127.0.0.1:25565";
			const fakeLanParam = fakeUrl.searchParams.get("fake_lan");
			const fake_lan_broadcast = fakeLanParam !== "0" && fakeLanParam !== "false";

			return {
				name: name || server_addr,
				server_addr,
				transport,
				auth_token,
				listen_addr,
				fake_lan_broadcast,
			};
		} catch {
			return null;
		}
	}

	// Fallback: If user just pasted "relay.example.com:7000" or plain "relay.example.com"
	if (!trimmed.includes(" ")) {
		// http or https url
		if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
			try {
				const url = new URL(trimmed);
				const host = url.hostname;
				return {
					name: host,
					server_addr: `${host}:7000`,
					transport: "quic",
					listen_addr: "127.0.0.1:25565",
					fake_lan_broadcast: true,
				};
			} catch {
				return null;
			}
		}

		if (trimmed.includes(":")) {
			return {
				name: trimmed,
				server_addr: trimmed,
				transport: "quic",
				listen_addr: "127.0.0.1:25565",
				fake_lan_broadcast: true,
			};
		}

		if (/^[a-zA-Z0-9.-]+$/.test(trimmed)) {
			return {
				name: trimmed,
				server_addr: `${trimmed}:7000`,
				transport: "quic",
				listen_addr: "127.0.0.1:25565",
				fake_lan_broadcast: true,
			};
		}
	}

	return null;
}

export interface ResolvedRemoteConnection {
	managementUrl: string;
	serverAddr: string;
	transport: string;
	authToken?: string;
	name?: string;
	listenAddr: string;
	fakeLanBroadcast: boolean;
}

/**
 * Resolves any remote link, server address, or HTTP endpoint into connection targets.
 */
export function resolveRemoteConnection(raw: string): ResolvedRemoteConnection {
	const trimmed = raw.trim();
	if (!trimmed) {
		return {
			managementUrl: "http://127.0.0.1:8080",
			serverAddr: "127.0.0.1:7000",
			transport: "quic",
			listenAddr: "127.0.0.1:25565",
			fakeLanBroadcast: true,
		};
	}

	// Case 1: http or https url
	if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
		try {
			const url = new URL(trimmed);
			const host = url.hostname;
			const cleanUrl = trimmed.replace(/\/+$/, "");
			return {
				managementUrl: cleanUrl,
				serverAddr: `${host}:7000`,
				transport: "quic",
				listenAddr: "127.0.0.1:25565",
				fakeLanBroadcast: true,
			};
		} catch {
			// fall through
		}
	}

	// Case 2: prism:// link
	const parsed = parsePrismLink(trimmed);
	if (parsed?.server_addr) {
		let host = parsed.server_addr;
		if (host.includes(":")) {
			host = host.split(":")[0] || host;
		}
		const managementUrl = `http://${host}:8080`;
		return {
			managementUrl,
			serverAddr: parsed.server_addr,
			transport: parsed.transport || "quic",
			authToken: parsed.auth_token,
			name: parsed.name,
			listenAddr: parsed.listen_addr || "127.0.0.1:25565",
			fakeLanBroadcast: parsed.fake_lan_broadcast ?? true,
		};
	}

	// Fallback: raw host
	const host = trimmed.includes(":") ? trimmed.split(":")[0] : trimmed;
	return {
		managementUrl: `http://${host || "127.0.0.1"}:8080`,
		serverAddr: trimmed.includes(":") ? trimmed : `${trimmed}:7000`,
		transport: "quic",
		listenAddr: "127.0.0.1:25565",
		fakeLanBroadcast: true,
	};
}
