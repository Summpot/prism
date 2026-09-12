import { getClientStatus } from "@/lib/client/clientIpc";
import { getQueryClient } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import { useClientUiStore } from "@/lib/state/clientUiStore";
import type { ClientStatusResponse } from "@/types/client";

export async function refreshClientStatus(): Promise<ClientStatusResponse | null> {
	try {
		const status = await getClientStatus();
		getQueryClient().setQueryData(queryKeys.client.status, status);
		useClientUiStore.getState().ingestStatus(status);
		return status;
	} catch {
		return null;
	}
}

export async function waitForClientConnected(): Promise<ClientStatusResponse | null> {
	let latest: ClientStatusResponse | null = null;
	for (let i = 0; i < 25; i++) {
		const status = await refreshClientStatus();
		if (status) {
			latest = status;
			if (status.state === "connected") {
				return status;
			}
		}
		await new Promise((resolve) => setTimeout(resolve, 200));
	}
	return latest;
}

export async function invalidateClientQueries(): Promise<void> {
	const queryClient = getQueryClient();
	await Promise.all([
		queryClient.invalidateQueries({ queryKey: queryKeys.client.status }),
		queryClient.invalidateQueries({ queryKey: queryKeys.client.config }),
		queryClient.invalidateQueries({ queryKey: queryKeys.client.logs }),
	]);
}
