import {
	createRootRoute,
	HeadContent,
	Outlet,
	Scripts,
	useLocation,
	useNavigate,
} from "@tanstack/react-router";
import { useEffect } from "react";

import Header from "@/components/Header";
import { PanelSessionProvider } from "@/lib/panelSession";

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
				title: "Prism Connect",
			},
		],
		links: [
			{
				rel: "stylesheet",
				href: appCss,
			},
		],
	}),
	component: RootDocument,
});

function RootContent() {
	const location = useLocation();
	const navigate = useNavigate();

	const isDesktop =
		typeof window !== "undefined" &&
		(Boolean((window as unknown as { __TAURI__?: unknown }).__TAURI__) ||
			Boolean((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__));

	useEffect(() => {
		if (
			typeof window !== "undefined" &&
			(isDesktop || window.location.pathname.includes("_shell.html"))
		) {
			if (location.pathname === "/" || location.pathname === "/_shell.html") {
				void navigate({ to: "/client" });
			}
		}
	}, [isDesktop, location.pathname, navigate]);

	const isClientShell = location.pathname === "/client" || location.pathname === "/_shell.html";

	if (isClientShell) {
		return (
			<div className="h-screen max-h-screen overflow-hidden bg-background text-foreground">
				<Outlet />
			</div>
		);
	}

	return (
		<div className="min-h-screen bg-background text-foreground">
			<div className="mx-auto flex min-h-screen max-w-[1800px]">
				<Header />
				<main className="flex-1 min-w-0 px-4 pt-16 pb-8 md:px-8 xl:px-10 xl:pt-8 xl:pb-8">
					<Outlet />
				</main>
			</div>
		</div>
	);
}

function RootDocument() {
	return (
		<html lang="en" className="dark">
			<head>
				<HeadContent />
			</head>
			<body className="min-h-screen bg-background text-foreground antialiased selection:bg-primary/20 selection:text-primary">
				<PanelSessionProvider>
					<RootContent />
				</PanelSessionProvider>
				<Scripts />
			</body>
		</html>
	);
}
