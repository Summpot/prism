export interface ClientAuthViewInput {
	isConnected: boolean;
	isRunning: boolean;
	authenticated: boolean;
	isAdmin: boolean;
	loginAdminUnlocked: boolean;
	authToken: string;
	isLoadingSession: boolean;
	knownServiceCount: number;
	hasGithubProvider: boolean;
	providersError: boolean;
	oauthExchanging: boolean;
	oauthWaitingCallback: boolean;
}

export interface ClientAuthView {
	/** Identity card is only meaningful while the tunnel (and `$admin`) is up. */
	showLoggedInCard: boolean;
	loginRequired: boolean;
	/** Hide sign-in methods once a live session exists. */
	showLoginMethods: boolean;
	showAdminConsole: boolean;
	tunnelAction: "disconnect" | "start" | "wait-login";
}

export function selectClientAuthView(input: ClientAuthViewInput): ClientAuthView {
	const hasToken = Boolean(input.authToken.trim());
	const liveAuthenticated = input.isConnected && input.authenticated;
	const loginRequired = input.isConnected && input.knownServiceCount === 0 && !input.authenticated;

	const showLoginMethods =
		!input.oauthExchanging &&
		!input.oauthWaitingCallback &&
		!input.authenticated &&
		!input.isLoadingSession &&
		(input.hasGithubProvider || input.providersError || loginRequired);

	let tunnelAction: ClientAuthView["tunnelAction"];
	if (input.isRunning) {
		tunnelAction = "disconnect";
	} else if (hasToken || input.authenticated) {
		tunnelAction = "start";
	} else {
		tunnelAction = "wait-login";
	}

	return {
		showLoggedInCard: liveAuthenticated,
		loginRequired,
		showLoginMethods,
		showAdminConsole: liveAuthenticated && (input.isAdmin || input.loginAdminUnlocked),
		tunnelAction,
	};
}
