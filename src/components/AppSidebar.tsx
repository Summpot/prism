import { Link, useLocation } from "@tanstack/react-router";
import {
	Activity,
	Box,
	Cable,
	ChevronRight,
	Gamepad2,
	Gauge,
	LogOut,
	Radio,
	Settings2,
	ShieldCheck,
	Terminal,
	Unplug,
	Users,
} from "lucide-react";
import { useState } from "react";

import { Github } from "@/components/icons/Github";
import LanguageSwitcher from "@/components/LanguageSwitcher";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
	const { connection, authSession, isAdmin, clearConnection } = usePanelSession();
	const [adminExpanded, setAdminExpanded] = useState(true);

	return (
		<div className="flex h-full flex-col select-none">
			{/* Brand Header */}
			<div className="border-b border-border/80 px-4 py-3 flex-none">
				<div className="flex items-center gap-2.5">
					<img
						src="/logo192.png"
						alt="Prism"
						className="h-8 w-8 rounded-lg object-contain shadow-xs ring-1 ring-primary/20"
					/>
					<div className="min-w-0 flex-1">
						<div className="flex items-center gap-1.5">
							<span className="text-sm font-bold tracking-tight text-foreground">Prism</span>
							<Badge variant="secondary" className="text-[9px] px-1 py-0 h-3.5 uppercase font-mono">
								v0.1
							</Badge>
						</div>
						<p className="truncate text-[10px] text-muted-foreground">{m.brand_tagline()}</p>
					</div>
				</div>
			</div>

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
					<div className="space-y-1 pt-1 border-t border-border/50">
						<button
							type="button"
							onClick={() => setAdminExpanded(!adminExpanded)}
							className="flex w-full items-center justify-between px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-primary hover:opacity-80 transition cursor-pointer"
						>
							<div className="flex items-center gap-1.5">
								<ShieldCheck className="h-3.5 w-3.5 text-primary" />
								<span>{m.nav_admin()}</span>
							</div>
							<div className="flex items-center gap-1">
								<span className="rounded bg-primary/20 px-1 py-0 text-[8.5px] font-bold text-primary">
									ADMIN
								</span>
								<ChevronRight
									className={cn(
										"h-3 w-3 text-muted-foreground transition-transform duration-200",
										adminExpanded ? "rotate-90" : "",
									)}
								/>
							</div>
						</button>

						{adminExpanded ? (
							<div className="space-y-0.5 pl-1.5 border-l border-primary/20 ml-2">
								<SidebarNavItem
									to="/admin"
									exact
									label={m.nav_overview()}
									icon={<Activity className="h-3.5 w-3.5" />}
									onClick={onNavigate}
								/>
								<SidebarNavItem
									to="/admin/nodes"
									label={m.nav_nodes()}
									icon={<Box className="h-3.5 w-3.5" />}
									onClick={onNavigate}
								/>
								<SidebarNavItem
									to="/admin/connections"
									label={m.nav_connections()}
									icon={<Cable className="h-3.5 w-3.5" />}
									onClick={onNavigate}
								/>
								<SidebarNavItem
									to="/admin/tunnel-services"
									label={m.nav_services()}
									icon={<Unplug className="h-3.5 w-3.5" />}
									onClick={onNavigate}
								/>
								<SidebarNavItem
									to="/admin/runtime"
									label={m.nav_runtime()}
									icon={<Gauge className="h-3.5 w-3.5" />}
									onClick={onNavigate}
								/>
								<SidebarNavItem
									to="/admin/users"
									label={m.nav_users()}
									icon={<Users className="h-3.5 w-3.5" />}
									onClick={onNavigate}
								/>
							</div>
						) : null}
					</div>
				) : null}
			</div>

			{/* Footer User / Session Card */}
			<div className="flex-none border-t border-border/80 p-2.5 bg-card/40">
				{connection ? (
					<div className="rounded-lg border border-border/70 bg-background/60 p-2 space-y-1.5">
						<div className="flex items-center gap-2 min-w-0">
							{authSession?.avatar_url ? (
								<img
									src={authSession.avatar_url}
									alt={authSession.username || "User"}
									className="h-7 w-7 rounded-full object-cover ring-1 ring-border flex-none"
								/>
							) : (
								<div className="flex h-7 w-7 items-center justify-center rounded-full bg-muted text-muted-foreground flex-none">
									<Github className="h-4 w-4" />
								</div>
							)}
							<div className="min-w-0 flex-1">
								<div className="flex items-center gap-1.5">
									<span className="text-xs font-semibold text-foreground truncate">
										{authSession?.display_name || authSession?.username || m.session_logged_in()}
									</span>
									{isAdmin ? (
										<span className="rounded bg-emerald-500/15 border border-emerald-500/30 px-1 py-0 text-[8.5px] font-bold text-emerald-400 flex-none">
											Admin
										</span>
									) : (
										<span className="rounded bg-muted px-1 py-0 text-[8.5px] font-medium text-muted-foreground flex-none">
											Member
										</span>
									)}
								</div>
								<div className="truncate font-mono text-[10px] text-muted-foreground">
									{connection.baseUrl}
								</div>
							</div>
						</div>

						<Button
							variant="ghost"
							size="xs"
							onClick={clearConnection}
							className="h-6 w-full gap-1 text-[10px] text-muted-foreground hover:bg-destructive/10 hover:text-destructive cursor-pointer"
						>
							<LogOut className="h-3 w-3" />
							<span>{m.session_disconnect()}</span>
						</Button>
					</div>
				) : (
					<div className="rounded-lg border border-border/70 bg-background/60 p-2 space-y-1.5 text-center">
						<div className="flex items-center justify-center gap-1.5 text-[11px] text-muted-foreground">
							<Radio className="h-3.5 w-3.5 text-muted-foreground/60" />
							<span>{m.session_not_connected()}</span>
						</div>
						<Link
							to="/"
							onClick={onNavigate}
							className="inline-flex h-6 w-full items-center justify-center gap-1 rounded bg-primary/10 px-2 text-[10px] font-semibold text-primary hover:bg-primary/20 transition"
						>
							<span>{m.session_enter_link()}</span>
						</Link>
					</div>
				)}
				<div className="mt-2 flex justify-end">
					<LanguageSwitcher />
				</div>
			</div>
		</div>
	);
}
