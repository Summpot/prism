import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/nodes")({
	beforeLoad: () => {
		throw redirect({ to: "/admin/nodes" });
	},
	component: () => null,
});
