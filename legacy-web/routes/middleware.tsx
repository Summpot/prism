import { createFileRoute } from "@tanstack/react-router";

import { ClientMiddleware } from "@/components/client/ClientMiddleware";

export const Route = createFileRoute("/middleware")({
	component: MiddlewarePage,
});

function MiddlewarePage() {
	return <ClientMiddleware />;
}
