import { useCallback } from "react";

import { parseDeepLink } from "@/lib/deepLink";
import {
	SUPPORTED_LINK_PROTOCOLS,
	encodePrismLink,
	extractProtocolAndAddress,
	parsePrismLink,
} from "@/lib/prismLink";
import { applyImportedProfile, connectFromLink } from "@/lib/state/clientActions";
import { patchActiveConfig, readClientConfig } from "@/lib/state/clientConfig";
import { useClientUiStore } from "@/lib/state/clientUiStore";
import { m } from "@/paraglide/messages";

export function useClientLink() {
	const remoteLinkInput = useClientUiStore((s) => s.remoteLinkInput);
	const setRemoteLinkInput = useClientUiStore((s) => s.setRemoteLinkInput);
	const linkProtocol = useClientUiStore((s) => s.linkProtocol);
	const setLinkProtocol = useClientUiStore((s) => s.setLinkProtocol);
	const copied = useClientUiStore((s) => s.copied);
	const copyText = useClientUiStore((s) => s.copyText);
	const setCopied = useClientUiStore((s) => s.setCopied);
	const importModalOpen = useClientUiStore((s) => s.importModalOpen);
	const setImportModalOpen = useClientUiStore((s) => s.setImportModalOpen);
	const importUrl = useClientUiStore((s) => s.importUrl);
	const setImportUrl = useClientUiStore((s) => s.setImportUrl);
	const importError = useClientUiStore((s) => s.importError);
	const setImportError = useClientUiStore((s) => s.setImportError);

	const handleSelectProtocol = useCallback(
		(newProtocol: string) => {
			setLinkProtocol(newProtocol);
			const matched = SUPPORTED_LINK_PROTOCOLS.find((p) => p.value === newProtocol);
			if (matched?.transport) {
				patchActiveConfig({ transport: matched.transport });
			}
		},
		[setLinkProtocol],
	);

	const handleAddressChange = useCallback(
		(val: string) => {
			const { protocol, address } = extractProtocolAndAddress(val);
			if (protocol) {
				const matched = SUPPORTED_LINK_PROTOCOLS.find(
					(p) => p.value.toLowerCase() === protocol.toLowerCase(),
				);
				if (matched) {
					setLinkProtocol(matched.value);
					if (matched.transport) {
						patchActiveConfig({ transport: matched.transport });
					}
				} else {
					setLinkProtocol(protocol);
				}
				setRemoteLinkInput(address);
			} else {
				setRemoteLinkInput(val);
			}
		},
		[setLinkProtocol, setRemoteLinkInput],
	);

	const handleAddressPaste = useCallback(
		(e: React.ClipboardEvent<HTMLInputElement>) => {
			const text = e.clipboardData.getData("text");
			if (!text) return;
			const trimmedText = text.trim();
			if (
				trimmedText.toLowerCase().startsWith("prism://") &&
				(trimmedText.includes("code=") || trimmedText.includes("token="))
			) {
				const deep = parseDeepLink(trimmedText);
				if (deep.kind === "auth-code" || deep.kind === "auth") {
					e.preventDefault();
					setRemoteLinkInput(trimmedText);
					void connectFromLink(trimmedText);
					return;
				}
			}

			const { protocol, address } = extractProtocolAndAddress(text);
			if (protocol) {
				e.preventDefault();
				const matched = SUPPORTED_LINK_PROTOCOLS.find(
					(p) => p.value.toLowerCase() === protocol.toLowerCase(),
				);
				if (matched) {
					setLinkProtocol(matched.value);
					if (matched.transport) {
						patchActiveConfig({ transport: matched.transport });
					}
				} else {
					setLinkProtocol(protocol);
				}
				setRemoteLinkInput(address);
			}
		},
		[setLinkProtocol, setRemoteLinkInput],
	);

	const handleAddressCopy = useCallback(
		(e: React.ClipboardEvent<HTMLInputElement>) => {
			const sel = window.getSelection()?.toString();
			if (sel && sel.trim() === remoteLinkInput.trim() && !remoteLinkInput.includes("://")) {
				e.preventDefault();
				e.clipboardData.setData("text/plain", `${linkProtocol}${remoteLinkInput}`);
			}
		},
		[linkProtocol, remoteLinkInput],
	);

	const handleImportLink = useCallback(() => {
		setImportError(null);
		const parsed = parsePrismLink(importUrl);
		if (!parsed || !parsed.server_addr) {
			setImportError(m.client_invalid_link());
			return;
		}
		applyImportedProfile(parsed);
		setImportModalOpen(false);
		setImportUrl("");
	}, [importUrl, setImportError, setImportModalOpen, setImportUrl]);

	const handleShareLink = useCallback(() => {
		const cfg = readClientConfig().active_config;
		const link = encodePrismLink({
			name: cfg.profile_name,
			server_addr: cfg.server_addr,
			transport: cfg.transport,
			auth_token: cfg.auth_token,
			listen_addr: cfg.listen_addr,
			fake_lan_broadcast: cfg.fake_lan_broadcast,
		});
		copyText(link, "share");
	}, [copyText]);

	return {
		remoteLinkInput,
		setRemoteLinkInput,
		linkProtocol,
		setLinkProtocol,
		copied,
		copyText,
		setCopied,
		handleSelectProtocol,
		handleAddressChange,
		handleAddressPaste,
		handleAddressCopy,
		importModalOpen,
		setImportModalOpen,
		importUrl,
		setImportUrl,
		importError,
		setImportError,
		handleImportLink,
		handleShareLink,
	};
}
