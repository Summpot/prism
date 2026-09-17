import { Link, useLocation } from "@tanstack/react-router";
import {
	Activity,
	ArrowDownUp,
	Cable,
	Gamepad2,
	Gauge,
	Network,
	Radio,
	Server,
	Settings,
	Sliders,
	SlidersHorizontal,
	Terminal,
	Unplug,
	Users,
} from "lucide-react";

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
	collapsed?: boolean;
}

function SidebarNavItem({ to, label, icon, badge, onClick, exact, collapsed }: NavItemProps) {
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
			title={collapsed ? label : undefined}
			className={cn(
				"group flex items-center rounded-lg text-xs font-medium transition-all cursor-pointer",
				collapsed ? "justify-center p-2.5" : "justify-between px-2.5 py-2",
				isActive
					? "bg-primary/15 text-primary font-semibold shadow-xs ring-1 ring-primary/25"
					: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
			)}
		>
			<div className={cn("flex items-center min-w-0", collapsed ? "justify-center" : "gap-2.5")}>
				<div
					className={cn(
						"flex h-4 w-4 items-center justify-center transition-colors flex-none",
						isActive ? "text-primary" : "text-muted-foreground group-hover:text-foreground",
					)}
				>
					{icon}
				</div>
				{!collapsed ? <span className="truncate">{label}</span> : null}
			</div>
			{!collapsed && badge ? (
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

export function AppSidebarContent({
	onNavigate,
	collapsed = false,
}: {
	onNavigate?: () => void;
	collapsed?: boolean;
}) {
	const { isAdmin } = usePanelSession();

	return (
		<div className="flex h-full flex-col select-none">
			{/* Navigation Section List */}
			<div
				className={cn(
					"flex-1 min-h-0 overflow-y-auto py-3 space-y-4 scrollbar-thin",
					collapsed ? "px-1.5" : "px-3",
				)}
			>
				{/* 1. Client Category (Always visible to all users) */}
				<div className="space-y-1">
					{!collapsed ? (
						<div className="px-2 pb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground/70">
							{m.nav_client()}
						</div>
					) : (
						<div className="my-1 border-t border-border/30" />
					)}
					<SidebarNavItem
						to="/"
						exact
						label={m.nav_connection()}
						icon={<Gamepad2 className="h-4 w-4" />}
						onClick={onNavigate}
						collapsed={collapsed}
					/>
					<SidebarNavItem
						to="/traffic"
						exact
						label={m.nav_traffic()}
						icon={<ArrowDownUp className="h-4 w-4" />}
						onClick={onNavigate}
						collapsed={collapsed}
					/>
					<SidebarNavItem
						to="/logs"
						exact
						label={m.nav_logs()}
						icon={<Terminal className="h-4 w-4" />}
						onClick={onNavigate}
						collapsed={collapsed}
					/>
					<SidebarNavItem
						to="/profiles"
						exact
						label={m.nav_tunnel_config()}
						icon={<SlidersHorizontal className="h-4 w-4" />}
						onClick={onNavigate}
						collapsed={collapsed}
					/>
					<SidebarNavItem
						to="/middleware"
						exact
						label={m.nav_middleware()}
						icon={<Sliders className="h-4 w-4" />}
						onClick={onNavigate}
						collapsed={collapsed}
					/>
				</div>

				{/* 2. Admin Control Panel Category (Shown ONLY if isAdmin is true) */}
				{isAdmin ? (
					<div className="space-y-1">
						{!collapsed ? (
							<div className="px-2 pb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground/70">
								{m.nav_admin()}
							</div>
						) : (
							<div className="my-1 border-t border-border/30" />
						)}
						<SidebarNavItem
							to="/admin"
							exact
							label={m.nav_overview()}
							icon={<Activity className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
						<SidebarNavItem
							to="/admin/topology"
							exact
							label={m.nav_topology()}
							icon={<Network className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
						<SidebarNavItem
							to="/admin/connections"
							label={m.nav_connections()}
							icon={<Cable className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
						<SidebarNavItem
							to="/admin/tunnel-services"
							label={m.nav_services()}
							icon={<Unplug className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
						<SidebarNavItem
							to="/admin/traffic"
							label={m.nav_server_traffic()}
							icon={<Server className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
						<SidebarNavItem
							to="/admin/connectors"
							label={m.nav_connector_traffic()}
							icon={<Radio className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
						<SidebarNavItem
							to="/admin/middleware"
							label={m.nav_middleware()}
							icon={<Sliders className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
						<SidebarNavItem
							to="/admin/runtime"
							label={m.nav_runtime()}
							icon={<Gauge className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
						<SidebarNavItem
							to="/admin/users"
							label={m.nav_users()}
							icon={<Users className="h-4 w-4" />}
							onClick={onNavigate}
							collapsed={collapsed}
						/>
					</div>
				) : null}
			</div>

			{/* Sidebar Footer Section */}
			<div className={cn("flex-none border-t border-border/50 p-2", collapsed ? "px-1.5" : "px-2")}>
				<SidebarNavItem
					to="/settings"
					exact
					label={m.nav_settings()}
					icon={<Settings className="h-4 w-4" />}
					onClick={onNavigate}
					collapsed={collapsed}
				/>
			</div>
		</div>
	);
}
