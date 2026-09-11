import { useCallback, useState } from "react";

import { parseDeepLink } from "@/lib/deepLink";
import {
	SUPPORTED_LINK_PROTOCOLS,
	encodePrismLink,
	extractProtocolAndAddress,
	parsePrismLink,
} from "@/lib/prismLink";
import { m } from "@/paraglide/messages";

interface UsePrismLinkProps {
	serverAddr: string;
	setServerAddr: (addr: string) => void;
	transport: string;
	setTransport: (t: string) => void;
	profileName: string;
	setProfileName: (n: string) => void;
	listenAddr: string;
	setListenAddr: (addr: string) => void;
	authToken: string;
	fakeLanBroadcast: boolean;
	setFakeLanBroadcast: (b: boolean) => void;
	onConnectFromLink?: (link: string) => void;
}

export function usePrismLink({
	serverAddr,
	setServerAddr,
	transport,
	setTransport,
	profileName,
	setProfileName,
	listenAddr,
	setListenAddr,
	authToken,
	fakeLanBroadcast,
	setFakeLanBroadcast,
	onConnectFromLink,
}: UsePrismLinkProps) {
	const [remoteLinkInput, setRemoteLinkInput] = useState("");
	const [linkProtocol, setLinkProtocol] = useState<string>("auto://");
	const [copied, setCopied] = useState<string | null>(null);

	// Import modal state
	const [importModalOpen, setImportModalOpen] = useState(false);
	const [importUrl, setImportUrl] = useState("");
	const [importError, setImportError] = useState<string | null>(null);

	const copyText = useCallback((text: string, id: string) => {
		navigator.clipboard.writeText(text);
		setCopied(id);
		setTimeout(() => setCopied(null), 2000);
	}, []);

	const handleSelectProtocol = useCallback(
		(newProtocol: string) => {
			setLinkProtocol(newProtocol);
			const matched = SUPPORTED_LINK_PROTOCOLS.find((p) => p.value === newProtocol);
			if (matched?.transport) {
				setTransport(matched.transport);
			}
		},
		[setTransport],
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
						setTransport(matched.transport);
					}
				} else {
					setLinkProtocol(protocol);
				}
				setRemoteLinkInput(address);
			} else {
				setRemoteLinkInput(val);
			}
		},
		[setTransport],
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
					if (onConnectFromLink) {
						onConnectFromLink(trimmedText);
					}
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
						setTransport(matched.transport);
					}
				} else {
					setLinkProtocol(protocol);
				}
				setRemoteLinkInput(address);
			}
		},
		[onConnectFromLink, setTransport],
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

		if (parsed.name) setProfileName(parsed.name);
		setServerAddr(parsed.server_addr);
		if (parsed.transport) setTransport(parsed.transport);
		if (parsed.listen_addr) setListenAddr(parsed.listen_addr);
		if (parsed.fake_lan_broadcast !== undefined) {
			setFakeLanBroadcast(parsed.fake_lan_broadcast);
		}

		setImportModalOpen(false);
		setImportUrl("");
	}, [importUrl, setFakeLanBroadcast, setListenAddr, setProfileName, setServerAddr, setTransport]);

	const handleShareLink = useCallback(() => {
		const link = encodePrismLink({
			name: profileName,
			server_addr: serverAddr,
			transport,
			auth_token: authToken,
			listen_addr: listenAddr,
			fake_lan_broadcast: fakeLanBroadcast,
		});

		navigator.clipboard.writeText(link);
		setCopied("share");
		setTimeout(() => setCopied(null), 2000);
	}, [authToken, fakeLanBroadcast, listenAddr, profileName, serverAddr, transport]);

	return {
		remoteLinkInput,
		setRemoteLinkInput,
		linkProtocol,
		setLinkProtocol,
		copied,
		setCopied,
		copyText,
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
