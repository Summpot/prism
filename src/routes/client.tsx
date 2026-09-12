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
		if (tab === "traffic") {
			throw redirect({ to: "/traffic" });
		}
		if (tab === "middleware") {
			throw redirect({ to: "/middleware" });
		}
		throw redirect({ to: "/" });
	},
	component: () => null,
});
