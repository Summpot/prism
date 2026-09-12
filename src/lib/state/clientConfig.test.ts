import { describe, expect, it } from "vitest";

import {
	EMPTY_CLIENT_CONFIG,
	applyProfileSelection,
	mergeActiveConfig,
	removeProfile,
	upsertProfile,
} from "@/lib/state/clientConfig";
import type { ClientConfigResponse, ClientProfile } from "@/types/client";

const alpha: ClientProfile = {
	id: "a",
	name: "Alpha",
	server_addr: "alpha.example:7000",
	transport: "quic",
	auth_token: "tok-a",
	listen_addr: "127.0.0.1:25565",
	fake_lan_broadcast: true,
};

const beta: ClientProfile = {
	id: "b",
	name: "Beta",
	server_addr: "beta.example:7000",
	transport: "tcp",
	auth_token: "tok-b",
	listen_addr: "127.0.0.1:25566",
	fake_lan_broadcast: false,
};

function config(overrides: Partial<ClientConfigResponse> = {}): ClientConfigResponse {
	return {
		...EMPTY_CLIENT_CONFIG,
		profiles: [alpha, beta],
		active_profile_id: "a",
		active_config: {
			...EMPTY_CLIENT_CONFIG.active_config,
			profile_name: alpha.name,
			server_addr: alpha.server_addr,
			transport: alpha.transport,
			auth_token: alpha.auth_token,
		},
		...overrides,
	};
}

describe("mergeActiveConfig", () => {
	it("patches form fields without dropping the rest of the snapshot", () => {
		const next = mergeActiveConfig(config(), { server_addr: "relay.example:1", auth_token: "n" });
		expect(next.active_config.server_addr).toBe("relay.example:1");
		expect(next.active_config.auth_token).toBe("n");
		expect(next.active_config.transport).toBe("quic");
		expect(next.profiles).toHaveLength(2);
	});
});

describe("applyProfileSelection", () => {
	it("copies the selected profile into the active form", () => {
		const next = applyProfileSelection(config(), "b");
		expect(next.active_profile_id).toBe("b");
		expect(next.active_config.server_addr).toBe(beta.server_addr);
		expect(next.active_config.auth_token).toBe(beta.auth_token);
		expect(next.active_config.listen_addr).toBe(beta.listen_addr);
	});

	it("keeps typed form fields when the id is not in the list yet", () => {
		const next = applyProfileSelection(config(), "profile-new");
		expect(next.active_profile_id).toBe("profile-new");
		expect(next.active_config.server_addr).toBe(alpha.server_addr);
	});
});

describe("upsertProfile / removeProfile", () => {
	it("inserts a new profile and selects it", () => {
		const created: ClientProfile = {
			id: "c",
			name: "Gamma",
			server_addr: "gamma.example:7000",
			transport: "auto",
			auth_token: "",
			listen_addr: "127.0.0.1:25565",
			fake_lan_broadcast: true,
		};
		const next = upsertProfile(config(), created);
		expect(next.profiles.map((p) => p.id)).toEqual(["a", "b", "c"]);
		expect(next.active_profile_id).toBe("c");
		expect(next.active_config.server_addr).toBe(created.server_addr);
	});

	it("falls back to the first remaining profile after deleting the active one", () => {
		const next = removeProfile(config(), "a");
		expect(next.profiles.map((p) => p.id)).toEqual(["b"]);
		expect(next.active_profile_id).toBe("b");
		expect(next.active_config.server_addr).toBe(beta.server_addr);
	});
});
