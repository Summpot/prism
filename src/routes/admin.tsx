import { createFileRoute, Outlet, useNavigate } from "@tanstack/react-router";
import { ShieldAlert } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { resolveAdminConsoleAccess } from "@/lib/admin/adminAccess";
import { usePanelSession } from "@/lib/panelSession";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin")({
	component: AdminLayout,
});

function AdminLayout() {
	const { isAdmin, ready, isLoadingSession, authSession, connection } = usePanelSession();
	const navigate = useNavigate();
	const access = resolveAdminConsoleAccess({
		ready,
		isLoadingSession,
		isAdmin,
		authSession,
		connection,
	});

	if (access === "loading") {
		return (
			<div className="flex h-full w-full flex-1 items-center justify-center">
				<Spinner className="size-6 text-muted-foreground" />
			</div>
		);
	}

	if (access === "deny") {
		return (
			<div className="flex flex-1 min-h-0 items-center justify-center overflow-y-auto p-4 sm:p-6 lg:p-8">
				<Card className="w-full max-w-md text-center shadow-xs">
					<CardHeader className="items-center">
						<div className="mx-auto flex size-14 items-center justify-center rounded-xl bg-destructive/15 text-destructive">
							<ShieldAlert className="size-7" />
						</div>
						<CardTitle className="text-xl">{m.admin_access_denied_title()}</CardTitle>
						<CardDescription>{m.admin_access_denied_description()}</CardDescription>
					</CardHeader>
					<CardFooter className="justify-center gap-3">
						<Button variant="outline" size="sm" onClick={() => void navigate({ to: "/" })}>
							{m.admin_back_client()}
						</Button>
						<Button size="sm" onClick={() => void navigate({ to: "/login" })}>
							{m.admin_go_login()}
						</Button>
					</CardFooter>
				</Card>
			</div>
		);
	}

	return (
		<div className="flex-1 min-h-0 overflow-y-auto p-4 sm:p-6 lg:p-8">
			<div className="mx-auto max-w-7xl">
				<Outlet />
			</div>
		</div>
	);
}
