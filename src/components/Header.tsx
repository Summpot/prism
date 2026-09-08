import { Menu, X } from "lucide-react";
import { useMemo, useState } from "react";

import { AppSidebarContent } from "@/components/AppSidebar";
import { isDesktopApp } from "@/lib/desktopWindow";

export { AppSidebarContent };

export default function Header() {
	const [mobileOpen, setMobileOpen] = useState(false);
	const isDesktop = useMemo(() => isDesktopApp(), []);

	return (
		<>
			{/* Mobile top bar (only in non-desktop browser mode when screen is narrow) */}
			{!isDesktop ? (
				<div className="fixed top-0 right-0 left-0 z-40 flex items-center justify-between border-b border-border bg-background/95 px-4 py-2.5 backdrop-blur md:hidden">
					<div className="flex items-center gap-2.5">
						<button
							type="button"
							onClick={() => setMobileOpen(true)}
							className="rounded-lg p-1.5 text-foreground hover:bg-accent cursor-pointer"
						>
							<Menu className="h-5 w-5" />
						</button>
						<img src="/logo192.png" alt="Prism" className="h-6 w-6 rounded-md object-contain" />
						<span className="text-sm font-semibold text-foreground">Prism</span>
					</div>
				</div>
			) : null}

			{/* Mobile Drawer (browser mode only) */}
			{!isDesktop && mobileOpen ? (
				<div className="fixed inset-0 z-50 md:hidden">
					<button
						type="button"
						aria-label="Close mobile menu backdrop"
						tabIndex={-1}
						className="absolute inset-0 bg-background/80 backdrop-blur-xs cursor-default"
						onClick={() => setMobileOpen(false)}
						onKeyDown={(e) => {
							if (e.key === "Escape") setMobileOpen(false);
						}}
					/>
					<aside className="relative flex h-full w-72 max-w-[85vw] flex-col overflow-y-auto border-r border-border bg-card shadow-2xl">
						<button
							type="button"
							onClick={() => setMobileOpen(false)}
							className="absolute top-3 right-3 z-10 rounded-lg p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground cursor-pointer"
						>
							<X className="h-4 w-4" />
						</button>
						<AppSidebarContent onNavigate={() => setMobileOpen(false)} />
					</aside>
				</div>
			) : null}

			{/* Desktop / Tablet Sidebar: Always flex in desktop app, md:flex in browser */}
			<aside
				className={`${
					isDesktop ? "flex" : "hidden md:flex"
				} w-60 sm:w-64 flex-none flex-col border-r border-border bg-card/60 h-full overflow-hidden`}
			>
				<AppSidebarContent />
			</aside>
		</>
	);
}
