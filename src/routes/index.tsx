import { createFileRoute } from "@tanstack/react-router";

import { ClientOverview } from "@/components/client/ClientOverview";

export const Route = createFileRoute("/")({
	component: HomePage,
});

function HomePage() {
	return <ClientOverview />;
}
