import { isDesktopApp } from "./desktopWindow";
import { type ClientProfile, parsePrismLink } from "./prismLink";

export type DeepLinkPayload =
	| {
			kind: "auth";
			token: string;
			userId?: string;
			username?: string;
			role?: string;
	  }
	| {
			kind: "auth-code";
			code: string;
			state?: string;
	  }
	| {
			kind: "profile";
			profile: Partial<ClientProfile>;
	  }
	| {
			kind: "unknown";
			raw: string;
	  };

/**
 * Parses any incoming `prism://` URL, HTTP(S) auth callback URL, or raw auth code into structured DeepLinkPayload.
 */
export function parseDeepLink(rawUrl: string): DeepLinkPayload {
	const trimmed = rawUrl.trim();
	if (!trimmed) {
		return { kind: "unknown", raw: trimmed };
	}

	let targetString = trimmed;
	let isAuthEndpoint = false;

	if (trimmed.toLowerCase().startsWith("prism://")) {
		targetString = trimmed.slice("prism://".length);
		isAuthEndpoint =
			targetString.startsWith("auth/callback") ||
			targetString.startsWith("auth/github/callback") ||
			targetString.startsWith("oauth/callback") ||
			targetString.startsWith("login") ||
			targetString.startsWith("auth?") ||
			targetString.startsWith("auth#") ||
			targetString.includes("code=");
	} else if (
		trimmed.toLowerCase().startsWith("http://") ||
		trimmed.toLowerCase().startsWith("https://")
	) {
		try {
			const u = new URL(trimmed);
			if (
				u.searchParams.has("code") ||
				u.searchParams.has("token") ||
				u.pathname.includes("auth")
			) {
				isAuthEndpoint = true;
				targetString = u.pathname.replace(/^\//, "") + u.search + u.hash;
			}
		} catch {
			// ignore
		}
	} else if (trimmed.includes("code=")) {
		isAuthEndpoint = true;
	}

	if (isAuthEndpoint) {
		let token = "";
		let code = "";
		let state: string | undefined;
		let userId: string | undefined;
		let username: string | undefined;
		let role: string | undefined;

		const queryIdx = targetString.indexOf("?");
		const hashIdx = targetString.indexOf("#");

		if (queryIdx !== -1) {
			const queryPart = targetString.slice(
				queryIdx + 1,
				hashIdx !== -1 && hashIdx > queryIdx ? hashIdx : undefined,
			);
			const params = new URLSearchParams(queryPart);
			token = params.get("token") || token;
			code = params.get("code") || code;
			state = params.get("state") || state;
			userId = params.get("user_id") || undefined;
			username = params.get("username") || undefined;
			role = params.get("role") || undefined;
		}

		if (hashIdx !== -1) {
			const hashPart = targetString.slice(
				hashIdx + 1,
				queryIdx !== -1 && queryIdx > hashIdx ? queryIdx : undefined,
			);
			const params = new URLSearchParams(hashPart);
			token = params.get("token") || token;
			code = params.get("code") || code;
			state = params.get("state") || state;
			userId = params.get("user_id") || userId;
			username = params.get("username") || username;
			role = params.get("role") || role;
		}

		if (token) {
			return {
				kind: "auth",
				token,
				userId,
				username,
				role,
			};
		}

		if (code) {
			return {
				kind: "auth-code",
				code,
				state,
			};
		}
	}

	// Check if it matches a node/client profile link
	if (trimmed.toLowerCase().startsWith("prism://")) {
		const profile = parsePrismLink(trimmed);
		if (profile?.server_addr) {
			return {
				kind: "profile",
				profile,
			};
		}
	}

	return { kind: "unknown", raw: trimmed };
}

export type DeepLinkHandler = (payload: DeepLinkPayload) => void;

const consumedDeepLinks = new Set<string>();

/**
 * Sets up listeners for Tauri deep link events in desktop mode.
 */
export function setupDeepLinkListener(onPayload: DeepLinkHandler): () => void {
	if (!isDesktopApp() || typeof window === "undefined") {
		return () => {};
	}

	let unlisten: (() => void) | null = null;
	let cancelled = false;

	const handleUrl = (url: string) => {
		const trimmed = url.trim();
		if (!trimmed) return;
		const storageKey = `prism_deep_link_${trimmed}`;
		if (typeof window !== "undefined" && window.sessionStorage.getItem(storageKey)) {
			return;
		}
		if (consumedDeepLinks.has(trimmed)) {
			return;
		}
		consumedDeepLinks.add(trimmed);
		if (typeof window !== "undefined") {
			window.sessionStorage.setItem(storageKey, "1");
		}
		onPayload(parseDeepLink(trimmed));
	};

	import("@tauri-apps/plugin-deep-link")
		.then(({ onOpenUrl, getCurrent }) => {
			if (cancelled) return;

			// Handle initial URL on startup (e.g. app launched via deep link)
			getCurrent()
				.then((urls) => {
					if (cancelled || !urls || urls.length === 0) return;
					for (const url of urls) {
						if (url) {
							handleUrl(url);
						}
					}
				})
				.catch((err) => {
					console.debug("Failed to get current deep link:", err);
				});

			// Handle URLs received while app is already running
			onOpenUrl((urls) => {
				if (cancelled) return;
				for (const url of urls) {
					if (url) {
						handleUrl(url);
					}
				}
			})
				.then((fn) => {
					if (cancelled) {
						fn();
					} else {
						unlisten = fn;
					}
				})
				.catch((err) => {
					console.debug("Failed to listen to deep links:", err);
				});
		})
		.catch((err) => {
			console.debug("Failed to import @tauri-apps/plugin-deep-link:", err);
		});

	return () => {
		cancelled = true;
		if (unlisten) {
			unlisten();
		}
	};
}
