import type { AuthProvidersResponse, OptimizerStatsSnapshot, UserRecord } from "./admin";
export type { AuthProvidersResponse, UserRecord };

export interface CumulativeStats {
	raw_bytes: number;
	wire_bytes: number;
	saved_bytes: number;
	saved_ratio: number;
	sessions_count: number;
	last_session_at?: string | null;
}

export type ClientOptimizerStats = OptimizerStatsSnapshot & {
	sessions_count?: number;
};

export interface ClientRegisteredService {
	name: string;
	proto: string;
	local_addr: string;
	route_only: boolean;
	remote_addr: string;
	masquerade_host: string;
	middleware?: string | null;
}

export interface ClientStatusResponse {
	running: boolean;
	state: string; // "idle" | "connecting" | "connected" | "disconnected"
	server_addr: string;
	transport: string;
	actual_transport?: string | null;
	listen_addr: string;
	fake_lan_broadcast: boolean;
	known_services: ClientRegisteredService[];
	stats: ClientOptimizerStats;
	active_profile_id?: string | null;
	cumulative_stats?: CumulativeStats | null;
}

export interface ClientConfigState {
	profile_name: string;
	server_addr: string;
	transport: string;
	auth_token: string;
	listen_addr: string;
	fake_lan_broadcast: boolean;
	auto_connect_panel: boolean;
	auto_connect?: boolean;
	management_url?: string;
	token_id?: string;
	token_type?: string;
	user_id?: string;
	username?: string;
	expires_at?: number | null;
}

export interface ClientConfigResponse {
	active_profile_id: string | null;
	active_config: ClientConfigState;
	profiles: ClientProfile[];
	cumulative_stats: CumulativeStats;
	device_id?: string;
}

export interface StartClientPayload {
	server_addr: string;
	transport?: string;
	auth_token?: string;
	listen_addr?: string;
	fake_lan_broadcast?: boolean;
	motd_prefix?: string;
	profile_id?: string;
	profile_name?: string;
}

export interface ClientProfile {
	id: string;
	name: string;
	server_addr: string;
	transport: string;
	auth_token: string;
	listen_addr: string;
	fake_lan_broadcast: boolean;
}

export interface ClientLogEntry {
	timestamp: string;
	level: string;
	target: string;
	message: string;
}

export interface ClientContextValue {
	// Status & Metrics
	status: ClientStatusResponse | null;
	cumulativeStats: CumulativeStats | null;
	statsViewMode: "session" | "lifetime";
	setStatsViewMode: (mode: "session" | "lifetime") => void;
	throughputSamples: number[];
	uptimeSeconds: number;
	formatUptime: (seconds: number) => string;
	isRunning: boolean;
	isConnected: boolean;
	isConnecting: boolean;
	rawBytes: number;
	wireBytes: number;
	savedRatio: number;

	// Actions & State
	actionLoading: boolean;
	error: string | null;
	setError: (err: string | null) => void;
	copied: string | null;
	copyText: (text: string, id: string) => void;
	handleConnect: () => Promise<void>;
	handleDisconnect: () => Promise<void>;
	handleToggleTunnel: () => void;
	handleResetStats: () => Promise<void>;
	handleShareLink: () => void;

	// Profiles & Active Form
	profiles: ClientProfile[];
	selectedProfileId: string;
	profileName: string;
	setProfileName: (val: string) => void;
	serverAddr: string;
	setServerAddr: (val: string) => void;
	transport: string;
	setTransport: (val: string) => void;
	authToken: string;
	setAuthToken: (val: string) => void;
	listenAddr: string;
	setListenAddr: (val: string) => void;
	fakeLanBroadcast: boolean;
	setFakeLanBroadcast: (val: boolean) => void;
	autoConnectPanel: boolean;
	setAutoConnectPanel: (val: boolean) => void;
	autoConnect: boolean;
	setAutoConnect: (val: boolean) => void;
	managementUrl: string;
	handleSelectProfile: (id: string) => void;
	handleSaveProfile: () => Promise<void>;
	handleDeleteProfile: (id: string) => Promise<void>;

	// Remote Link Input
	remoteLinkInput: string;
	setRemoteLinkInput: (val: string) => void;
	linkProtocol: string;
	setLinkProtocol: (proto: string) => void;
	handleSelectProtocol: (proto: string) => void;
	handleAddressChange: (val: string) => void;
	handleAddressPaste: (e: React.ClipboardEvent<HTMLInputElement>) => void;
	handleAddressCopy: (e: React.ClipboardEvent<HTMLInputElement>) => void;
	handleConnectFromLink: (customLink?: string) => Promise<void>;

	// Logs
	logs: ClientLogEntry[];
	filteredLogs: ClientLogEntry[];
	logFilterLevel: string;
	setLogFilterLevel: (level: string) => void;
	logSearchQuery: string;
	setLogSearchQuery: (q: string) => void;
	autoScrollLogs: boolean;
	setAutoScrollLogs: (enabled: boolean) => void;
	isAtBottom: boolean;
	logsContainerRef: React.RefObject<HTMLDivElement | null>;
	handleLogsScroll: () => void;
	scrollToBottom: (smooth?: boolean) => void;
	handleClearLogs: () => Promise<void>;
	handleCopyAllLogs: () => void;

	// Modals & OAuth
	loginModalOpen: boolean;
	setLoginModalOpen: (open: boolean) => void;
	checkingProviders: boolean;
	providersResult: AuthProvidersResponse | null;
	setProvidersResult: (result: AuthProvidersResponse | null) => void;
	providersError: string | null;
	setProvidersError: (err: string | null) => void;
	authServerUrl: string;
	setAuthServerUrl: (url: string) => void;
	authError: string | null;
	setAuthError: (err: string | null) => void;
	oauthLoading: boolean;
	oauthWaitingCallback: boolean;
	setOauthWaitingCallback: (waiting: boolean) => void;
	oauthExchanging: boolean;
	manualCallbackInput: string;
	setManualCallbackInput: (val: string) => void;
	handleManualOAuthCallback: (input: string) => Promise<void>;
	startGitHubAuthWithUrl: (targetAuthUrl: string, targetServerAddr?: string) => Promise<void>;
	handleRedetectProviders: (overrideUrl?: string) => Promise<void>;
	loginAdminUnlocked: boolean;

	// Import modal
	importModalOpen: boolean;
	setImportModalOpen: (open: boolean) => void;
	importUrl: string;
	setImportUrl: (url: string) => void;
	importError: string | null;
	setImportError: (err: string | null) => void;
	handleImportLink: () => void;
}
