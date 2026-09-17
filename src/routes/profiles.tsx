import { createFileRoute } from "@tanstack/react-router";

import { ClientSettings } from "@/components/client/ClientSettings";

export const Route = createFileRoute("/profiles")({
	component: ProfilesPage,
});

function ProfilesPage() {
	return <ClientSettings />;
}
