import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
	getClientConfig,
	getClientProfiles,
	saveClientConfig,
	saveClientProfiles,
} from "@/lib/client/clientIpc";
import { deriveManagementUrl } from "@/lib/panelConnection";
import type { ClientProfile, CumulativeStats } from "@/types/client";

interface UseClientProfilesOptions {
	onCumulativeStatsLoaded?: (stats: CumulativeStats) => void;
	onRemoteLinkInputSync?: (serverAddr: string) => void;
}

export function useClientProfiles(options?: UseClientProfilesOptions) {
	const [profiles, setProfiles] = useState<ClientProfile[]>([]);
	const [selectedProfileId, setSelectedProfileId] = useState<string>("");
	const [serverAddr, setServerAddr] = useState("127.0.0.1");
	const [transport, setTransport] = useState("auto");
	const [authToken, setAuthToken] = useState("");
	const [listenAddr, setListenAddr] = useState("127.0.0.1:25565");
	const [fakeLanBroadcast, setFakeLanBroadcast] = useState(true);
	const [profileName, setProfileName] = useState("Default Realm");
	const [autoConnectPanel, setAutoConnectPanel] = useState(true);

	const configLoadedRef = useRef(false);
	const autoSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

	const managementUrl = useMemo(
		() => deriveManagementUrl(serverAddr) || "http://127.0.0.1:8080",
		[serverAddr],
	);

	// Fetch full client configuration from persistent storage on mount
	const fetchClientConfigData = useCallback(() => {
		getClientConfig()
			.then((resp) => {
				setProfiles(resp.profiles);
				if (resp.cumulative_stats && options?.onCumulativeStatsLoaded) {
					options.onCumulativeStatsLoaded(resp.cumulative_stats);
				}

				if (!configLoadedRef.current) {
					configLoadedRef.current = true;
					if (resp.active_config) {
						setProfileName(resp.active_config.profile_name || "Default Realm");
						const sAddr = resp.active_config.server_addr || "127.0.0.1";
						setServerAddr(sAddr);
						if (options?.onRemoteLinkInputSync) {
							options.onRemoteLinkInputSync(sAddr);
						}
						setTransport(resp.active_config.transport || "auto");
						setAuthToken(resp.active_config.auth_token || "");
						setListenAddr(resp.active_config.listen_addr || "127.0.0.1:25565");
						setFakeLanBroadcast(resp.active_config.fake_lan_broadcast ?? true);
						setAutoConnectPanel(resp.active_config.auto_connect_panel ?? true);
					}
					if (resp.active_profile_id) {
						setSelectedProfileId(resp.active_profile_id);
					} else if (resp.profiles.length > 0) {
						setSelectedProfileId(resp.profiles[0].id);
					}
				}
			})
			.catch(() => {
				getClientProfiles()
					.then((list) => {
						setProfiles(list);
						if (list.length > 0 && !selectedProfileId && !configLoadedRef.current) {
							configLoadedRef.current = true;
							const first = list[0];
							setSelectedProfileId(first.id);
							setProfileName(first.name);
							setServerAddr(first.server_addr);
							setTransport(first.transport);
							setAuthToken(first.auth_token);
							setListenAddr(first.listen_addr);
							setFakeLanBroadcast(first.fake_lan_broadcast);
						}
					})
					.catch(() => {});
			});
	}, [options, selectedProfileId]);

	useEffect(() => {
		fetchClientConfigData();
	}, [fetchClientConfigData]);

	// Debounced auto-save of active configuration
	useEffect(() => {
		if (!configLoadedRef.current) return;
		if (autoSaveTimerRef.current) {
			clearTimeout(autoSaveTimerRef.current);
		}
		autoSaveTimerRef.current = setTimeout(() => {
			saveClientConfig({
				active_profile_id: selectedProfileId || null,
				active_config: {
					server_addr: serverAddr,
					transport,
					auth_token: authToken,
					listen_addr: listenAddr,
					fake_lan_broadcast: fakeLanBroadcast,
					auto_connect_panel: autoConnectPanel,
				},
			}).catch(() => {});
		}, 500);

		return () => {
			if (autoSaveTimerRef.current) {
				clearTimeout(autoSaveTimerRef.current);
			}
		};
	}, [
		selectedProfileId,
		serverAddr,
		transport,
		authToken,
		listenAddr,
		fakeLanBroadcast,
		autoConnectPanel,
	]);

	const handleSelectProfile = useCallback(
		(id: string) => {
			setSelectedProfileId(id);
			const p = profiles.find((item) => item.id === id);
			if (p) {
				setProfileName(p.name);
				setServerAddr(p.server_addr);
				setTransport(p.transport);
				setAuthToken(p.auth_token);
				setListenAddr(p.listen_addr);
				setFakeLanBroadcast(p.fake_lan_broadcast);
				saveClientConfig({
					active_profile_id: id,
					active_config: {
						server_addr: p.server_addr,
						transport: p.transport,
						auth_token: p.auth_token,
						listen_addr: p.listen_addr,
						fake_lan_broadcast: p.fake_lan_broadcast,
						auto_connect_panel: autoConnectPanel,
					},
				}).catch(() => {});
			}
		},
		[autoConnectPanel, profiles],
	);

	const handleSaveProfile = useCallback(async () => {
		const existingIndex = profiles.findIndex(
			(p) => p.id === selectedProfileId || p.server_addr === serverAddr,
		);
		const id = selectedProfileId || `profile-${Date.now()}`;
		const newProfile: ClientProfile = {
			id,
			name: profileName || serverAddr,
			server_addr: serverAddr,
			transport,
			auth_token: authToken,
			listen_addr: listenAddr,
			fake_lan_broadcast: fakeLanBroadcast,
		};

		let updated: ClientProfile[];
		if (existingIndex >= 0) {
			updated = [...profiles];
			updated[existingIndex] = newProfile;
		} else {
			updated = [...profiles, newProfile];
		}

		setProfiles(updated);
		setSelectedProfileId(id);
		await saveClientProfiles(updated).catch(() => {});
		await saveClientConfig({
			active_profile_id: id,
			active_config: {
				server_addr: serverAddr,
				transport,
				auth_token: authToken,
				listen_addr: listenAddr,
				fake_lan_broadcast: fakeLanBroadcast,
				auto_connect_panel: autoConnectPanel,
			},
		}).catch(() => {});
	}, [
		authToken,
		autoConnectPanel,
		fakeLanBroadcast,
		listenAddr,
		profileName,
		profiles,
		selectedProfileId,
		serverAddr,
		transport,
	]);

	const handleDeleteProfile = useCallback(
		async (id: string) => {
			const updated = profiles.filter((p) => p.id !== id);
			setProfiles(updated);
			const nextActiveId = selectedProfileId === id ? updated[0]?.id || "" : selectedProfileId;
			if (selectedProfileId === id) {
				setSelectedProfileId(nextActiveId);
				if (updated[0]) {
					const first = updated[0];
					setProfileName(first.name);
					setServerAddr(first.server_addr);
					setTransport(first.transport);
					setAuthToken(first.auth_token);
					setListenAddr(first.listen_addr);
					setFakeLanBroadcast(first.fake_lan_broadcast);
				}
			}
			await saveClientProfiles(updated).catch(() => {});
			await saveClientConfig({
				active_profile_id: nextActiveId || null,
			}).catch(() => {});
		},
		[profiles, selectedProfileId],
	);

	return {
		profiles,
		setProfiles,
		selectedProfileId,
		setSelectedProfileId,
		profileName,
		setProfileName,
		serverAddr,
		setServerAddr,
		transport,
		setTransport,
		authToken,
		setAuthToken,
		listenAddr,
		setListenAddr,
		fakeLanBroadcast,
		setFakeLanBroadcast,
		autoConnectPanel,
		setAutoConnectPanel,
		managementUrl,
		handleSelectProfile,
		handleSaveProfile,
		handleDeleteProfile,
		fetchClientConfigData,
	};
}
