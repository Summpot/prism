import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/runtime")({
	beforeLoad: () => {
		throw redirect({ to: "/admin/runtime" });
	},
	component: () => null,
});
