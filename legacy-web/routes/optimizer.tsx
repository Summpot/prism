import { createFileRoute } from "@tanstack/react-router";

import { ClientOptimizer } from "@/components/client/ClientOptimizer";

export const Route = createFileRoute("/optimizer")({
	component: OptimizerPage,
});

function OptimizerPage() {
	return <ClientOptimizer />;
}
