export interface PanelConnection {
	baseUrl: string;
	token: string;
}

export interface StorageLike {
	getItem(key: string): string | null;
	setItem(key: string, value: string): void;
	removeItem(key: string): void;
}

export const PANEL_CONNECTION_STORAGE_KEY = "prism.panel.connection";

export function normalizeBaseUrl(value: string) {
	return value.trim().replace(/\/+$/, "");
}

export function deriveManagementUrl(serverAddr: string): string {
	const trimmed = serverAddr.trim();
	if (!trimmed) {
		return "";
	}
	let host = trimmed;
	if (host.includes("://")) {
		host = host.split("://")[1] || "";
	}
	if (host.startsWith("[")) {
		const end = host.indexOf("]");
		if (end !== -1) {
			host = host.slice(0, end + 1);
		}
	} else if (host.includes(":")) {
		host = host.split(":")[0] || "";
	}
	if (!host) {
		return "";
	}
	return `http://${host}:8080`;
}

export function normalizePanelConnection(value: PanelConnection): PanelConnection {
	return {
		baseUrl: normalizeBaseUrl(value.baseUrl),
		token: value.token.trim(),
	};
}

export function isValidPanelConnection(value: PanelConnection | null): value is PanelConnection {
	return Boolean(value?.baseUrl && value.token);
}

export function loadPanelConnection(storage: StorageLike): PanelConnection | null {
	const raw = storage.getItem(PANEL_CONNECTION_STORAGE_KEY);
	if (!raw) {
		return null;
	}

	try {
		const parsed = JSON.parse(raw) as Partial<PanelConnection>;
		const normalized = normalizePanelConnection({
			baseUrl: parsed.baseUrl ?? "",
			token: parsed.token ?? "",
		});

		if (!isValidPanelConnection(normalized)) {
			storage.removeItem(PANEL_CONNECTION_STORAGE_KEY);
			return null;
		}

		return normalized;
	} catch {
		storage.removeItem(PANEL_CONNECTION_STORAGE_KEY);
		return null;
	}
}

export function persistPanelConnection(storage: StorageLike, value: PanelConnection) {
	const normalized = normalizePanelConnection(value);
	storage.setItem(PANEL_CONNECTION_STORAGE_KEY, JSON.stringify(normalized));
	return normalized;
}

export function clearPanelConnection(storage: StorageLike) {
	storage.removeItem(PANEL_CONNECTION_STORAGE_KEY);
}
