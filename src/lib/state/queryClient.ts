import { QueryClient } from "@tanstack/react-query";

function createQueryClient() {
	return new QueryClient({
		defaultOptions: {
			queries: {
				retry: 1,
				refetchOnWindowFocus: false,
				refetchOnReconnect: false,
				staleTime: 1_000,
			},
			mutations: {
				retry: 0,
			},
		},
	});
}

let queryClient: QueryClient | null = null;

export function getQueryClient(): QueryClient {
	if (!queryClient) {
		queryClient = createQueryClient();
	}
	return queryClient;
}

export function resetQueryClient(): QueryClient {
	queryClient?.clear();
	queryClient = createQueryClient();
	return queryClient;
}

export function invalidateAdminQueries() {
	return getQueryClient().invalidateQueries({ queryKey: ["admin"] });
}
