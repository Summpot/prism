import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import { getClientConfig } from "@/lib/client/clientIpc";
import { deriveManagementUrl } from "@/lib/panelConnection";
import { deleteProfile, saveActiveProfile, selectProfile } from "@/lib/state/clientActions";
import { EMPTY_CLIENT_CONFIG, patchActiveConfig } from "@/lib/state/clientConfig";
import { queryKeys } from "@/lib/state/queryKeys";

export function useClientConfig() {
	const query = useQuery({
		queryKey: queryKeys.client.config,
		queryFn: getClientConfig,
		staleTime: Infinity,
	});

	const data = query.data ?? EMPTY_CLIENT_CONFIG;
	const cfg = data.active_config;
	const managementUrl = useMemo(
		() => deriveManagementUrl(cfg.server_addr) || "http://127.0.0.1:8080",
		[cfg.server_addr],
	);

	return {
		configLoaded: query.isSuccess,
		profiles: data.profiles,
		selectedProfileId: data.active_profile_id ?? "",
		profileName: cfg.profile_name,
		setProfileName: (val: string) => patchActiveConfig({ profile_name: val }),
		serverAddr: cfg.server_addr,
		setServerAddr: (val: string) => patchActiveConfig({ server_addr: val }),
		transport: cfg.transport,
		setTransport: (val: string) => patchActiveConfig({ transport: val }),
		authToken: cfg.auth_token,
		setAuthToken: (val: string) => patchActiveConfig({ auth_token: val }),
		listenAddr: cfg.listen_addr,
		setListenAddr: (val: string) => patchActiveConfig({ listen_addr: val }),
		fakeLanBroadcast: cfg.fake_lan_broadcast,
		setFakeLanBroadcast: (val: boolean) => patchActiveConfig({ fake_lan_broadcast: val }),
		autoConnectPanel: cfg.auto_connect_panel,
		setAutoConnectPanel: (val: boolean) => patchActiveConfig({ auto_connect_panel: val }),
		autoConnect: cfg.auto_connect ?? true,
		setAutoConnect: (val: boolean) => patchActiveConfig({ auto_connect: val }),
		managementUrl,
		deviceId: data.device_id ?? "",
		cumulativeStats: data.cumulative_stats,
		handleSelectProfile: (id: string) => {
			void selectProfile(id);
		},
		handleSaveProfile: () => saveActiveProfile(),
		handleDeleteProfile: (id: string) => deleteProfile(id),
	};
}
