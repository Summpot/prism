import { createFileRoute } from "@tanstack/react-router";

import { ClientTraffic } from "@/components/client/ClientTraffic";

export const Route = createFileRoute("/traffic")({
	component: TrafficPage,
});

function TrafficPage() {
	return <ClientTraffic />;
}
