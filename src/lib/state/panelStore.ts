import { create } from "zustand";

import {
	clearPanelConnection,
	loadPanelConnection,
	persistPanelConnection,
	type PanelConnection,
} from "@/lib/panelConnection";
import { getQueryClient } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";

type PanelStore = {
	connection: PanelConnection | null;
	ready: boolean;
	hydrate: () => void;
	saveConnection: (value: PanelConnection) => void;
	clearConnection: () => void;
};

export const usePanelStore = create<PanelStore>((set, get) => ({
	connection: null,
	ready: typeof window === "undefined",
	hydrate: () => {
		if (typeof window === "undefined") {
			set({ ready: true });
			return;
		}
		set({
			connection: loadPanelConnection(window.localStorage),
			ready: true,
		});
	},
	saveConnection: (value) => {
		if (typeof window === "undefined") {
			set({ connection: value });
			return;
		}
		const saved = persistPanelConnection(window.localStorage, value);
		set({ connection: saved });
	},
	clearConnection: () => {
		const previous = get().connection;
		if (typeof window !== "undefined") {
			clearPanelConnection(window.localStorage);
		}
		getQueryClient().setQueryData(queryKeys.admin.session(previous), null);
		set({ connection: null });
	},
}));
