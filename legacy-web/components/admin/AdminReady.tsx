import type { ReactNode } from "react";

import { StateCard } from "@/components/ui";
import { usePanelSession } from "@/lib/panelSession";
import type { PanelConnection } from "@/lib/panelConnection";
import { m } from "@/paraglide/messages";

export function AdminReady({
	children,
	connectLabel,
}: {
	children: (connection: PanelConnection) => ReactNode;
	connectLabel?: string;
}) {
	const { connection, ready } = usePanelSession();

	if (!ready) {
		return <StateCard label={m.common_restoring_session()} />;
	}

	if (!connection) {
		return <StateCard label={connectLabel ?? m.common_connect_panel()} />;
	}

	return <>{children(connection)}</>;
}
