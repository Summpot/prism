import { Link, useLocation } from "@tanstack/react-router";
import {
	Activity,
	Box,
	Cable,
	Gamepad2,
	Gauge,
	Github,
	LogOut,
	Menu,
	Network,
	PlugZap,
	ShieldCheck,
	Unplug,
	Users,
	X,
} from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { usePanelSession } from "@/lib/panelSession";

const navItems = [
	{ to: "/client", label: "Client GUI", icon: <Gamepad2 className="h-4 w-4" /> },
	{ to: "/", label: "Overview", icon: <Activity className="h-4 w-4" /> },
	{ to: "/nodes", label: "Nodes", icon: <Box className="h-4 w-4" /> },
	{ to: "/connections", label: "Connections", icon: <Cable className="h-4 w-4" /> },
	{ to: "/tunnel-services", label: "Tunnel Services", icon: <Unplug className="h-4 w-4" /> },
	{ to: "/runtime", label: "Runtime", icon: <Gauge className="h-4 w-4" /> },
	{ to: "/users", label: "Access & Users", icon: <Users className="h-4 w-4" /> },
	{ to: "/login", label: "Control Plane Login", icon: <PlugZap className="h-4 w-4" /> },
] as const;

function NavLink({
	to,
	label,
	icon,
	onClick,
}: {
	to: string;
	label: string;
	icon: React.ReactNode;
	onClick?: () => void;
}) {
	return (
		<Link
			to={to}
			onClick={onClick}
			className="flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium text-muted-foreground transition hover:bg-accent hover:text-accent-foreground"
			activeProps={{
				className:
					"flex items-center gap-3 rounded-lg bg-primary/10 px-3 py-2 text-sm font-medium text-primary shadow-xs ring-1 ring-primary/20",
			}}
			activeOptions={to === "/" ? { exact: true } : undefined}
		>
			{icon}
			<span>{label}</span>
		</Link>
	);
}

function SidebarContent({ onNavigate }: { onNavigate?: () => void }) {
	const location = useLocation();
	const { connection, clearConnection } = usePanelSession();

	return (
		<div className="flex h-full flex-col">
			<div className="border-b border-border px-6 py-5">
				<div className="flex items-center gap-3">
					<div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary ring-1 ring-primary/20">
						<Network className="h-5 w-5" />
					</div>
					<div>
						<div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
							Prism
						</div>
						<h1 className="text-lg font-bold tracking-tight text-foreground">Control Plane</h1>
					</div>
				</div>
			</div>

			<nav className="flex flex-1 flex-col gap-1.5 px-4 py-4">
				{navItems.map((item) => (
					<NavLink
						key={item.to}
						to={item.to}
						label={item.label}
						icon={item.icon}
						onClick={onNavigate}
					/>
				))}

				<div className="mt-auto pt-4">
					<div className="rounded-xl border border-border bg-card p-4 text-card-foreground shadow-xs">
						<div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wider text-muted-foreground">
							<ShieldCheck className="h-4 w-4 text-emerald-500" />
							<span>Session</span>
						</div>
						{connection ? (
							<div className="mt-3 space-y-2">
								<div className="truncate font-mono text-xs font-semibold text-foreground">
									{connection.baseUrl}
								</div>
								<p className="text-xs text-muted-foreground">Bearer token in local storage.</p>
								<Button
									variant="outline"
									size="sm"
									onClick={clearConnection}
									className="w-full text-destructive hover:bg-destructive/10 hover:text-destructive"
								>
									<LogOut className="h-3.5 w-3.5" />
									Clear session
								</Button>
							</div>
						) : (
							<div className="mt-3 space-y-2">
								<p className="text-xs text-muted-foreground">No node attached.</p>
								<Button variant="outline" size="sm" asChild className="w-full">
									<Link to="/login" onClick={onNavigate}>
										<Github className="h-3.5 w-3.5" />
										Sign In
									</Link>
								</Button>
							</div>
						)}
					</div>

					<div className="mt-3 truncate rounded-lg border border-border/50 bg-muted/50 px-3 py-2 font-mono text-xs text-muted-foreground">
						{location.pathname}
					</div>
				</div>
			</nav>
		</div>
	);
}

export default function Header() {
	const [mobileOpen, setMobileOpen] = useState(false);

	return (
		<>
			<div className="fixed top-0 right-0 left-0 z-40 flex items-center justify-between border-b border-border bg-background/95 px-4 py-2.5 backdrop-blur xl:hidden">
				<div className="flex items-center gap-2.5">
					<button
						type="button"
						onClick={() => setMobileOpen(true)}
						className="rounded-lg p-1.5 text-foreground hover:bg-accent"
					>
						<Menu className="h-5 w-5" />
					</button>
					<Network className="h-5 w-5 text-primary" />
					<span className="text-sm font-semibold text-foreground">Prism</span>
				</div>
			</div>

			{mobileOpen ? (
				<div className="fixed inset-0 z-50 xl:hidden">
					<button
						type="button"
						aria-label="Close mobile menu backdrop"
						tabIndex={-1}
						className="absolute inset-0 bg-background/80 backdrop-blur-xs"
						onClick={() => setMobileOpen(false)}
						onKeyDown={(e) => {
							if (e.key === "Escape") setMobileOpen(false);
						}}
					/>
					<aside className="relative flex h-full w-72 max-w-[85vw] flex-col overflow-y-auto border-r border-border bg-card">
						<button
							type="button"
							onClick={() => setMobileOpen(false)}
							className="absolute top-4 right-4 z-10 rounded-lg p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
						>
							<X className="h-4 w-4" />
						</button>
						<SidebarContent onNavigate={() => setMobileOpen(false)} />
					</aside>
				</div>
			) : null}

			<aside className="hidden w-64 flex-col border-r border-border bg-card/50 xl:flex">
				<SidebarContent />
			</aside>
		</>
	);
}
