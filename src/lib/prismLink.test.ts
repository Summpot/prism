import { describe, expect, it } from "vitest";

import {
	encodePrismLink,
	extractProtocolAndAddress,
	parsePrismLink,
	resolveRemoteConnection,
} from "./prismLink";

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
		expect(link).not.toContain("token=");
		expect(link).toContain("transport=kcp");
		expect(link).toContain("fake_lan=0");

		const parsed = parsePrismLink(link);
		expect(parsed).not.toBeNull();
		expect(parsed?.name).toBe("Survival Realm");
		expect(parsed?.server_addr).toBe("play.example.com:7000");
		expect(parsed?.transport).toBe("kcp");
		expect(parsed?.auth_token).toBe("");
		expect(parsed?.listen_addr).toBe("127.0.0.1:25566");
		expect(parsed?.fake_lan_broadcast).toBe(false);
	});

	it("still decodes legacy prism:// links with token", () => {
		const legacyLink = "prism://play.example.com:7000?name=Legacy&token=old-token&transport=tcp";
		const parsed = parsePrismLink(legacyLink);
		expect(parsed).not.toBeNull();
		expect(parsed?.name).toBe("Legacy");
		expect(parsed?.auth_token).toBe("old-token");
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

	it("resolves various remote connection string formats", () => {
		const fromPrism = resolveRemoteConnection(
			"prism://play.example.com:7000?transport=kcp",
		);
		expect(fromPrism.managementUrl).toBe("http://play.example.com:8080");
		expect(fromPrism.serverAddr).toBe("play.example.com:7000");
		expect(fromPrism.transport).toBe("kcp");

		const fromHttp = resolveRemoteConnection("http://192.168.1.50:8080");
		expect(fromHttp.managementUrl).toBe("http://192.168.1.50:8080");
		expect(fromHttp.serverAddr).toBe("192.168.1.50:7000");

		const fromHost = resolveRemoteConnection("relay.mydomain.org:7000");
		expect(fromHost.managementUrl).toBe("http://relay.mydomain.org:8080");
		expect(fromHost.serverAddr).toBe("relay.mydomain.org:7000");

		const fromBare = resolveRemoteConnection("relay.mydomain.org");
		expect(fromBare.managementUrl).toBe("http://relay.mydomain.org:8080");
		expect(fromBare.serverAddr).toBe("relay.mydomain.org:7000");

		const fromQuic = resolveRemoteConnection("quic://play.example.com:7000");
		expect(fromQuic.serverAddr).toBe("play.example.com:7000");
		expect(fromQuic.transport).toBe("quic");

		const fromTcp = resolveRemoteConnection("tcp://10.0.0.1:9000");
		expect(fromTcp.serverAddr).toBe("10.0.0.1:9000");
		expect(fromTcp.transport).toBe("tcp");

		const fromWs = resolveRemoteConnection("ws://gateway.org:8000");
		expect(fromWs.serverAddr).toBe("gateway.org:8000");
		expect(fromWs.transport).toBe("websocket");
	});

	it("extracts protocol and address correctly", () => {
		expect(extractProtocolAndAddress("prism://play.example.com:7000?transport=kcp")).toEqual({
			protocol: "kcp://",
			address: "play.example.com:7000",
		});
		expect(extractProtocolAndAddress("prism://play.example.com:7000")).toEqual({
			protocol: "quic://",
			address: "play.example.com:7000",
		});
		expect(extractProtocolAndAddress("QUIC://node.com:7000")).toEqual({
			protocol: "quic://",
			address: "node.com:7000",
		});
		expect(extractProtocolAndAddress("tcp://1.2.3.4:7000")).toEqual({
			protocol: "tcp://",
			address: "1.2.3.4:7000",
		});
		expect(extractProtocolAndAddress("play.example.com:7000")).toEqual({
			protocol: null,
			address: "play.example.com:7000",
		});
		expect(extractProtocolAndAddress("")).toEqual({
			protocol: null,
			address: "",
		});
	});
});
