import { useQuery, type UseQueryResult } from "@tanstack/react-query";

import { useAdminSession } from "@/lib/admin/adminSession";
import type { PanelConnection } from "@/lib/panelConnection";
import { shouldFetchAdminSession } from "@/lib/state/session";

export function queryErrorMessage(error: unknown): string | null {
	if (!error) {
		return null;
	}
	if (error instanceof Error) {
		return error.message;
	}
	return String(error);
}

export function useAdminQuery<T>(
	queryKey: readonly unknown[],
	fetcher: (connection: PanelConnection) => Promise<T>,
	options?: {
		refetchInterval?: number | false;
		enabled?: boolean;
		staleTime?: number;
	},
): UseQueryResult<T> & { errorMessage: string | null } {
	const { connection, ready, tunnelState } = useAdminSession();
	const enabled = Boolean(
		ready &&
		connection &&
		shouldFetchAdminSession(connection, tunnelState) &&
		(options?.enabled ?? true),
	);
	const query = useQuery({
		queryKey,
		queryFn: () => fetcher(connection!),
		enabled,
		refetchInterval: options?.refetchInterval ?? false,
		staleTime: options?.staleTime ?? 2_000,
		retry: 1,
	});
	return {
		...query,
		errorMessage: queryErrorMessage(query.error),
	};
}
