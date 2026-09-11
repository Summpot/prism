import { describe, expect, it } from "vitest";

import { selectClientAuthView, type ClientAuthViewInput } from "./clientAuthView";

function input(overrides: Partial<ClientAuthViewInput> = {}): ClientAuthViewInput {
	return {
		isConnected: false,
		isRunning: false,
		authenticated: false,
		isAdmin: false,
		loginAdminUnlocked: false,
		authToken: "",
		isLoadingSession: false,
		knownServiceCount: 0,
		hasGithubProvider: false,
		providersError: false,
		oauthExchanging: false,
		oauthWaitingCallback: false,
		...overrides,
	};
}

describe("selectClientAuthView", () => {
	it("shows login methods after the tunnel is up and providers are probed", () => {
		const view = selectClientAuthView(
			input({
				isConnected: true,
				isRunning: true,
				hasGithubProvider: true,
			}),
		);
		expect(view.showLoggedInCard).toBe(false);
		expect(view.showLoginMethods).toBe(true);
		expect(view.loginRequired).toBe(true);
		expect(view.tunnelAction).toBe("disconnect");
	});

	it("hides login methods and shows the identity card after a live admin session", () => {
		const view = selectClientAuthView(
			input({
				isConnected: true,
				isRunning: true,
				authenticated: true,
				isAdmin: true,
				authToken: "prism_cl_abc",
				knownServiceCount: 2,
				hasGithubProvider: true,
			}),
		);
		expect(view.showLoggedInCard).toBe(true);
		expect(view.showLoginMethods).toBe(false);
		expect(view.showAdminConsole).toBe(true);
		expect(view.loginRequired).toBe(false);
	});

	it("does not show a logged-in card after the tunnel is stopped", () => {
		const view = selectClientAuthView(
			input({
				authenticated: true,
				isAdmin: true,
				authToken: "prism_cl_abc",
				hasGithubProvider: true,
			}),
		);
		expect(view.showLoggedInCard).toBe(false);
		expect(view.showAdminConsole).toBe(false);
		expect(view.showLoginMethods).toBe(false);
		expect(view.tunnelAction).toBe("start");
	});

	it("lets a stored token restart the tunnel without signing in again", () => {
		const view = selectClientAuthView(input({ authToken: "prism_cl_abc" }));
		expect(view.tunnelAction).toBe("start");
		expect(view.showLoggedInCard).toBe(false);
		expect(view.showLoginMethods).toBe(false);
	});

	it("hides login methods while a session refresh is in flight", () => {
		const view = selectClientAuthView(
			input({
				isConnected: true,
				isRunning: true,
				authToken: "prism_cl_abc",
				isLoadingSession: true,
				hasGithubProvider: true,
			}),
		);
		expect(view.showLoginMethods).toBe(false);
		expect(view.showLoggedInCard).toBe(false);
	});

	it("shows wait-for-login when idle with no token", () => {
		const view = selectClientAuthView(input());
		expect(view.tunnelAction).toBe("wait-login");
		expect(view.showLoggedInCard).toBe(false);
	});
});
