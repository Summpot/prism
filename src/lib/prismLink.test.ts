import { describe, expect, it } from "vitest";

import { encodePrismLink, parsePrismLink } from "./prismLink";

describe("prismLink", () => {
	it("encodes and decodes standard prism:// links", () => {
		const profile = {
			name: "Survival Realm",
			server_addr: "play.example.com:7000",
			transport: "kcp",
			auth_token: "super-secret-token",
			listen_addr: "127.0.0.1:25566",
			fake_lan_broadcast: false,
		};

		const link = encodePrismLink(profile);
		expect(link).toContain("prism://play.example.com:7000");
		expect(link).toContain("name=Survival+Realm");
		expect(link).toContain("token=super-secret-token");
		expect(link).toContain("transport=kcp");
		expect(link).toContain("fake_lan=0");

		const parsed = parsePrismLink(link);
		expect(parsed).not.toBeNull();
		expect(parsed?.name).toBe("Survival Realm");
		expect(parsed?.server_addr).toBe("play.example.com:7000");
		expect(parsed?.transport).toBe("kcp");
		expect(parsed?.auth_token).toBe("super-secret-token");
		expect(parsed?.listen_addr).toBe("127.0.0.1:25566");
		expect(parsed?.fake_lan_broadcast).toBe(false);
	});

	it("handles minimal server address strings", () => {
		const parsed = parsePrismLink("prism://1.2.3.4:7000");
		expect(parsed).not.toBeNull();
		expect(parsed?.server_addr).toBe("1.2.3.4:7000");
		expect(parsed?.transport).toBe("quic");
		expect(parsed?.listen_addr).toBe("127.0.0.1:25565");
		expect(parsed?.fake_lan_broadcast).toBe(true);
	});

	it("handles plain host:port input as fallback", () => {
		const parsed = parsePrismLink("relay.prism.gg:7000");
		expect(parsed).not.toBeNull();
		expect(parsed?.server_addr).toBe("relay.prism.gg:7000");
	});

	it("returns null for invalid inputs", () => {
		expect(parsePrismLink("")).toBeNull();
		expect(parsePrismLink("not a link")).toBeNull();
	});
});
