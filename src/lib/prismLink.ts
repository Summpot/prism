import type { ClientProfile } from "./managementApi";
export type { ClientProfile };

/**
 * Encodes a server profile into a shareable `prism://` link.
 */
export function encodePrismLink(profile: Partial<ClientProfile>): string {
	const server = profile.server_addr || "127.0.0.1:7000";
	const params = new URLSearchParams();

	if (profile.name) {
		params.set("name", profile.name);
	}
	if (profile.transport && profile.transport !== "auto") {
		params.set("transport", profile.transport);
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

export const SUPPORTED_LINK_PROTOCOLS = [
	{ value: "auto://", label: "auto://", transport: "auto" },
	{ value: "wt://", label: "wt://", transport: "webtransport" },
	{ value: "quic://", label: "quic://", transport: "quic" },
	{ value: "tcp://", label: "tcp://", transport: "tcp" },
	{ value: "kcp://", label: "kcp://", transport: "kcp" },
	{ value: "ws://", label: "ws://", transport: "websocket" },
	{ value: "wss://", label: "wss://", transport: "websocket" },
] as const;

/**
 * Extracts transport protocol prefix (if present) and address portion from a connection string.
 * For prism:// configuration links, extracts the underlying transport protocol and server address.
 */
export function extractProtocolAndAddress(raw: string): {
	protocol: string | null;
	address: string;
} {
	const trimmed = raw.trim();
	if (!trimmed) {
		return { protocol: null, address: "" };
	}

	// Handle prism:// invite/configuration links: extract underlying transport protocol and host:port
	if (trimmed.toLowerCase().startsWith("prism://")) {
		const parsed = parsePrismLink(trimmed);
		if (parsed?.server_addr) {
			const transport = parsed.transport || "auto";
			const protocol =
				transport === "auto"
					? "auto://"
					: transport === "webtransport" || transport === "wt"
						? "wt://"
						: transport === "websocket" || transport === "ws"
							? "ws://"
							: transport === "wss"
								? "wss://"
								: `${transport}://`;
			return {
				protocol,
				address: parsed.server_addr,
			};
		}
	}

	const match = trimmed.match(/^([a-zA-Z][a-zA-Z0-9+.-]*:\/\/)(.*)$/);
	if (match) {
		return {
			protocol: match[1].toLowerCase(),
			address: match[2].trim(),
		};
	}

	return { protocol: null, address: trimmed };
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

	// Standard prism:// or transport:// URI (auto://, wt://, quic://, tcp://, kcp://, ws://, wss://)
	const transportMatch = trimmed.match(/^(prism|auto|wt|webtransport|quic|tcp|kcp|ws|wss):\/\/(.*)$/i);
	if (transportMatch) {
		try {
			const scheme = transportMatch[1].toLowerCase();
			const fakeUrl = new URL(trimmed.replace(/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//i, "https://"));
			const server_addr = fakeUrl.host;
			if (!server_addr) {
				return null;
			}

			const defaultTransport =
				scheme === "prism" || scheme === "auto"
					? "auto"
					: scheme === "wt" || scheme === "webtransport"
						? "webtransport"
						: scheme === "ws" || scheme === "wss"
							? "websocket"
							: scheme;
			const name = fakeUrl.searchParams.get("name") || "";
			const transport = fakeUrl.searchParams.get("transport") || defaultTransport;
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

	// Fallback: If user just pasted "relay.example.com" or "relay.example.com:7000"
	if (!trimmed.includes(" ")) {
		// http or https url
		if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
			try {
				const url = new URL(trimmed);
				const host = url.hostname;
				return {
					name: host,
					server_addr: host,
					transport: "auto",
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
				transport: "auto",
				listen_addr: "127.0.0.1:25565",
				fake_lan_broadcast: true,
			};
		}

		if (/^[a-zA-Z0-9.-]+$/.test(trimmed)) {
			return {
				name: trimmed,
				server_addr: trimmed,
				transport: "auto",
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
			serverAddr: "127.0.0.1",
			transport: "auto",
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
				serverAddr: host,
				transport: "auto",
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
			transport: parsed.transport || "auto",
			name: parsed.name,
			listenAddr: parsed.listen_addr || "127.0.0.1:25565",
			fakeLanBroadcast: parsed.fake_lan_broadcast ?? true,
		};
	}

	// Fallback: raw host
	const host = trimmed.includes(":") ? trimmed.split(":")[0] : trimmed;
	return {
		managementUrl: `http://${host || "127.0.0.1"}:8080`,
		serverAddr: trimmed,
		transport: "auto",
		listenAddr: "127.0.0.1:25565",
		fakeLanBroadcast: true,
	};
}
