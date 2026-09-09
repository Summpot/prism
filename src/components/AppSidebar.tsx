import { Link, useLocation } from "@tanstack/react-router";
import {
	Activity,
	Box,
	Cable,
	Gamepad2,
	Gauge,
	Settings2,
	Terminal,
	Unplug,
	Users,
} from "lucide-react";

import LanguageSwitcher from "@/components/LanguageSwitcher";
import { isDesktopApp } from "@/lib/desktopWindow";
import { usePanelSession } from "@/lib/panelSession";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

interface NavItemProps {
	to: string;
	label: string;
	icon: React.ReactNode;
	badge?: string;
	onClick?: () => void;
	exact?: boolean;
}

function SidebarNavItem({ to, label, icon, badge, onClick, exact }: NavItemProps) {
	const location = useLocation();

	const isActive = (() => {
		if (exact || to === "/") {
			return location.pathname === to;
		}
		return location.pathname === to || location.pathname.startsWith(to + "/");
	})();

	return (
		<Link
			to={to}
			onClick={onClick}
			className={cn(
				"group flex items-center justify-between rounded-lg px-2.5 py-2 text-xs font-medium transition-all",
				isActive
					? "bg-primary/15 text-primary font-semibold shadow-xs ring-1 ring-primary/25"
					: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
			)}
		>
			<div className="flex items-center gap-2.5 min-w-0">
				<div
					className={cn(
						"flex h-4 w-4 items-center justify-center transition-colors",
						isActive ? "text-primary" : "text-muted-foreground group-hover:text-foreground",
					)}
				>
					{icon}
				</div>
				<span className="truncate">{label}</span>
			</div>
			{badge ? (
				<span
					className={cn(
						"rounded px-1.5 py-0.2 text-[9px] font-semibold font-mono",
						isActive ? "bg-primary/20 text-primary" : "bg-muted text-muted-foreground",
					)}
				>
					{badge}
				</span>
			) : null}
		</Link>
	);
}

export function AppSidebarContent({ onNavigate }: { onNavigate?: () => void }) {
	const { isAdmin } = usePanelSession();

	return (
		<div className="flex h-full flex-col select-none">
			{/* Navigation Section List */}
			<div className="flex-1 min-h-0 overflow-y-auto px-3 py-3 space-y-4 scrollbar-thin">
				{/* 1. Client Category (Always visible to all users) */}
				<div className="space-y-1">
					<div className="px-2 pb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground/70">
						{m.nav_client()}
					</div>
					<SidebarNavItem
						to="/"
						exact
						label={m.nav_connection()}
						icon={<Gamepad2 className="h-4 w-4" />}
						onClick={onNavigate}
					/>
					<SidebarNavItem
						to="/logs"
						exact
						label={m.nav_logs()}
						icon={<Terminal className="h-4 w-4" />}
						onClick={onNavigate}
					/>
					<SidebarNavItem
						to="/settings"
						exact
						label={m.nav_tunnel_config()}
						icon={<Settings2 className="h-4 w-4" />}
						onClick={onNavigate}
					/>
				</div>

				{/* 2. Admin Control Panel Category (Shown ONLY if isAdmin is true) */}
				{isAdmin ? (
					<div className="space-y-1">
						<div className="px-2 pb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground/70">
							{m.nav_admin()}
						</div>
						<SidebarNavItem
							to="/admin"
							exact
							label={m.nav_overview()}
							icon={<Activity className="h-4 w-4" />}
							onClick={onNavigate}
						/>
						<SidebarNavItem
							to="/admin/nodes"
							label={m.nav_nodes()}
							icon={<Box className="h-4 w-4" />}
							onClick={onNavigate}
						/>
						<SidebarNavItem
							to="/admin/connections"
							label={m.nav_connections()}
							icon={<Cable className="h-4 w-4" />}
							onClick={onNavigate}
						/>
						<SidebarNavItem
							to="/admin/tunnel-services"
							label={m.nav_services()}
							icon={<Unplug className="h-4 w-4" />}
							onClick={onNavigate}
						/>
						<SidebarNavItem
							to="/admin/runtime"
							label={m.nav_runtime()}
							icon={<Gauge className="h-4 w-4" />}
							onClick={onNavigate}
						/>
						<SidebarNavItem
							to="/admin/users"
							label={m.nav_users()}
							icon={<Users className="h-4 w-4" />}
							onClick={onNavigate}
						/>
					</div>
				) : null}
			</div>

			{/* Non-desktop browser fallback for language switcher */}
			{!isDesktopApp() ? (
				<div className="flex-none border-t border-border/50 p-2 flex justify-end">
					<LanguageSwitcher />
				</div>
			) : null}
		</div>
	);
}
