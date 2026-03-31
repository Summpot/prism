import {
	createRootRoute,
	HeadContent,
	Outlet,
	Scripts,
} from "@tanstack/react-router";

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
				title: "Prism Control Plane",
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

function RootDocument() {
	return (
		<html lang="en" className="bg-slate-950">
			<head>
				<HeadContent />
			</head>
			<body>
				<PanelSessionProvider>
					<div className="min-h-screen bg-[radial-gradient(circle_at_top,_rgba(34,211,238,0.12),_transparent_32%),linear-gradient(180deg,_#020617_0%,_#0f172a_52%,_#020617_100%)] text-white">
						<div className="mx-auto flex min-h-screen max-w-[1800px]">
							<Header />
							<main className="flex-1 px-4 pt-16 pb-6 md:px-8 xl:px-10 xl:pt-8 xl:pb-8">
								<Outlet />
							</main>
						</div>
					</div>
				</PanelSessionProvider>
				<Scripts />
			</body>
		</html>
	);
}
