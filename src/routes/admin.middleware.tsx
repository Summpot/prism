import { createFileRoute } from "@tanstack/react-router";

import { AdminReady } from "@/components/admin/AdminReady";
import { MiddlewareConfigEditor } from "@/components/MiddlewareConfigEditor";
import { PageHeader } from "@/components/ui";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/middleware")({
	component: AdminMiddlewarePage,
});

function AdminMiddlewarePage() {
	return (
		<AdminReady connectLabel={m.runtime_connect_panel()}>
			{(connection) => (
				<div className="space-y-5">
					<PageHeader
						eyebrow={m.admin_middleware_eyebrow()}
						title={m.admin_middleware_title()}
						description={m.admin_middleware_description()}
					/>
					<MiddlewareConfigEditor connection={connection} />
				</div>
			)}
		</AdminReady>
	);
}
