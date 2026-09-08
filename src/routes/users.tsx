import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/users")({
	beforeLoad: () => {
		throw redirect({ to: "/admin/users" });
	},
	component: () => null,
});
