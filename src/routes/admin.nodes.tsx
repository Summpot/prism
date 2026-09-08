import { createFileRoute, Outlet } from "@tanstack/react-router";

export const Route = createFileRoute("/admin/nodes")({ component: AdminNodesLayout });

function AdminNodesLayout() {
	return <Outlet />;
}
