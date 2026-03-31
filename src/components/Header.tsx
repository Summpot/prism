import { Link, useLocation } from "@tanstack/react-router";
import {
	Activity,
	Box,
	LogOut,
	Network,
	PlugZap,
	ShieldCheck,
} from "lucide-react";

import { usePanelSession } from "@/lib/panelSession";

function NavLink({
	to,
	label,
	icon,
}: {
	to: string;
	label: string;
	icon: React.ReactNode;
}) {
	return (
		<Link
			to={to}
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

export default function Header() {
	const location = useLocation();
	const { connection, clearConnection } = usePanelSession();

	return (
		<aside className="hidden border-r border-white/8 bg-slate-950/80 xl:flex xl:w-80 xl:flex-col xl:backdrop-blur">
			<div className="border-b border-white/8 px-6 py-6">
				<div className="flex items-center gap-4">
					<div className="flex h-12 w-12 items-center justify-center rounded-2xl border border-cyan-400/30 bg-cyan-400/10 text-cyan-300 shadow-[0_0_30px_rgba(34,211,238,0.15)]">
						<Network className="h-6 w-6" />
					</div>
					<div>
						<p className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">
							Prism
						</p>
						<h1 className="text-2xl font-semibold text-white">Control Plane</h1>
					</div>
				</div>
				<p className="mt-4 text-sm leading-6 text-slate-400">
					Operate standalone management nodes, inspect worker state, and edit
					structured Prism configs without dropping into raw files.
				</p>
			</div>

			<nav className="flex flex-1 flex-col gap-3 px-6 py-6">
				<NavLink
					to="/"
					label="Overview"
					icon={<Activity className="h-4 w-4" />}
				/>
				<NavLink to="/nodes" label="Nodes" icon={<Box className="h-4 w-4" />} />
				<NavLink
					to="/login"
					label="Connection"
					icon={<PlugZap className="h-4 w-4" />}
				/>

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
					<div className="mt-2 break-all text-cyan-200/85">
						{location.pathname}
					</div>
				</div>
			</nav>
		</aside>
	);
}
