import { createContext, useContext, useEffect, useMemo, useState } from "react";

import {
	clearPanelConnection,
	loadPanelConnection,
	type PanelConnection,
	persistPanelConnection,
} from "@/lib/panelConnection";

interface PanelSessionContextValue {
	connection: PanelConnection | null;
	ready: boolean;
	saveConnection: (value: PanelConnection) => void;
	clearConnection: () => void;
}

const PanelSessionContext = createContext<PanelSessionContextValue | null>(null);

export function PanelSessionProvider({ children }: { children: React.ReactNode }) {
	const [connection, setConnection] = useState<PanelConnection | null>(null);
	const [ready, setReady] = useState(false);

	useEffect(() => {
		if (typeof window === "undefined") {
			setReady(true);
			return;
		}

		setConnection(loadPanelConnection(window.localStorage));
		setReady(true);
	}, []);

	const value = useMemo<PanelSessionContextValue>(
		() => ({
			connection,
			ready,
			saveConnection: (next) => {
				if (typeof window !== "undefined") {
					setConnection(persistPanelConnection(window.localStorage, next));
				}
			},
			clearConnection: () => {
				if (typeof window !== "undefined") {
					clearPanelConnection(window.localStorage);
				}
				setConnection(null);
			},
		}),
		[connection, ready],
	);

	return <PanelSessionContext.Provider value={value}>{children}</PanelSessionContext.Provider>;
}

export function usePanelSession() {
	const value = useContext(PanelSessionContext);
	if (!value) {
		throw new Error("usePanelSession must be used within PanelSessionProvider");
	}
	return value;
}
