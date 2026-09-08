import { createFileRoute } from "@tanstack/react-router";

import { ClientSettings } from "@/components/client/ClientSettings";

export const Route = createFileRoute("/settings")({
	component: SettingsPage,
});

function SettingsPage() {
	return <ClientSettings />;
}
