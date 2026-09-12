import { useClientActions } from "@/hooks/useClientActions";
import { useClientAuth } from "@/hooks/useClientAuth";
import { useClientConfig } from "@/hooks/useClientConfig";
import { useClientLink } from "@/hooks/useClientLink";
import { useClientLogs } from "@/hooks/useClientLogs";
import { useClientRuntime } from "@/hooks/useClientRuntime";

/**
 * Compatibility facade over query + zustand stores.
 * Prefer the focused hooks in `@/hooks` for new UI.
 */
export function useClient() {
	const runtime = useClientRuntime();
	const config = useClientConfig();
	const link = useClientLink();
	const logs = useClientLogs();
	const auth = useClientAuth();
	const actions = useClientActions();

	return {
		...runtime,
		...config,
		...link,
		...logs,
		...auth,
		...actions,
		handleManualOAuthCallback: actions.handleManualOAuthCallback,
		handleConnectFromLink: actions.handleConnectFromLink,
	};
}

/** State lives in AppStateProvider. Kept so existing imports compile. */
export function ClientProvider({ children }: { children: React.ReactNode }) {
	return <>{children}</>;
}
