import { invokeTauri, isTauriContext } from "./appWindow";
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
		const pathOnly = targetString.split(/[?#]/)[0].replace(/\/+$/, "").toLowerCase();
		isAuthEndpoint =
			pathOnly === "auth/callback" ||
			pathOnly === "auth/github/callback" ||
			pathOnly === "oauth/callback" ||
			pathOnly === "login";
	} else if (
		trimmed.toLowerCase().startsWith("http://") ||
		trimmed.toLowerCase().startsWith("https://")
	) {
		try {
			const u = new URL(trimmed);
			const pathOnly = u.pathname.replace(/^\//, "").replace(/\/+$/, "").toLowerCase();
			if (
				pathOnly === "auth/callback" ||
				pathOnly === "auth/github/callback" ||
				pathOnly === "oauth/callback"
			) {
				isAuthEndpoint = true;
				targetString = u.pathname.replace(/^\//, "") + u.search + u.hash;
			}
		} catch {
			// ignore
		}
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
 * Sets up listeners for Tauri deep link events.
 */
export function setupDeepLinkListener(onPayload: DeepLinkHandler): () => void {
	if (!isTauriContext() || typeof window === "undefined") {
		return () => {};
	}

	let unlistenTauriEvent: (() => void) | null = null;
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

	// 1. Check for initial URL on startup (e.g. app launched via deep link)
	invokeTauri<string | null>("client_get_initial_deep_link")
		.then((initialUrl) => {
			if (cancelled || !initialUrl) return;
			handleUrl(initialUrl);
		})
		.catch((err) => {
			console.debug("Failed to get initial deep link from Tauri:", err);
		});

	// 2. Listen to custom DOM event dispatched by Rust webview.eval
	const onDomEvent = (event: Event) => {
		if (cancelled) return;
		const detail = (event as CustomEvent<string>).detail;
		if (typeof detail === "string") {
			handleUrl(detail);
		}
	};
	window.addEventListener("prism-deep-link", onDomEvent);

	// 3. Listen to native Tauri event if global Tauri event listener is available
	const globalTauri = (
		window as unknown as {
			__TAURI__?: {
				event?: {
					listen: (event: string, cb: (e: { payload: unknown }) => void) => Promise<() => void>;
				};
			};
		}
	).__TAURI__;
	if (globalTauri?.event?.listen) {
		globalTauri.event
			.listen("prism://deep-link", (e) => {
				if (cancelled) return;
				if (typeof e.payload === "string") {
					handleUrl(e.payload);
				} else if (Array.isArray(e.payload) && typeof e.payload[0] === "string") {
					handleUrl(e.payload[0]);
				}
			})
			.then((fn) => {
				if (cancelled) {
					fn();
				} else {
					unlistenTauriEvent = fn;
				}
			})
			.catch((err) => {
				console.debug("Failed to listen to prism://deep-link event:", err);
			});
	}

	return () => {
		cancelled = true;
		window.removeEventListener("prism-deep-link", onDomEvent);
		if (unlistenTauriEvent) {
			unlistenTauriEvent();
		}
	};
}
