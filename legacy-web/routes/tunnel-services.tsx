import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/tunnel-services")({
	beforeLoad: () => {
		throw redirect({ to: "/admin/tunnel-services" });
	},
	component: () => null,
});
