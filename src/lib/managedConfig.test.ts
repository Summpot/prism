import { describe, expect, it } from "vitest";

import {
	createEmptyManagedConfig,
	managedConfigFingerprint,
	moveItem,
	normalizeManagedConfig,
	parseManagedConfigJson,
	summarizeManagedConfig,
	validateManagedConfig,
} from "@/lib/managedConfig";

describe("managedConfig helpers", () => {
	it("flags missing routing fields and invalid UDP listeners", () => {
		const doc = createEmptyManagedConfig();
		doc.listeners = [{ listen_addr: ":19132", protocol: "udp", upstream: "" }];
		doc.routes = [
			{
				hosts: ["play.example.com"],
				upstreams: ["127.0.0.1:25565"],
				middlewares: [],
				strategy: "sequential",
			},
		];

		const issues = validateManagedConfig(doc);
		expect(issues.some((issue) => issue.path === "listeners.0.upstream")).toBe(true);
		expect(issues.some((issue) => issue.path === "routes.0.middlewares")).toBe(true);
		expect(issues.some((issue) => issue.path === "listeners")).toBe(true);
	});

	it("normalizes whitespace and middleware aliases", () => {
		const doc = createEmptyManagedConfig();
		doc.routes = [
			{
				hosts: [" play.example.com "],
				upstreams: [" 127.0.0.1:25565 "],
				middlewares: ["Minecraft-Handshake"],
				strategy: "",
			},
		];

		const normalized = normalizeManagedConfig(doc);
		expect(normalized.routes[0]).toEqual({
			hosts: ["play.example.com"],
			upstreams: ["127.0.0.1:25565"],
			middlewares: ["minecraft_handshake"],
			strategy: "sequential",
		});
	});

	it("summarizes a populated document", () => {
		const doc = createEmptyManagedConfig();
		doc.listeners = [
			{ listen_addr: ":25565", protocol: "tcp", upstream: "" },
			{ listen_addr: ":19132", protocol: "udp", upstream: "127.0.0.1:19132" },
		];
		doc.routes = [
			{
				hosts: ["play.example.com"],
				upstreams: ["127.0.0.1:25565"],
				middlewares: ["minecraft_handshake"],
				strategy: "sequential",
			},
		];
		doc.tunnel = {
			auth_token: "",
			auto_listen_services: true,
			endpoints: [{ listen_addr: ":7000", transport: "tcp" }],
			client: null,
			services: [
				{
					name: "home",
					proto: "tcp",
					local_addr: "127.0.0.1:25565",
					route_only: true,
					remote_addr: "",
					masquerade_host: "",
				},
			],
		};

		expect(summarizeManagedConfig(doc)).toMatchObject({
			listeners: 2,
			tcpListeners: 1,
			udpListeners: 1,
			hostnameRoutingListeners: 1,
			routes: 1,
			tunnelEnabled: true,
			tunnelEndpoints: 1,
			tunnelServices: 1,
			hasTunnelClient: false,
		});
	});

	it("parses JSON documents and rejects invalid roots", () => {
		const parsed = parseManagedConfigJson(
			JSON.stringify({
				listeners: [{ listen_addr: ":25565", protocol: "tcp", upstream: "" }],
				routes: [],
			}),
		);
		expect(parsed.ok).toBe(true);
		if (parsed.ok) {
			expect(parsed.value.listeners).toHaveLength(1);
		}

		expect(parseManagedConfigJson("[]").ok).toBe(false);
		expect(parseManagedConfigJson("{").ok).toBe(false);
	});

	it("moves items and fingerprints normalized docs", () => {
		expect(moveItem(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
		const a = createEmptyManagedConfig();
		const b = createEmptyManagedConfig();
		b.max_header_bytes = 1;
		expect(managedConfigFingerprint(a)).not.toEqual(managedConfigFingerprint(b));
	});
});
