import { Sliders, WifiOff } from "lucide-react";

import { MiddlewareConfigEditor } from "@/components/MiddlewareConfigEditor";
import { usePanelSession } from "@/lib/panelSession";
import { m } from "@/paraglide/messages";

export function ClientMiddleware() {
	const { connection, ready } = usePanelSession();

	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-y-auto">
			<div className="flex flex-none select-none items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
				<div className="flex items-center gap-2 min-w-0">
					<Sliders className="h-4 w-4 text-primary flex-none" />
					<div className="min-w-0">
						<h1 className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground">
							{m.client_middleware_title()}
						</h1>
						<p className="truncate text-[10px] text-muted-foreground hidden sm:block">
							{m.client_middleware_description()}
						</p>
					</div>
				</div>
			</div>

			{!ready ? (
				<div className="rounded-lg border border-border bg-card px-4 py-8 text-center text-xs text-muted-foreground shadow-xs">
					{m.common_restoring_session()}
				</div>
			) : !connection ? (
				<div className="flex flex-1 flex-col items-center justify-center rounded-lg border border-border bg-card p-8 text-center text-muted-foreground shadow-xs">
					<WifiOff className="mb-2 h-7 w-7 text-muted-foreground/50" />
					<p className="text-xs font-medium text-foreground">{m.client_middleware_connect()}</p>
					<p className="mt-1 max-w-sm text-[11px] text-muted-foreground">
						{m.client_middleware_connect_hint()}
					</p>
				</div>
			) : (
				<MiddlewareConfigEditor connection={connection} />
			)}
		</div>
	);
}
