import { Link, useLocation } from "@tanstack/react-router";
import {
	Activity,
	Box,
	Cable,
	LogOut,
	Menu,
	Network,
	PlugZap,
	ShieldCheck,
	Unplug,
	X,
} from "lucide-react";
import { useState } from "react";

import { usePanelSession } from "@/lib/panelSession";

const navItems = [
	{ to: "/", label: "Overview", icon: <Activity className="h-4 w-4" /> },
	{ to: "/nodes", label: "Nodes", icon: <Box className="h-4 w-4" /> },
	{
		to: "/connections",
		label: "Connections",
		icon: <Cable className="h-4 w-4" />,
	},
	{
		to: "/tunnel-services",
		label: "Tunnel Services",
		icon: <Unplug className="h-4 w-4" />,
	},
	{
		to: "/login",
		label: "Connection",
		icon: <PlugZap className="h-4 w-4" />,
	},
];

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
			className="flex items-center gap-3 rounded-2xl border border-white/8 bg-white/3 px-4 py-3 text-sm font-medium text-slate-300 transition hover:border-cyan-400/30 hover:bg-cyan-400/8 hover:text-white"
			activeProps={{
				className:
					"flex items-center gap-3 rounded-2xl border border-cyan-400/40 bg-cyan-400/12 px-4 py-3 text-sm font-medium text-white shadow-[0_0_0_1px_rgba(34,211,238,0.18)]",
			}}
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
		<>
			<div className="border-b border-white/8 px-6 py-6">
				<div className="flex items-center gap-4">
					<div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-cyan-400/30 bg-cyan-400/10 text-cyan-300 shadow-[0_0_30px_rgba(34,211,238,0.15)]">
						<Network className="h-6 w-6" />
					</div>
					<div>
						<p className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">Prism</p>
						<h1 className="text-2xl font-semibold text-white">Control Plane</h1>
					</div>
				</div>
				<p className="mt-4 text-sm leading-6 text-slate-400">
					Operate standalone management nodes, inspect worker state, and edit structured Prism
					configs without dropping into raw files.
				</p>
			</div>

			<nav className="flex flex-1 flex-col gap-3 px-6 py-6">
				{navItems.map((item) => (
					<NavLink
						key={item.to}
						to={item.to}
						label={item.label}
						icon={item.icon}
						onClick={onNavigate}
					/>
				))}

				<div className="mt-6 rounded-3xl border border-white/8 bg-white/4 p-4">
					<div className="flex items-center gap-2 text-xs uppercase tracking-[0.24em] text-slate-500">
						<ShieldCheck className="h-4 w-4 text-emerald-300" />
						Session
					</div>
					{connection ? (
						<>
							<div className="mt-4 break-all text-sm font-medium text-white">
								{connection.baseUrl}
							</div>
							<div className="mt-2 text-sm text-slate-400">
								Bearer token loaded in browser storage.
							</div>
							<button
								type="button"
								onClick={clearConnection}
								className="mt-4 inline-flex items-center gap-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-3 py-2 text-sm font-medium text-red-200 transition hover:border-red-400/40 hover:bg-red-400/16"
							>
								<LogOut className="h-4 w-4" />
								Clear session
							</button>
						</>
					) : (
						<div className="mt-4 text-sm text-slate-400">
							No management endpoint configured yet.
						</div>
					)}
				</div>

				<div className="mt-auto rounded-3xl border border-white/8 bg-gradient-to-br from-slate-900 to-slate-950 p-4 text-sm text-slate-400">
					<div className="font-medium text-white">Current route</div>
					<div className="mt-2 break-all text-cyan-200/85">{location.pathname}</div>
				</div>
			</nav>
		</>
	);
}

export default function Header() {
	const [mobileOpen, setMobileOpen] = useState(false);

	return (
		<>
			{/* Mobile top bar */}
			<div className="fixed top-0 right-0 left-0 z-40 flex items-center gap-3 border-b border-white/8 bg-slate-950/95 px-4 py-3 backdrop-blur xl:hidden">
				<button
					type="button"
					onClick={() => setMobileOpen(true)}
					className="rounded-xl p-1.5 text-white transition hover:bg-white/10"
				>
					<Menu className="h-6 w-6" />
				</button>
				<Network className="h-5 w-5 text-cyan-300" />
				<span className="text-sm font-semibold text-white">Prism Control Plane</span>
			</div>

			{/* Mobile drawer overlay */}
			{mobileOpen ? (
				<div className="fixed inset-0 z-50 xl:hidden">
					<button
						type="button"
						tabIndex={-1}
						className="absolute inset-0 bg-black/60 backdrop-blur-sm"
						onClick={() => setMobileOpen(false)}
						onKeyDown={(e) => {
							if (e.key === "Escape") setMobileOpen(false);
						}}
					/>
					<aside className="relative flex h-full w-80 max-w-[85vw] flex-col overflow-y-auto border-r border-white/8 bg-slate-950">
						<button
							type="button"
							onClick={() => setMobileOpen(false)}
							className="absolute top-4 right-4 z-10 rounded-xl p-1.5 text-slate-400 transition hover:bg-white/10 hover:text-white"
						>
							<X className="h-5 w-5" />
						</button>
						<SidebarContent onNavigate={() => setMobileOpen(false)} />
					</aside>
				</div>
			) : null}

			{/* Desktop sidebar */}
			<aside className="hidden border-r border-white/8 bg-slate-950/80 xl:flex xl:w-80 xl:flex-col xl:backdrop-blur">
				<SidebarContent />
			</aside>
		</>
	);
}
