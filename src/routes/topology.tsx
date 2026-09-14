import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/topology")({
	beforeLoad: () => {
		throw redirect({ to: "/admin/topology" });
	},
	component: () => null,
});
