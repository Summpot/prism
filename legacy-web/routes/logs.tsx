import { createFileRoute } from "@tanstack/react-router";

import { ClientLogs } from "@/components/client/ClientLogs";

export const Route = createFileRoute("/logs")({
	component: LogsPage,
});

function LogsPage() {
	return <ClientLogs />;
}
