import { describe, expect, it } from "vitest";

import {
	clearPanelConnection,
	deriveManagementUrl,
	loadPanelConnection,
	normalizeBaseUrl,
	persistPanelConnection,
	type StorageLike,
} from "@/lib/panelConnection";

function createStorage(): StorageLike {
	const map = new Map<string, string>();
	return {
		getItem: (key) => map.get(key) ?? null,
		setItem: (key, value) => {
			map.set(key, value);
		},
		removeItem: (key) => {
			map.delete(key);
		},
	};
}

describe("panelConnection", () => {
	it("normalizes trailing slashes from base URLs", () => {
		expect(normalizeBaseUrl(" http://127.0.0.1:8080/// ")).toBe("http://127.0.0.1:8080");
	});

	it("persists and restores a valid connection", () => {
		const storage = createStorage();
		persistPanelConnection(storage, {
			baseUrl: "http://127.0.0.1:8080/",
			token: " panel-secret ",
		});

		expect(loadPanelConnection(storage)).toEqual({
			baseUrl: "http://127.0.0.1:8080",
			token: "panel-secret",
		});
	});

	it("clears malformed values from storage", () => {
		const storage = createStorage();
		storage.setItem("prism.panel.connection", "{bad json");
		expect(loadPanelConnection(storage)).toBeNull();
	});

	it("removes the persisted connection", () => {
		const storage = createStorage();
		persistPanelConnection(storage, {
			baseUrl: "http://127.0.0.1:8080",
			token: "panel-secret",
		});
		clearPanelConnection(storage);
		expect(loadPanelConnection(storage)).toBeNull();
	});

	it("derives management url correctly from tunnel server address", () => {
		expect(deriveManagementUrl("")).toBe("");
		expect(deriveManagementUrl("127.0.0.1:7000")).toBe("http://127.0.0.1:8080");
		expect(deriveManagementUrl("mc.prism.gg:7000")).toBe("http://mc.prism.gg:8080");
		expect(deriveManagementUrl("quic://play.example.com:7000")).toBe(
			"http://play.example.com:8080",
		);
		expect(deriveManagementUrl("[::1]:7000")).toBe("http://[::1]:8080");
		expect(deriveManagementUrl("1.2.3.4")).toBe("http://1.2.3.4:8080");
	});
});
