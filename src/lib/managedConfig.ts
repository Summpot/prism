/**
 * @deprecated This module is maintained for backwards compatibility.
 * Use `@/lib/cluster/nodeConfig` for node configuration utilities.
 * Use `@/types/cluster` for node configuration type definitions.
 */

export type {
	ConfigIssue,
	ManagedConfigSummary,
	NodeConfigSummary,
} from "@/lib/cluster/nodeConfig";

export {
	createEmptyManagedConfig,
	createEmptyNodeConfig,
	createEmptyRoute,
	createEmptyTunnel,
	createEmptyTunnelClient,
	createEmptyTunnelEndpoint,
	createEmptyTunnelService,
	formatIssuesByPath,
	managedConfigFingerprint,
	moveItem,
	nodeConfigFingerprint,
	normalizeManagedConfig,
	normalizeManagedRoute,
	normalizeNodeConfig,
	normalizeNodeRoute,
	parseManagedConfigJson,
	parseNodeConfigJson,
	summarizeManagedConfig,
	summarizeNodeConfig,
	validateManagedConfig,
	validateNodeConfig,
} from "@/lib/cluster/nodeConfig";
