import { describe, expect, it } from "vitest";

import {
	createEmptyManagedConfig,
	normalizeManagedConfig,
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
});
