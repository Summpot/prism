/**
 * @deprecated This module is maintained for backwards compatibility.
 * Use `@/lib/client/clientIpc` for desktop client IPC operations.
 * Use `@/lib/admin/adminApi` for remote management REST API operations.
 * Use `@/types/client`, `@/types/admin`, and `@/types/cluster` for type definitions.
 */

// Re-export all Types
export type * from "@/types/client";
export type * from "@/types/admin";
export type * from "@/types/cluster";

// Re-export Admin Client and Error
export { AdminApiError, ManagementApiError, adminRequest } from "@/lib/admin/adminClient";

// Re-export Remote Admin REST APIs
export {
	createAuthToken,
	exchangeGitHubCode,
	getAuthProviders,
	getAuthSession,
	getConfigPath,
	getConnections,
	getGitHubLoginUrl,
	getHealth,
	getManagedNode,
	getManagedNodeConfig,
	getManagedNodes,
	getManagementStatus,
	getMiddlewareConfig,
	getMiddlewareSchema,
	getOptimizerStats,
	getTunnelServices,
	listAuthTokens,
	listManagedUsers,
	listMiddlewares,
	resetMiddlewareConfig,
	revokeAuthToken,
	triggerReload,
	updateManagedNodeConfig,
	updateManagedUser,
	updateMiddlewareConfig,
} from "@/lib/admin/adminApi";

// Re-export Desktop Client IPC APIs
export {
	clearClientLogs,
	getClientConfig,
	getClientLogs,
	getClientProfiles,
	getClientStatus,
	listLocalMiddlewares,
	resetClientStats,
	resetLocalMiddlewareConfig,
	saveClientConfig,
	saveClientProfiles,
	startClient,
	stopClient,
	updateLocalMiddlewareConfig,
} from "@/lib/client/clientIpc";
