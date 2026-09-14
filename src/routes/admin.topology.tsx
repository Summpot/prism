import { createFileRoute } from "@tanstack/react-router";
import { AdminReady } from "@/components/admin/AdminReady";
import { NetworkTopology } from "@/components/topology/NetworkTopology";

export const Route = createFileRoute("/admin/topology")({
	component: AdminTopologyPage,
});

function AdminTopologyPage() {
	return <AdminReady>{(connection) => <NetworkTopology connection={connection} />}</AdminReady>;
}
