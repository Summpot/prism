import { createFileRoute, Outlet } from "@tanstack/react-router";

export const Route = createFileRoute("/nodes")({ component: NodesLayout });

function NodesLayout() {
	return <Outlet />;
}
