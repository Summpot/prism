import { describe, expect, it } from "vitest";
import { parseDeepLink } from "./deepLink";

describe("parseDeepLink", () => {
	it("parses standard auth callback with query parameters", () => {
		const result = parseDeepLink(
			"prism://auth/callback?token=prism_adm_abc123&user_id=gh_1001&username=octocat&role=Admin",
		);

		expect(result).toEqual({
			kind: "auth",
			token: "prism_adm_abc123",
			userId: "gh_1001",
			username: "octocat",
			role: "Admin",
		});
	});

	it("parses auth callback with hash parameters", () => {
		const result = parseDeepLink(
			"prism://login#token=prism_cl_test456&user_id=gh_2002&username=devuser&role=Member",
		);

		expect(result).toEqual({
			kind: "auth",
			token: "prism_cl_test456",
			userId: "gh_2002",
			username: "devuser",
			role: "Member",
		});
	});

	it("parses auth callback with mixed query and hash", () => {
		const result = parseDeepLink("prism://auth/callback#token=prism_adm_xyz789");

		expect(result).toEqual({
			kind: "auth",
			token: "prism_adm_xyz789",
			userId: undefined,
			username: undefined,
			role: undefined,
		});
	});

	it("parses prism profile connection link", () => {
		const result = parseDeepLink(
			"prism://play.myserver.net:7000?name=Survival&transport=kcp&token=my_secret_token&listen=127.0.0.1:25565",
		);

		expect(result.kind).toBe("profile");
		if (result.kind === "profile") {
			expect(result.profile.server_addr).toBe("play.myserver.net:7000");
			expect(result.profile.name).toBe("Survival");
			expect(result.profile.transport).toBe("kcp");
			expect(result.profile.auth_token).toBe("my_secret_token");
			expect(result.profile.listen_addr).toBe("127.0.0.1:25565");
		}
	});

	it("parses direct GitHub custom scheme callback with authorization code", () => {
		const result = parseDeepLink("prism://auth/callback?code=gho_1234567890&state=deeplink");

		expect(result).toEqual({
			kind: "auth-code",
			code: "gho_1234567890",
			state: "deeplink",
		});
	});

	it("returns unknown for non-prism links", () => {
		expect(parseDeepLink("https://example.com")).toEqual({
			kind: "unknown",
			raw: "https://example.com",
		});

		expect(parseDeepLink("invalid-link")).toEqual({
			kind: "unknown",
			raw: "invalid-link",
		});
	});
});
