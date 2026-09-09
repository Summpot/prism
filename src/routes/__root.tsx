import {
	createRootRoute,
	HeadContent,
	Outlet,
	Scripts,
	useLocation,
	useNavigate,
} from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";

import Header from "@/components/Header";
import LanguageSwitcher from "@/components/LanguageSwitcher";
import { ClientModals } from "@/components/client/ClientModals";
import { Button } from "@/components/ui/button";
import { ClientProvider } from "@/context/ClientContext";
import { setupDeepLinkListener } from "@/lib/deepLink";
import { exchangeGitHubCode, getClientStatus } from "@/lib/managementApi";
import {
	closeWindow,
	isDesktopApp,
	isWindowMaximized,
	minimizeWindow,
	toggleMaximizeWindow,
} from "@/lib/desktopWindow";
import { normalizeBaseUrl } from "@/lib/panelConnection";
import { PanelSessionProvider, usePanelSession } from "@/lib/panelSession";
import { m } from "@/paraglide/messages";
import { getLocale } from "@/paraglide/runtime";

import appCss from "../styles.css?url";

export const Route = createRootRoute({
	head: () => ({
		meta: [
			{
				charSet: "utf-8",
			},
			{
				name: "viewport",
				content: "width=device-width, initial-scale=1",
			},
			{
				title: "Prism",
			},
		],
		links: [
			{
				rel: "stylesheet",
				href: appCss,
			},
			{
				rel: "icon",
				href: "/favicon.ico",
			},
			{
				rel: "apple-touch-icon",
				href: "/logo192.png",
			},
		],
	}),
	component: RootDocument,
});

function DesktopTitleBar() {
	const [isMaximized, setIsMaximized] = useState(false);

	useEffect(() => {
		let mounted = true;
		const checkMaximized = async () => {
			const max = await isWindowMaximized();
			if (mounted) setIsMaximized(max);
		};
		void checkMaximized();

		window.addEventListener("resize", checkMaximized);
		return () => {
			mounted = false;
			window.removeEventListener("resize", checkMaximized);
		};
	}, []);

	const handleToggleMaximize = async () => {
		await toggleMaximizeWindow();
		const max = await isWindowMaximized();
		setIsMaximized(max);
	};

	return (
		<div
			data-tauri-drag-region
			className="h-8 flex-none select-none border-b border-border/80 bg-card/80 px-2.5 flex items-center justify-between cursor-default z-50 backdrop-blur"
		>
			<div className="flex items-center gap-2" data-tauri-drag-region>
				<img
					src="/logo192.png"
					alt="Prism"
					className="h-4 w-4 rounded object-contain"
					data-tauri-drag-region
				/>
				<span
					className="text-xs font-semibold text-foreground tracking-tight"
					data-tauri-drag-region
				>
					Prism
				</span>
			</div>

			<div className="flex-1 h-full" data-tauri-drag-region />

			{/* Window Controls & Language Switcher */}
			<div className="flex items-center gap-0.5 -mr-1">
				<LanguageSwitcher className="mr-1" />
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={() => void minimizeWindow()}
					className="h-6 w-7 text-muted-foreground hover:bg-accent hover:text-foreground rounded cursor-pointer"
					title={m.window_minimize()}
					aria-label="Minimize window"
				>
					<svg
						className="h-3 w-3"
						viewBox="0 0 12 12"
						fill="none"
						stroke="currentColor"
						strokeWidth="1.2"
					>
						<line x1="2" y1="6" x2="10" y2="6" />
					</svg>
				</Button>
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={() => void handleToggleMaximize()}
					className="h-6 w-7 text-muted-foreground hover:bg-accent hover:text-foreground rounded cursor-pointer"
					title={isMaximized ? m.window_restore() : m.window_maximize()}
					aria-label={isMaximized ? "Restore window" : "Maximize window"}
				>
					{isMaximized ? (
						<svg
							className="h-2.5 w-2.5"
							viewBox="0 0 12 12"
							fill="none"
							stroke="currentColor"
							strokeWidth="1.2"
						>
							<path d="M3.5 2.5V1.5A1 1 0 0 1 4.5 0.5H10.5A1 1 0 0 1 11.5 1.5V7.5A1 1 0 0 1 10.5 8.5H9.5" />
							<rect x="0.5" y="3.5" width="7" height="7" rx="1" />
						</svg>
					) : (
						<svg
							className="h-2.5 w-2.5"
							viewBox="0 0 12 12"
							fill="none"
							stroke="currentColor"
							strokeWidth="1.2"
						>
							<rect x="1.5" y="1.5" width="9" height="9" rx="1" />
						</svg>
					)}
				</Button>
				<Button
					variant="ghost"
					size="icon-xs"
					onClick={() => void closeWindow()}
					className="h-6 w-7 text-muted-foreground hover:bg-destructive hover:text-destructive-foreground transition-colors rounded cursor-pointer"
					title={m.window_close_tray()}
					aria-label="Close window to tray"
				>
					<svg
						className="h-3 w-3"
						viewBox="0 0 12 12"
						fill="none"
						stroke="currentColor"
						strokeWidth="1.2"
					>
						<line x1="2.5" y1="2.5" x2="9.5" y2="9.5" />
						<line x1="9.5" y1="2.5" x2="2.5" y2="9.5" />
					</svg>
				</Button>
			</div>
		</div>
	);
}

function RootContent() {
	const location = useLocation();
	const navigate = useNavigate();
	const { connection, saveConnection } = usePanelSession();

	const isDesktop = useMemo(() => isDesktopApp(), []);

	const locationRef = useRef(location);
	const navigateRef = useRef(navigate);
	const connectionRef = useRef(connection);
	const saveConnectionRef = useRef(saveConnection);

	useEffect(() => {
		locationRef.current = location;
		navigateRef.current = navigate;
		connectionRef.current = connection;
		saveConnectionRef.current = saveConnection;
	});

	useEffect(() => {
		return setupDeepLinkListener((payload) => {
			if (payload.kind === "auth") {
				const currentBaseUrl = connectionRef.current?.baseUrl || "";
				saveConnectionRef.current({
					baseUrl: currentBaseUrl,
					token: payload.token,
				});
				window.dispatchEvent(new CustomEvent("prism:deep-link-auth", { detail: payload }));
				if (locationRef.current.pathname === "/login") {
					void navigateRef.current({ to: payload.role === "admin" ? "/admin" : "/" });
				}
			} else if (payload.kind === "auth-code") {
				window.dispatchEvent(
					new CustomEvent("prism:deep-link-exchange-start", {
						detail: { code: payload.code },
					}),
				);
				const doExchange = async () => {
					const candidates: string[] = [];
					if (typeof window !== "undefined") {
						const fromLocal = window.localStorage.getItem("prism_pending_auth_url");
						const fromSession = window.sessionStorage.getItem("prism_pending_auth_url");
						if (fromLocal && !candidates.includes(fromLocal)) candidates.push(fromLocal);
						if (fromSession && !candidates.includes(fromSession)) candidates.push(fromSession);
					}

					// Also check if local tunnel client is running with an active in-band admin bridge
					try {
						const clientSt = await getClientStatus();
						if (clientSt?.admin_url && !candidates.includes(clientSt.admin_url)) {
							candidates.push(clientSt.admin_url);
						}
					} catch {
						// ignore
					}

					if (
						connectionRef.current?.baseUrl &&
						!candidates.includes(connectionRef.current.baseUrl)
					) {
						candidates.push(connectionRef.current.baseUrl);
					}

					// Fallback to default in-band tunnel admin bridge port
					if (!candidates.includes("http://127.0.0.1:18080")) {
						candidates.push("http://127.0.0.1:18080");
					}

					let lastErr: unknown = null;
					for (const targetUrl of candidates) {
						try {
							const norm = normalizeBaseUrl(targetUrl);
							const res = await exchangeGitHubCode({ baseUrl: norm, token: "" }, payload.code);
							if (typeof window !== "undefined") {
								window.localStorage.removeItem("prism_pending_auth_url");
								window.sessionStorage.removeItem("prism_pending_auth_url");
							}
							saveConnectionRef.current({
								baseUrl: norm,
								token: res.token,
							});
							window.dispatchEvent(
								new CustomEvent("prism:deep-link-auth", {
									detail: {
										token: res.token,
										userId: res.user.id,
										username: res.user.username,
										role: res.user.role,
									},
								}),
							);
							if (locationRef.current.pathname === "/login") {
								void navigateRef.current({ to: res.user.role === "admin" ? "/admin" : "/" });
							}
							return;
						} catch (err) {
							lastErr = err;
						}
					}
					console.error("Failed to exchange GitHub OAuth code via deep link:", lastErr);
					window.dispatchEvent(
						new CustomEvent("prism:deep-link-exchange-error", {
							detail: {
								error:
									lastErr instanceof Error
										? lastErr.message
										: "GitHub 授权码兑换凭证失败，验证码可能已失效，请重新发起登录",
							},
						}),
					);
				};
				void doExchange();
			} else if (payload.kind === "profile") {
				window.dispatchEvent(
					new CustomEvent("prism:deep-link-profile", {
						detail: payload.profile,
					}),
				);
			}
		});
	}, []);

	useEffect(() => {
		if (typeof window !== "undefined") {
			if (location.pathname === "/_shell.html") {
				void navigate({ to: "/", replace: true });
			}
		}
	}, [location.pathname, navigate]);

	return (
		<div
			className={`h-screen max-h-screen overflow-hidden flex flex-col bg-background text-foreground ${
				isDesktop ? "border border-border/80" : ""
			}`}
		>
			{/* Custom frameless titlebar in desktop app */}
			{isDesktop ? <DesktopTitleBar /> : null}

			{/* Sidebar + Main Viewport */}
			<div className="flex-1 min-h-0 flex overflow-hidden">
				<Header />
				<main className="flex-1 min-w-0 h-full overflow-hidden bg-background md:pt-0 pt-12 flex flex-col">
					<Outlet />
				</main>
			</div>
		</div>
	);
}

function RootDocument() {
	return (
		<html lang={getLocale()} className="dark">
			<head>
				<HeadContent />
			</head>
			<body className="min-h-screen bg-background text-foreground antialiased selection:bg-primary/20 selection:text-primary">
				<PanelSessionProvider>
					<ClientProvider>
						<RootContent />
						<ClientModals />
					</ClientProvider>
				</PanelSessionProvider>
				<Scripts />
			</body>
		</html>
	);
}
