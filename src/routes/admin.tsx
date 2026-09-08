import { createFileRoute, Outlet, useNavigate } from "@tanstack/react-router";
import { RotateCcw, ShieldAlert } from "lucide-react";

import { Button } from "@/components/ui/button";
import { usePanelSession } from "@/lib/panelSession";

export const Route = createFileRoute("/admin")({
	component: AdminLayout,
});

function AdminLayout() {
	const { isAdmin, ready } = usePanelSession();
	const navigate = useNavigate();

	if (!ready) {
		return (
			<div className="flex flex-1 h-full w-full items-center justify-center bg-slate-950 text-slate-400">
				<RotateCcw className="h-6 w-6 animate-spin text-primary" />
			</div>
		);
	}

	if (!isAdmin) {
		return (
			<div className="flex-1 min-h-0 overflow-y-auto bg-slate-950 text-slate-100 p-4 sm:p-6 lg:p-8 flex items-center justify-center">
				<div className="max-w-md w-full rounded-2xl border border-white/10 bg-slate-900/90 p-8 text-center shadow-2xl space-y-4">
					<div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-destructive/15 text-destructive border border-destructive/30">
						<ShieldAlert className="h-7 w-7" />
					</div>
					<div className="space-y-1.5">
						<h2 className="text-xl font-bold text-white tracking-tight">
							管理控制台仅对管理员开放
						</h2>
						<p className="text-xs text-slate-400 leading-relaxed">
							当前未连接管理节点或当前登录账号非管理员。请先登录管理员账号或返回桌面端使用代理连接功能。
						</p>
					</div>
					<div className="flex items-center justify-center gap-3 pt-3">
						<Button
							variant="outline"
							size="sm"
							onClick={() => void navigate({ to: "/" })}
							className="text-xs border-white/10 text-slate-200 hover:bg-white/10"
						>
							返回客户端
						</Button>
						<Button
							size="sm"
							onClick={() => void navigate({ to: "/login" })}
							className="text-xs bg-primary text-primary-foreground hover:bg-primary/90"
						>
							前往管理登录
						</Button>
					</div>
				</div>
			</div>
		);
	}

	return (
		<div className="flex-1 min-h-0 overflow-y-auto bg-slate-950 text-slate-100 p-4 sm:p-6 lg:p-8">
			<div className="mx-auto max-w-7xl">
				<Outlet />
			</div>
		</div>
	);
}
