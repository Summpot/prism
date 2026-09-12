import { create } from "zustand";

import type { AuthProvidersResponse } from "@/types/admin";
import type { ClientStatusResponse } from "@/types/client";

const EMPTY_THROUGHPUT = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

type StatsViewMode = "session" | "lifetime";

type ClientUiState = {
	actionLoading: boolean;
	error: string | null;
	statsViewMode: StatsViewMode;
	throughputSamples: number[];
	prevWireBytes: number;
	connectedAt: number | null;
	copied: string | null;
	copiedTimer: ReturnType<typeof setTimeout> | null;

	logFilterLevel: string;
	logSearchQuery: string;
	autoScrollLogs: boolean;

	remoteLinkInput: string;
	linkProtocol: string;

	importModalOpen: boolean;
	importUrl: string;
	importError: string | null;

	loginModalOpen: boolean;
	checkingProviders: boolean;
	providersResult: AuthProvidersResponse | null;
	providersError: string | null;
	authServerUrl: string;
	authError: string | null;
	oauthLoading: boolean;
	oauthWaitingCallback: boolean;
	oauthExchanging: boolean;
	manualCallbackInput: string;
	loginAdminUnlocked: boolean;
};

type ClientUiActions = {
	setActionLoading: (value: boolean) => void;
	setError: (value: string | null) => void;
	setStatsViewMode: (mode: StatsViewMode) => void;
	ingestStatus: (status: ClientStatusResponse | null | undefined) => void;
	copyText: (text: string, id: string) => void;
	setCopied: (id: string | null) => void;
	setLogFilterLevel: (level: string) => void;
	setLogSearchQuery: (query: string) => void;
	setAutoScrollLogs: (enabled: boolean) => void;
	setRemoteLinkInput: (value: string | ((prev: string) => string)) => void;
	setLinkProtocol: (value: string) => void;
	setImportModalOpen: (open: boolean) => void;
	setImportUrl: (value: string) => void;
	setImportError: (value: string | null) => void;
	setLoginModalOpen: (open: boolean) => void;
	setCheckingProviders: (value: boolean) => void;
	setProvidersResult: (value: AuthProvidersResponse | null) => void;
	setProvidersError: (value: string | null) => void;
	setAuthServerUrl: (value: string) => void;
	setAuthError: (value: string | null) => void;
	setOauthLoading: (value: boolean) => void;
	setOauthWaitingCallback: (value: boolean) => void;
	setOauthExchanging: (value: boolean) => void;
	setManualCallbackInput: (value: string) => void;
	setLoginAdminUnlocked: (value: boolean) => void;
	resetAuthFlow: () => void;
	clearProviders: () => void;
};

export const useClientUiStore = create<ClientUiState & ClientUiActions>((set, get) => ({
	actionLoading: false,
	error: null,
	statsViewMode: "session",
	throughputSamples: EMPTY_THROUGHPUT,
	prevWireBytes: 0,
	connectedAt: null,
	copied: null,
	copiedTimer: null,

	logFilterLevel: "ALL",
	logSearchQuery: "",
	autoScrollLogs: true,

	remoteLinkInput: "",
	linkProtocol: "auto://",

	importModalOpen: false,
	importUrl: "",
	importError: null,

	loginModalOpen: false,
	checkingProviders: false,
	providersResult: null,
	providersError: null,
	authServerUrl: "http://127.0.0.1:8080",
	authError: null,
	oauthLoading: false,
	oauthWaitingCallback: false,
	oauthExchanging: false,
	manualCallbackInput: "",
	loginAdminUnlocked: false,

	setActionLoading: (value) => set({ actionLoading: value }),
	setError: (value) => set({ error: value }),
	setStatsViewMode: (mode) => set({ statsViewMode: mode }),
	ingestStatus: (status) => {
		if (!status?.running) {
			set({
				throughputSamples: EMPTY_THROUGHPUT,
				prevWireBytes: 0,
				connectedAt: null,
			});
			return;
		}
		const currentWire = status.stats.wire_bytes;
		const prev = get().prevWireBytes;
		const delta = prev > 0 ? Math.max(0, currentWire - prev) : 0;
		const connectedAt = status.state === "connected" ? (get().connectedAt ?? Date.now()) : null;
		set((state) => ({
			prevWireBytes: currentWire,
			throughputSamples: [...state.throughputSamples.slice(1), delta],
			connectedAt,
		}));
	},
	copyText: (text, id) => {
		void navigator.clipboard.writeText(text);
		const existing = get().copiedTimer;
		if (existing) {
			clearTimeout(existing);
		}
		const timer = setTimeout(() => {
			set({ copied: null, copiedTimer: null });
		}, 2000);
		set({ copied: id, copiedTimer: timer });
	},
	setCopied: (id) => {
		const existing = get().copiedTimer;
		if (existing) {
			clearTimeout(existing);
		}
		if (!id) {
			set({ copied: null, copiedTimer: null });
			return;
		}
		const timer = setTimeout(() => {
			set({ copied: null, copiedTimer: null });
		}, 2000);
		set({ copied: id, copiedTimer: timer });
	},
	setLogFilterLevel: (level) => set({ logFilterLevel: level }),
	setLogSearchQuery: (query) => set({ logSearchQuery: query }),
	setAutoScrollLogs: (enabled) => set({ autoScrollLogs: enabled }),
	setRemoteLinkInput: (value) =>
		set((state) => ({
			remoteLinkInput: typeof value === "function" ? value(state.remoteLinkInput) : value,
		})),
	setLinkProtocol: (value) => set({ linkProtocol: value }),
	setImportModalOpen: (open) => set({ importModalOpen: open }),
	setImportUrl: (value) => set({ importUrl: value }),
	setImportError: (value) => set({ importError: value }),
	setLoginModalOpen: (open) => set({ loginModalOpen: open }),
	setCheckingProviders: (value) => set({ checkingProviders: value }),
	setProvidersResult: (value) => set({ providersResult: value }),
	setProvidersError: (value) => set({ providersError: value }),
	setAuthServerUrl: (value) => set({ authServerUrl: value }),
	setAuthError: (value) => set({ authError: value }),
	setOauthLoading: (value) => set({ oauthLoading: value }),
	setOauthWaitingCallback: (value) => set({ oauthWaitingCallback: value }),
	setOauthExchanging: (value) => set({ oauthExchanging: value }),
	setManualCallbackInput: (value) => set({ manualCallbackInput: value }),
	setLoginAdminUnlocked: (value) => set({ loginAdminUnlocked: value }),
	resetAuthFlow: () =>
		set({
			loginModalOpen: false,
			providersResult: null,
			providersError: null,
			authError: null,
			oauthWaitingCallback: false,
			oauthExchanging: false,
			oauthLoading: false,
			checkingProviders: false,
			manualCallbackInput: "",
		}),
	clearProviders: () =>
		set({
			providersResult: null,
			providersError: null,
			authError: null,
		}),
}));
