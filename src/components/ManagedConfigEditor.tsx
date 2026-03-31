import {
	AlertTriangle,
	Cable,
	CheckCircle2,
	Clock,
	CopyPlus,
	FolderSync,
	Globe,
	Plus,
	Router,
	Save,
	Trash2,
	Waypoints,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import {
	createEmptyManagedConfig,
	createEmptyTunnel,
	createEmptyTunnelClient,
	createEmptyTunnelEndpoint,
	createEmptyTunnelService,
	formatIssuesByPath,
	normalizeManagedConfig,
	validateManagedConfig,
} from "@/lib/managedConfig";
import type { ManagedConfigDocument, ManagedTunnelDocument } from "@/lib/managementApi";

function Card({
	title,
	description,
	icon,
	children,
}: {
	title: string;
	description: string;
	icon: React.ReactNode;
	children: React.ReactNode;
}) {
	return (
		<section className="rounded-3xl border border-white/8 bg-slate-950/75 p-6 shadow-[0_24px_80px_rgba(15,23,42,0.45)]">
			<div className="flex items-start justify-between gap-4">
				<div>
					<div className="flex items-center gap-3 text-white">
						<div className="rounded-2xl border border-cyan-400/25 bg-cyan-400/10 p-2 text-cyan-300">
							{icon}
						</div>
						<h2 className="text-lg font-semibold">{title}</h2>
					</div>
					<p className="mt-2 max-w-3xl text-sm leading-6 text-slate-400">{description}</p>
				</div>
			</div>
			<div className="mt-6">{children}</div>
		</section>
	);
}

function FieldError({ messages }: { messages?: string[] }) {
	if (!messages?.length) {
		return null;
	}

	return (
		<div className="mt-2 flex flex-col gap-1 text-sm text-amber-200">
			{messages.map((message) => (
				<div key={message} className="flex items-center gap-2">
					<AlertTriangle className="h-4 w-4 flex-none" />
					<span>{message}</span>
				</div>
			))}
		</div>
	);
}

function SectionLabel({ title, hint }: { title: string; hint: string }) {
	return (
		<div>
			<div className="text-sm font-medium text-white">{title}</div>
			<div className="mt-1 text-xs leading-5 text-slate-500">{hint}</div>
		</div>
	);
}

interface ManagedConfigEditorProps {
	initialConfig?: ManagedConfigDocument | null;
	isSaving: boolean;
	saveError?: string | null;
	onSave: (value: ManagedConfigDocument) => Promise<void>;
}

export function ManagedConfigEditor({
	initialConfig,
	isSaving,
	saveError,
	onSave,
}: ManagedConfigEditorProps) {
	const [draft, setDraft] = useState<ManagedConfigDocument>(
		initialConfig ?? createEmptyManagedConfig(),
	);

	useEffect(() => {
		setDraft(initialConfig ?? createEmptyManagedConfig());
	}, [initialConfig]);

	const normalizedDraft = useMemo(() => normalizeManagedConfig(draft), [draft]);
	const issues = useMemo(() => validateManagedConfig(draft), [draft]);
	const issueMap = useMemo(() => formatIssuesByPath(issues), [issues]);

	const updateListener = (index: number, key: string, value: string) => {
		setDraft((current) => ({
			...current,
			listeners: current.listeners.map((listener, listenerIndex) =>
				listenerIndex === index ? { ...listener, [key]: value } : listener,
			),
		}));
	};

	const updateRoute = (
		index: number,
		key: "hosts" | "upstreams" | "middlewares" | "strategy",
		value: string,
	) => {
		setDraft((current) => ({
			...current,
			routes: current.routes.map((route, routeIndex) => {
				if (routeIndex !== index) {
					return route;
				}

				if (key === "strategy") {
					return { ...route, strategy: value };
				}

				return {
					...route,
					[key]: value
						.split("\n")
						.map((entry) => entry.trim())
						.filter(Boolean),
				};
			}),
		}));
	};

	const addListener = () => {
		setDraft((current) => ({
			...current,
			listeners: [...current.listeners, { listen_addr: "", protocol: "tcp", upstream: "" }],
		}));
	};

	const addRoute = () => {
		setDraft((current) => ({
			...current,
			routes: [
				...current.routes,
				{
					hosts: [""],
					upstreams: [""],
					middlewares: ["minecraft_handshake"],
					strategy: "sequential",
				},
			],
		}));
	};

	const save = async () => {
		if (issues.length > 0) {
			return;
		}
		await onSave(normalizedDraft);
	};

	return (
		<div className="space-y-6">
			<Card
				title="Runtime envelope"
				description="Tune the hot-reload-safe runtime knobs that this node should converge to when the config revision is applied."
				icon={<FolderSync className="h-5 w-5" />}
			>
				<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
					<label className="space-y-2">
						<SectionLabel title="Max header bytes" hint="TCP routing prelude cap" />
						<input
							type="number"
							value={draft.max_header_bytes}
							onChange={(event) =>
								setDraft((current) => ({
									...current,
									max_header_bytes: Number(event.target.value),
								}))
							}
							className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
						/>
					</label>
					<label className="space-y-2">
						<SectionLabel title="Buffer size" hint="Copy buffer hint (bytes)" />
						<input
							type="number"
							value={draft.buffer_size}
							onChange={(event) =>
								setDraft((current) => ({
									...current,
									buffer_size: Number(event.target.value),
								}))
							}
							className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
						/>
					</label>
					<label className="space-y-2">
						<SectionLabel title="Dial timeout" hint="Upstream timeout in ms" />
						<input
							type="number"
							value={draft.upstream_dial_timeout_ms}
							onChange={(event) =>
								setDraft((current) => ({
									...current,
									upstream_dial_timeout_ms: Number(event.target.value),
								}))
							}
							className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
						/>
					</label>
					<label className="flex items-end gap-3 rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-sm text-slate-300">
						<input
							type="checkbox"
							checked={draft.proxy_protocol_v2}
							onChange={(event) =>
								setDraft((current) => ({
									...current,
									proxy_protocol_v2: event.target.checked,
								}))
							}
							className="h-4 w-4 accent-cyan-400"
						/>
						Inject PROXY protocol v2 on TCP upstream connections
					</label>
				</div>
			</Card>

			<Card
				title="Timeouts"
				description="Control how long the proxy plane waits during handshake and idle phases."
				icon={<Clock className="h-5 w-5" />}
			>
				<div className="grid gap-4 md:grid-cols-2">
					<label className="space-y-2">
						<SectionLabel title="Handshake timeout (ms)" hint="TCP prelude capture timeout" />
						<input
							type="number"
							value={draft.timeouts?.handshake_timeout_ms ?? 0}
							onChange={(event) =>
								setDraft((current) => ({
									...current,
									timeouts: {
										...current.timeouts,
										handshake_timeout_ms: Number(event.target.value),
										idle_timeout_ms: current.timeouts?.idle_timeout_ms ?? 0,
									},
								}))
							}
							className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
						/>
					</label>
					<label className="space-y-2">
						<SectionLabel
							title="Idle timeout (ms)"
							hint="Bidirectional copy lifetime cap (0 = no limit)"
						/>
						<input
							type="number"
							value={draft.timeouts?.idle_timeout_ms ?? 0}
							onChange={(event) =>
								setDraft((current) => ({
									...current,
									timeouts: {
										...current.timeouts,
										handshake_timeout_ms: current.timeouts?.handshake_timeout_ms ?? 0,
										idle_timeout_ms: Number(event.target.value),
									},
								}))
							}
							className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
						/>
					</label>
				</div>
			</Card>

			<Card
				title="Listeners"
				description="Define the traffic entrypoints the worker owns. UDP listeners must always forward to a concrete upstream."
				icon={<Cable className="h-5 w-5" />}
			>
				<div className="space-y-4">
					{draft.listeners.map((listener, index) => (
						<div
							key={`${listener.listen_addr}-${index}`}
							className="rounded-3xl border border-white/8 bg-white/3 p-5"
						>
							<div className="grid gap-4 lg:grid-cols-[1.3fr,0.7fr,1.6fr,auto]">
								<label className="space-y-2">
									<SectionLabel title="Listen address" hint="Examples: :25565 or 127.0.0.1:8081" />
									<input
										value={listener.listen_addr}
										onChange={(event) => updateListener(index, "listen_addr", event.target.value)}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`listeners.${index}.listen_addr`]} />
								</label>
								<label className="space-y-2">
									<SectionLabel title="Protocol" hint="TCP routing or UDP forwarding" />
									<select
										value={listener.protocol}
										onChange={(event) => updateListener(index, "protocol", event.target.value)}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									>
										<option value="tcp">tcp</option>
										<option value="udp">udp</option>
									</select>
								</label>
								<label className="space-y-2">
									<SectionLabel title="Upstream" hint="Leave empty for TCP hostname-routing" />
									<input
										value={listener.upstream}
										onChange={(event) => updateListener(index, "upstream", event.target.value)}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`listeners.${index}.upstream`]} />
								</label>
								<div className="flex items-end">
									<button
										type="button"
										onClick={() =>
											setDraft((current) => ({
												...current,
												listeners: current.listeners.filter((_, i) => i !== index),
											}))
										}
										className="inline-flex items-center gap-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm font-medium text-red-200 transition hover:border-red-400/40 hover:bg-red-400/16"
									>
										<Trash2 className="h-4 w-4" />
										Remove
									</button>
								</div>
							</div>
						</div>
					))}

					<button
						type="button"
						onClick={addListener}
						className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10"
					>
						<Plus className="h-4 w-4" />
						Add listener
					</button>
				</div>
			</Card>

			<Card
				title="Routes"
				description="Model hostname-routing in structured form. The editor enforces the same basics the Rust config loader expects."
				icon={<Router className="h-5 w-5" />}
			>
				<div className="space-y-4">
					{draft.routes.map((route, index) => (
						<div
							key={`${route.hosts.join("-")}-${index}`}
							className="rounded-3xl border border-white/8 bg-white/3 p-5"
						>
							<div className="grid gap-4 xl:grid-cols-2">
								<label className="space-y-2">
									<SectionLabel title="Hosts" hint="One hostname or pattern per line" />
									<textarea
										value={route.hosts.join("\n")}
										onChange={(event) => updateRoute(index, "hosts", event.target.value)}
										rows={4}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`routes.${index}.hosts`]} />
								</label>
								<label className="space-y-2">
									<SectionLabel title="Upstreams" hint="One upstream per line" />
									<textarea
										value={route.upstreams.join("\n")}
										onChange={(event) => updateRoute(index, "upstreams", event.target.value)}
										rows={4}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`routes.${index}.upstreams`]} />
								</label>
								<label className="space-y-2">
									<SectionLabel title="Middlewares" hint="One middleware name per line" />
									<textarea
										value={route.middlewares.join("\n")}
										onChange={(event) => updateRoute(index, "middlewares", event.target.value)}
										rows={4}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`routes.${index}.middlewares`]} />
								</label>
								<div className="space-y-4">
									<label className="space-y-2">
										<SectionLabel
											title="Strategy"
											hint="Failover ordering for multi-upstream routes"
										/>
										<select
											value={route.strategy}
											onChange={(event) => updateRoute(index, "strategy", event.target.value)}
											className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
										>
											<option value="sequential">sequential</option>
											<option value="random">random</option>
											<option value="round-robin">round-robin</option>
										</select>
									</label>
									<button
										type="button"
										onClick={() =>
											setDraft((current) => ({
												...current,
												routes: current.routes.filter((_, i) => i !== index),
											}))
										}
										className="inline-flex items-center gap-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm font-medium text-red-200 transition hover:border-red-400/40 hover:bg-red-400/16"
									>
										<Trash2 className="h-4 w-4" />
										Remove route
									</button>
								</div>
							</div>
						</div>
					))}
					<button
						type="button"
						onClick={addRoute}
						className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10"
					>
						<CopyPlus className="h-4 w-4" />
						Add route
					</button>
				</div>
			</Card>

			<TunnelSection
				tunnel={draft.tunnel ?? null}
				issueMap={issueMap}
				onChange={(tunnel) => setDraft((current) => ({ ...current, tunnel }))}
			/>

			<Card
				title="Revision preview"
				description="The panel edits a structured managed document first. This raw preview is a transparency aid, not the primary editing surface."
				icon={<CheckCircle2 className="h-5 w-5" />}
			>
				<div className="rounded-3xl border border-white/8 bg-slate-950 p-4">
					<pre className="max-h-[24rem] overflow-auto whitespace-pre-wrap break-all text-sm leading-6 text-cyan-100/85">
						{JSON.stringify(normalizedDraft, null, 2)}
					</pre>
				</div>
			</Card>

			<div className="rounded-3xl border border-white/8 bg-slate-950/75 p-6">
				{saveError ? (
					<div className="mb-4 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm text-red-100">
						{saveError}
					</div>
				) : null}
				{issues.length > 0 ? (
					<div className="mb-4 rounded-2xl border border-amber-400/20 bg-amber-400/8 px-4 py-3 text-sm text-amber-100">
						Fix {issues.length} validation issue{issues.length === 1 ? "" : "s"} before saving this
						revision.
					</div>
				) : null}
				<button
					type="button"
					onClick={save}
					disabled={isSaving || issues.length > 0}
					className="inline-flex items-center gap-3 rounded-2xl bg-cyan-400 px-5 py-3 text-sm font-semibold text-slate-950 transition hover:bg-cyan-300 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
				>
					<Save className="h-4 w-4" />
					{isSaving ? "Saving revision…" : "Save managed revision"}
				</button>
			</div>
		</div>
	);
}

function TunnelSection({
	tunnel,
	issueMap,
	onChange,
}: {
	tunnel: ManagedTunnelDocument | null;
	issueMap: Record<string, string[]>;
	onChange: (tunnel: ManagedTunnelDocument | null | undefined) => void;
}) {
	if (!tunnel) {
		return (
			<section className="rounded-3xl border border-dashed border-white/10 bg-white/3 p-6">
				<div className="flex items-center justify-between">
					<div>
						<div className="flex items-center gap-3 text-white">
							<div className="rounded-2xl border border-cyan-400/25 bg-cyan-400/10 p-2 text-cyan-300">
								<Waypoints className="h-5 w-5" />
							</div>
							<h2 className="text-lg font-semibold">Tunnel</h2>
						</div>
						<p className="mt-2 max-w-3xl text-sm leading-6 text-slate-400">
							Enable reverse tunnel mode for this worker to register services or expose tunnel
							endpoints.
						</p>
					</div>
					<button
						type="button"
						onClick={() => onChange(createEmptyTunnel())}
						className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10"
					>
						<Plus className="h-4 w-4" />
						Enable tunnel
					</button>
				</div>
			</section>
		);
	}

	const updateTunnel = (partial: Partial<ManagedTunnelDocument>) => {
		onChange({ ...tunnel, ...partial });
	};

	return (
		<Card
			title="Tunnel"
			description="Configure reverse tunnel endpoints, client connectivity, and service registrations."
			icon={<Waypoints className="h-5 w-5" />}
		>
			<div className="space-y-6">
				<div className="flex items-end justify-between gap-4">
					<div className="grid flex-1 gap-4 md:grid-cols-2">
						<label className="space-y-2">
							<SectionLabel title="Auth token" hint="Shared secret for tunnel authentication" />
							<input
								type="password"
								value={tunnel.auth_token}
								onChange={(e) => updateTunnel({ auth_token: e.target.value })}
								className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
							/>
						</label>
						<label className="flex items-end gap-3 rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-sm text-slate-300">
							<input
								type="checkbox"
								checked={tunnel.auto_listen_services}
								onChange={(e) =>
									updateTunnel({
										auto_listen_services: e.target.checked,
									})
								}
								className="h-4 w-4 accent-cyan-400"
							/>
							Auto-listen for registered services with remote_addr
						</label>
					</div>
					<button
						type="button"
						onClick={() => onChange(undefined)}
						className="inline-flex items-center gap-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm font-medium text-red-200 transition hover:border-red-400/40 hover:bg-red-400/16"
					>
						<Trash2 className="h-4 w-4" />
						Disable tunnel
					</button>
				</div>

				{/* Endpoints */}
				<div className="space-y-3">
					<SectionLabel
						title="Endpoints"
						hint="Server-side tunnel listeners accepting client connections"
					/>
					{tunnel.endpoints.map((endpoint, index) => (
						<div
							key={`ep-${endpoint.listen_addr || index}`}
							className="rounded-3xl border border-white/8 bg-white/3 p-5"
						>
							<div className="grid gap-4 lg:grid-cols-[1.3fr,0.7fr,auto]">
								<label className="space-y-2">
									<SectionLabel title="Listen address" hint="e.g. :7000 or 0.0.0.0:7000" />
									<input
										value={endpoint.listen_addr}
										onChange={(e) => {
											const endpoints = [...tunnel.endpoints];
											endpoints[index] = {
												...endpoint,
												listen_addr: e.target.value,
											};
											updateTunnel({ endpoints });
										}}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
								</label>
								<label className="space-y-2">
									<SectionLabel title="Transport" hint="tcp, udp (KCP), or quic" />
									<select
										value={endpoint.transport}
										onChange={(e) => {
											const endpoints = [...tunnel.endpoints];
											endpoints[index] = {
												...endpoint,
												transport: e.target.value,
											};
											updateTunnel({ endpoints });
										}}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									>
										<option value="tcp">tcp</option>
										<option value="udp">udp (KCP)</option>
										<option value="quic">quic</option>
									</select>
								</label>
								<div className="flex items-end">
									<button
										type="button"
										onClick={() => {
											const endpoints = tunnel.endpoints.filter((_, i) => i !== index);
											updateTunnel({ endpoints });
										}}
										className="inline-flex items-center gap-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm font-medium text-red-200 transition hover:border-red-400/40 hover:bg-red-400/16"
									>
										<Trash2 className="h-4 w-4" />
										Remove
									</button>
								</div>
							</div>
							{endpoint.transport === "quic" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-2">
									<label className="space-y-2">
										<SectionLabel
											title="QUIC cert file"
											hint="Path to TLS certificate (auto-generated if empty)"
										/>
										<input
											value={endpoint.quic?.cert_file ?? ""}
											onChange={(e) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													quic: {
														...endpoint.quic,
														cert_file: e.target.value,
														key_file: endpoint.quic?.key_file ?? "",
													},
												};
												updateTunnel({ endpoints });
											}}
											className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
										/>
									</label>
									<label className="space-y-2">
										<SectionLabel title="QUIC key file" hint="Path to TLS private key" />
										<input
											value={endpoint.quic?.key_file ?? ""}
											onChange={(e) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													quic: {
														cert_file: endpoint.quic?.cert_file ?? "",
														key_file: e.target.value,
													},
												};
												updateTunnel({ endpoints });
											}}
											className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
										/>
									</label>
								</div>
							) : null}
						</div>
					))}
					<button
						type="button"
						onClick={() =>
							updateTunnel({
								endpoints: [...tunnel.endpoints, createEmptyTunnelEndpoint()],
							})
						}
						className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10"
					>
						<Plus className="h-4 w-4" />
						Add endpoint
					</button>
				</div>

				{/* Client */}
				<div className="space-y-3">
					<SectionLabel
						title="Client"
						hint="Outgoing tunnel connection to a management/server Prism node"
					/>
					{tunnel.client ? (
						<div className="rounded-3xl border border-white/8 bg-white/3 p-5">
							<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
								<label className="space-y-2">
									<SectionLabel title="Server address" hint="e.g. server.example.com:7000" />
									<input
										value={tunnel.client.server_addr}
										onChange={(e) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													server_addr: e.target.value,
												},
											})
										}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
								</label>
								<label className="space-y-2">
									<SectionLabel title="Transport" hint="tcp, udp (KCP), or quic" />
									<select
										value={tunnel.client.transport}
										onChange={(e) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													transport: e.target.value,
												},
											})
										}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									>
										<option value="tcp">tcp</option>
										<option value="udp">udp (KCP)</option>
										<option value="quic">quic</option>
									</select>
								</label>
								<label className="space-y-2">
									<SectionLabel
										title="Dial timeout (ms)"
										hint="Connection timeout for server dial"
									/>
									<input
										type="number"
										value={tunnel.client.dial_timeout_ms ?? 5000}
										onChange={(e) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													dial_timeout_ms: Number(e.target.value),
												},
											})
										}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
								</label>
								<div className="flex items-end">
									<button
										type="button"
										onClick={() => updateTunnel({ client: null })}
										className="inline-flex items-center gap-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm font-medium text-red-200 transition hover:border-red-400/40 hover:bg-red-400/16"
									>
										<Trash2 className="h-4 w-4" />
										Remove client
									</button>
								</div>
							</div>
							{tunnel.client.transport === "quic" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-2">
									<label className="space-y-2">
										<SectionLabel title="QUIC server name" hint="TLS SNI for QUIC handshake" />
										<input
											value={tunnel.client.quic?.server_name ?? ""}
											onChange={(e) =>
												tunnel.client &&
												updateTunnel({
													client: {
														...tunnel.client,
														quic: {
															server_name: e.target.value,
															insecure_skip_verify:
																tunnel.client.quic?.insecure_skip_verify ?? false,
														},
													},
												})
											}
											className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
										/>
									</label>
									<label className="flex items-end gap-3 rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-sm text-slate-300">
										<input
											type="checkbox"
											checked={tunnel.client.quic?.insecure_skip_verify ?? false}
											onChange={(e) =>
												tunnel.client &&
												updateTunnel({
													client: {
														...tunnel.client,
														quic: {
															server_name: tunnel.client.quic?.server_name ?? "",
															insecure_skip_verify: e.target.checked,
														},
													},
												})
											}
											className="h-4 w-4 accent-cyan-400"
										/>
										Skip TLS certificate verification
									</label>
								</div>
							) : null}
						</div>
					) : (
						<button
							type="button"
							onClick={() => updateTunnel({ client: createEmptyTunnelClient() })}
							className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10"
						>
							<Globe className="h-4 w-4" />
							Add client connection
						</button>
					)}
				</div>

				{/* Services */}
				<div className="space-y-3">
					<SectionLabel title="Services" hint="Services to register on the tunnel server" />
					{tunnel.services.map((service, index) => (
						<div
							key={`svc-${service.name || index}`}
							className="rounded-3xl border border-white/8 bg-white/3 p-5"
						>
							<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
								<label className="space-y-2">
									<SectionLabel title="Service name" hint="Unique identifier" />
									<input
										value={service.name}
										onChange={(e) => {
											const services = [...tunnel.services];
											services[index] = {
												...service,
												name: e.target.value,
											};
											updateTunnel({ services });
										}}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`tunnel.services.${index}.name`]} />
								</label>
								<label className="space-y-2">
									<SectionLabel title="Protocol" hint="tcp or udp" />
									<select
										value={service.proto}
										onChange={(e) => {
											const services = [...tunnel.services];
											services[index] = {
												...service,
												proto: e.target.value,
											};
											updateTunnel({ services });
										}}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									>
										<option value="tcp">tcp</option>
										<option value="udp">udp</option>
									</select>
								</label>
								<label className="space-y-2">
									<SectionLabel title="Local address" hint="Backend addr behind this tunnel" />
									<input
										value={service.local_addr}
										onChange={(e) => {
											const services = [...tunnel.services];
											services[index] = {
												...service,
												local_addr: e.target.value,
											};
											updateTunnel({ services });
										}}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`tunnel.services.${index}.local_addr`]} />
								</label>
								<label className="space-y-2">
									<SectionLabel
										title="Remote address"
										hint="Server-side bind addr for auto-listen"
									/>
									<input
										value={service.remote_addr}
										onChange={(e) => {
											const services = [...tunnel.services];
											services[index] = {
												...service,
												remote_addr: e.target.value,
											};
											updateTunnel({ services });
										}}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`tunnel.services.${index}.remote_addr`]} />
								</label>
								<label className="space-y-2">
									<SectionLabel
										title="Masquerade host"
										hint="Rewrite middleware label (optional)"
									/>
									<input
										value={service.masquerade_host}
										onChange={(e) => {
											const services = [...tunnel.services];
											services[index] = {
												...service,
												masquerade_host: e.target.value,
											};
											updateTunnel({ services });
										}}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
								</label>
								<div className="flex items-end gap-3">
									<label className="flex items-center gap-3 rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-sm text-slate-300">
										<input
											type="checkbox"
											checked={service.route_only}
											onChange={(e) => {
												const services = [...tunnel.services];
												services[index] = {
													...service,
													route_only: e.target.checked,
												};
												updateTunnel({ services });
											}}
											className="h-4 w-4 accent-cyan-400"
										/>
										Route only
									</label>
									<button
										type="button"
										onClick={() => {
											const services = tunnel.services.filter((_, i) => i !== index);
											updateTunnel({ services });
										}}
										className="inline-flex items-center gap-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm font-medium text-red-200 transition hover:border-red-400/40 hover:bg-red-400/16"
									>
										<Trash2 className="h-4 w-4" />
										Remove
									</button>
								</div>
							</div>
						</div>
					))}
					<button
						type="button"
						onClick={() =>
							updateTunnel({
								services: [...tunnel.services, createEmptyTunnelService()],
							})
						}
						className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10"
					>
						<CopyPlus className="h-4 w-4" />
						Add service
					</button>
				</div>
			</div>
		</Card>
	);
}
