import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/client")({
	beforeLoad: ({ search }) => {
		const tab = (search as Record<string, unknown>)?.tab;
		if (tab === "logs") {
			throw redirect({ to: "/logs" });
		}
		if (tab === "settings" || tab === "profiles") {
			throw redirect({ to: "/settings" });
		}
		throw redirect({ to: "/" });
	},
	component: () => null,
});
