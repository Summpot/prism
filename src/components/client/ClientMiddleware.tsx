import { Sliders } from "lucide-react";

import { MiddlewareConfigEditor } from "@/components/MiddlewareConfigEditor";
import { PageHeader } from "@/components/ui";
import { m } from "@/paraglide/messages";

export function ClientMiddleware() {
	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2 p-3 sm:p-4 overflow-y-auto">
			<PageHeader
				icon={<Sliders className="h-4 w-4 text-primary" />}
				title={m.client_middleware_title()}
				description={m.client_middleware_description()}
				className="pb-2.5 mb-1"
			/>

			<MiddlewareConfigEditor local />
		</div>
	);
}
