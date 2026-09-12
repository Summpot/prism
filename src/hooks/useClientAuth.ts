import { useClientUiStore } from "@/lib/state/clientUiStore";
import {
	handleManualOAuthCallback,
	redetectProviders,
	startGitHubAuthWithUrl,
} from "@/lib/state/clientActions";

export function useClientAuth() {
	const loginModalOpen = useClientUiStore((s) => s.loginModalOpen);
	const setLoginModalOpen = useClientUiStore((s) => s.setLoginModalOpen);
	const checkingProviders = useClientUiStore((s) => s.checkingProviders);
	const providersResult = useClientUiStore((s) => s.providersResult);
	const setProvidersResult = useClientUiStore((s) => s.setProvidersResult);
	const providersError = useClientUiStore((s) => s.providersError);
	const setProvidersError = useClientUiStore((s) => s.setProvidersError);
	const authServerUrl = useClientUiStore((s) => s.authServerUrl);
	const setAuthServerUrl = useClientUiStore((s) => s.setAuthServerUrl);
	const authError = useClientUiStore((s) => s.authError);
	const setAuthError = useClientUiStore((s) => s.setAuthError);
	const oauthLoading = useClientUiStore((s) => s.oauthLoading);
	const oauthWaitingCallback = useClientUiStore((s) => s.oauthWaitingCallback);
	const setOauthWaitingCallback = useClientUiStore((s) => s.setOauthWaitingCallback);
	const oauthExchanging = useClientUiStore((s) => s.oauthExchanging);
	const manualCallbackInput = useClientUiStore((s) => s.manualCallbackInput);
	const setManualCallbackInput = useClientUiStore((s) => s.setManualCallbackInput);
	const loginAdminUnlocked = useClientUiStore((s) => s.loginAdminUnlocked);

	return {
		loginModalOpen,
		setLoginModalOpen,
		checkingProviders,
		providersResult,
		setProvidersResult,
		providersError,
		setProvidersError,
		authServerUrl,
		setAuthServerUrl,
		authError,
		setAuthError,
		oauthLoading,
		oauthWaitingCallback,
		setOauthWaitingCallback,
		oauthExchanging,
		manualCallbackInput,
		setManualCallbackInput,
		loginAdminUnlocked,
		handleManualOAuthCallback,
		startGitHubAuthWithUrl,
		handleRedetectProviders: redetectProviders,
	};
}
