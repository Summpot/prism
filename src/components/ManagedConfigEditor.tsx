import {
	ArrowDown,
	ArrowUp,
	Braces,
	Cable,
	CheckCircle2,
	ChevronDown,
	ChevronRight,
	Clock,
	CopyPlus,
	FolderSync,
	FormInput,
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
	createEmptyRoute,
	createEmptyTunnel,
	createEmptyTunnelClient,
	createEmptyTunnelEndpoint,
	createEmptyTunnelService,
	formatIssuesByPath,
	managedConfigFingerprint,
	moveItem,
	normalizeManagedConfig,
	parseManagedConfigJson,
	summarizeManagedConfig,
	validateManagedConfig,
} from "@/lib/managedConfig";
import type { ManagedConfigDocument, ManagedTunnelDocument } from "@/lib/managementApi";
import {
	DangerButton,
	Field,
	PrimaryButton,
	SecondaryButton,
	SectionCard,
	ToggleChip,
	fieldClassName,
} from "@/components/ui";

type EditorMode = "form" | "json";

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
	const [mode, setMode] = useState<EditorMode>("form");
	const [rawJson, setRawJson] = useState("");
	const [rawError, setRawError] = useState<string | null>(null);
	const [advancedOpen, setAdvancedOpen] = useState(false);
	const [baseline, setBaseline] = useState(
		managedConfigFingerprint(initialConfig ?? createEmptyManagedConfig()),
	);

	useEffect(() => {
		const next = initialConfig ?? createEmptyManagedConfig();
		setDraft(next);
		setBaseline(managedConfigFingerprint(next));
		setRawJson(JSON.stringify(normalizeManagedConfig(next), null, 2));
		setRawError(null);
	}, [initialConfig]);

	const normalizedDraft = useMemo(() => normalizeManagedConfig(draft), [draft]);
	const issues = useMemo(() => validateManagedConfig(draft), [draft]);
	const issueMap = useMemo(() => formatIssuesByPath(issues), [issues]);
	const summary = useMemo(() => summarizeManagedConfig(draft), [draft]);
	const dirty = managedConfigFingerprint(draft) !== baseline;

	const switchMode = (next: EditorMode) => {
		if (next === mode) {
			return;
		}
		if (next === "json") {
			setRawJson(JSON.stringify(normalizedDraft, null, 2));
			setRawError(null);
			setMode("json");
			return;
		}

		const parsed = parseManagedConfigJson(rawJson);
		if (!parsed.ok) {
			setRawError(parsed.error);
			return;
		}
		setDraft(parsed.value);
		setRawError(null);
		setMode("form");
	};

	const applyRawToDraft = () => {
		const parsed = parseManagedConfigJson(rawJson);
		if (!parsed.ok) {
			setRawError(parsed.error);
			return false;
		}
		setDraft(parsed.value);
		setRawError(null);
		return true;
	};

	const save = async () => {
		let document = draft;
		if (mode === "json") {
			const parsed = parseManagedConfigJson(rawJson);
			if (!parsed.ok) {
				setRawError(parsed.error);
				return;
			}
			document = parsed.value;
			setDraft(document);
		}

		const nextIssues = validateManagedConfig(document);
		if (nextIssues.length > 0) {
			if (mode === "json") {
				setRawError(nextIssues.map((issue) => `${issue.path}: ${issue.message}`).join("\n"));
			}
			return;
		}

		const normalized = normalizeManagedConfig(document);
		await onSave(normalized);
		setBaseline(managedConfigFingerprint(normalized));
		setDraft(normalized);
		setRawJson(JSON.stringify(normalized, null, 2));
	};

	const canSave =
		!isSaving &&
		(mode === "form" ? issues.length === 0 : !rawError) &&
		(mode === "form" ? dirty : true);

	return (
		<div className="space-y-6">
			<section className="rounded-3xl border border-white/8 bg-slate-950/75 p-5">
				<div className="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
					<div className="flex flex-wrap gap-2 text-sm">
						<SummaryPill label="Listeners" value={summary.listeners} />
						<SummaryPill label="Hostname routing" value={summary.hostnameRoutingListeners} />
						<SummaryPill label="Routes" value={summary.routes} />
						<SummaryPill
							label="Tunnel"
							value={
								summary.tunnelEnabled
									? `${summary.tunnelEndpoints} ep / ${summary.tunnelServices} svc`
									: "off"
							}
						/>
						{dirty ? (
							<span className="rounded-full bg-amber-400/12 px-3 py-1 text-xs font-semibold uppercase tracking-[0.2em] text-amber-100">
								Unsaved changes
							</span>
						) : (
							<span className="rounded-full bg-emerald-400/12 px-3 py-1 text-xs font-semibold uppercase tracking-[0.2em] text-emerald-100">
								Saved baseline
							</span>
						)}
					</div>
					<div className="flex flex-wrap gap-2">
						<ToggleChip active={mode === "form"} onClick={() => switchMode("form")}>
							<FormInput className="h-4 w-4" />
							Form
						</ToggleChip>
						<ToggleChip active={mode === "json"} onClick={() => switchMode("json")}>
							<Braces className="h-4 w-4" />
							JSON
						</ToggleChip>
					</div>
				</div>
				{issueMap.listeners?.length ? (
					<div className="mt-4 rounded-2xl border border-amber-400/20 bg-amber-400/8 px-4 py-3 text-sm text-amber-100">
						{issueMap.listeners.join(" ")}
					</div>
				) : null}
			</section>

			{mode === "json" ? (
				<SectionCard
					title="Raw managed document"
					description="Edit the structured managed config as JSON. Switching back to Form validates and loads the document."
					icon={<Braces className="h-5 w-5" />}
					actions={<SecondaryButton onClick={applyRawToDraft}>Apply to form model</SecondaryButton>}
				>
					<textarea
						value={rawJson}
						onChange={(event) => {
							setRawJson(event.target.value);
							setRawError(null);
						}}
						spellCheck={false}
						rows={28}
						className={`${fieldClassName} font-mono text-sm leading-6 text-cyan-100/90`}
					/>
					{rawError ? (
						<div className="mt-4 whitespace-pre-wrap rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm text-red-100">
							{rawError}
						</div>
					) : null}
				</SectionCard>
			) : (
				<>
					<SectionCard
						title="Listeners"
						description="Public entrypoints. Leave TCP upstream empty for hostname routing; UDP always needs a fixed upstream."
						icon={<Cable className="h-5 w-5" />}
					>
						<div className="space-y-4">
							{draft.listeners.map((listener, index) => (
								<div
									key={`listener-${index}`}
									className="rounded-3xl border border-white/8 bg-white/3 p-5"
								>
									<div className="grid gap-4 lg:grid-cols-[1.3fr,0.7fr,1.6fr,auto]">
										<Field
											title="Listen address"
											hint="Examples: :25565 or 127.0.0.1:8081"
											error={issueMap[`listeners.${index}.listen_addr`]}
										>
											<input
												value={listener.listen_addr}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														listeners: current.listeners.map((item, i) =>
															i === index ? { ...item, listen_addr: event.target.value } : item,
														),
													}))
												}
												className={fieldClassName}
											/>
										</Field>
										<Field
											title="Protocol"
											hint="TCP routing or UDP forwarding"
											error={issueMap[`listeners.${index}.protocol`]}
										>
											<select
												value={listener.protocol}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														listeners: current.listeners.map((item, i) =>
															i === index ? { ...item, protocol: event.target.value } : item,
														),
													}))
												}
												className={fieldClassName}
											>
												<option value="tcp">tcp</option>
												<option value="udp">udp</option>
											</select>
										</Field>
										<Field
											title="Upstream"
											hint="Empty TCP upstream enables hostname routing"
											error={issueMap[`listeners.${index}.upstream`]}
										>
											<input
												value={listener.upstream}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														listeners: current.listeners.map((item, i) =>
															i === index ? { ...item, upstream: event.target.value } : item,
														),
													}))
												}
												className={fieldClassName}
											/>
										</Field>
										<div className="flex items-end gap-2">
											<SecondaryButton
												onClick={() =>
													setDraft((current) => ({
														...current,
														listeners: [
															...current.listeners.slice(0, index + 1),
															{ ...listener },
															...current.listeners.slice(index + 1),
														],
													}))
												}
											>
												<CopyPlus className="h-4 w-4" />
												Duplicate
											</SecondaryButton>
											<DangerButton
												onClick={() =>
													setDraft((current) => ({
														...current,
														listeners: current.listeners.filter((_, i) => i !== index),
													}))
												}
											>
												<Trash2 className="h-4 w-4" />
												Remove
											</DangerButton>
										</div>
									</div>
								</div>
							))}
							<SecondaryButton
								onClick={() =>
									setDraft((current) => ({
										...current,
										listeners: [
											...current.listeners,
											{ listen_addr: "", protocol: "tcp", upstream: "" },
										],
									}))
								}
							>
								<Plus className="h-4 w-4" />
								Add listener
							</SecondaryButton>
						</div>
					</SectionCard>

					<SectionCard
						title="Routes"
						description="Ordered hostname matches. Match order is top-to-bottom; use the arrows to reorder."
						icon={<Router className="h-5 w-5" />}
					>
						<div className="space-y-4">
							{draft.routes.map((route, index) => (
								<div
									key={`route-${index}`}
									className="rounded-3xl border border-white/8 bg-white/3 p-5"
								>
									<div className="mb-4 flex flex-wrap items-center justify-between gap-3">
										<div className="text-sm font-medium text-white">Route #{index + 1}</div>
										<div className="flex flex-wrap gap-2">
											<SecondaryButton
												onClick={() =>
													setDraft((current) => ({
														...current,
														routes: moveItem(current.routes, index, index - 1),
													}))
												}
												disabled={index === 0}
											>
												<ArrowUp className="h-4 w-4" />
												Up
											</SecondaryButton>
											<SecondaryButton
												onClick={() =>
													setDraft((current) => ({
														...current,
														routes: moveItem(current.routes, index, index + 1),
													}))
												}
												disabled={index === draft.routes.length - 1}
											>
												<ArrowDown className="h-4 w-4" />
												Down
											</SecondaryButton>
											<SecondaryButton
												onClick={() =>
													setDraft((current) => ({
														...current,
														routes: [
															...current.routes.slice(0, index + 1),
															{
																hosts: [...route.hosts],
																upstreams: [...route.upstreams],
																middlewares: [...route.middlewares],
																strategy: route.strategy,
															},
															...current.routes.slice(index + 1),
														],
													}))
												}
											>
												<CopyPlus className="h-4 w-4" />
												Duplicate
											</SecondaryButton>
											<DangerButton
												onClick={() =>
													setDraft((current) => ({
														...current,
														routes: current.routes.filter((_, i) => i !== index),
													}))
												}
											>
												<Trash2 className="h-4 w-4" />
												Remove
											</DangerButton>
										</div>
									</div>
									<div className="grid gap-4 xl:grid-cols-2">
										<Field
											title="Hosts"
											hint="One hostname or pattern per line"
											error={issueMap[`routes.${index}.hosts`]}
										>
											<textarea
												value={route.hosts.join("\n")}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														routes: current.routes.map((item, i) =>
															i === index
																? {
																		...item,
																		hosts: event.target.value
																			.split("\n")
																			.map((entry) => entry.trim())
																			.filter(Boolean),
																	}
																: item,
														),
													}))
												}
												rows={4}
												className={fieldClassName}
											/>
										</Field>
										<Field
											title="Upstreams"
											hint="One upstream per line (host:port or tunnel:name)"
											error={issueMap[`routes.${index}.upstreams`]}
										>
											<textarea
												value={route.upstreams.join("\n")}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														routes: current.routes.map((item, i) =>
															i === index
																? {
																		...item,
																		upstreams: event.target.value
																			.split("\n")
																			.map((entry) => entry.trim())
																			.filter(Boolean),
																	}
																: item,
														),
													}))
												}
												rows={4}
												className={fieldClassName}
											/>
										</Field>
										<Field
											title="Middlewares"
											hint="One middleware name per line"
											error={issueMap[`routes.${index}.middlewares`]}
										>
											<textarea
												value={route.middlewares.join("\n")}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														routes: current.routes.map((item, i) =>
															i === index
																? {
																		...item,
																		middlewares: event.target.value
																			.split("\n")
																			.map((entry) => entry.trim())
																			.filter(Boolean),
																	}
																: item,
														),
													}))
												}
												rows={4}
												className={fieldClassName}
											/>
										</Field>
										<Field
											title="Strategy"
											hint="Load balancing when multiple upstreams are set"
											error={issueMap[`routes.${index}.strategy`]}
										>
											<select
												value={route.strategy}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														routes: current.routes.map((item, i) =>
															i === index ? { ...item, strategy: event.target.value } : item,
														),
													}))
												}
												className={fieldClassName}
											>
												<option value="sequential">sequential</option>
												<option value="random">random</option>
												<option value="round-robin">round-robin</option>
											</select>
										</Field>
									</div>
								</div>
							))}
							<SecondaryButton
								onClick={() =>
									setDraft((current) => ({
										...current,
										routes: [...current.routes, createEmptyRoute()],
									}))
								}
							>
								<Plus className="h-4 w-4" />
								Add route
							</SecondaryButton>
						</div>
					</SectionCard>

					<TunnelSection
						tunnel={draft.tunnel ?? null}
						issueMap={issueMap}
						onChange={(tunnel) => setDraft((current) => ({ ...current, tunnel }))}
					/>

					<section className="rounded-3xl border border-white/8 bg-slate-950/75">
						<button
							type="button"
							onClick={() => setAdvancedOpen((value) => !value)}
							className="flex w-full items-center justify-between gap-4 px-6 py-5 text-left"
						>
							<div className="flex items-center gap-3">
								<div className="rounded-2xl border border-cyan-400/25 bg-cyan-400/10 p-2 text-cyan-300">
									<FolderSync className="h-5 w-5" />
								</div>
								<div>
									<div className="text-lg font-semibold text-white">Advanced runtime</div>
									<div className="mt-1 text-sm text-slate-400">
										Buffer sizes, dial timeouts, PROXY protocol, handshake and idle limits.
									</div>
								</div>
							</div>
							{advancedOpen ? (
								<ChevronDown className="h-5 w-5 text-slate-400" />
							) : (
								<ChevronRight className="h-5 w-5 text-slate-400" />
							)}
						</button>
						{advancedOpen ? (
							<div className="space-y-6 border-t border-white/8 px-6 py-6">
								<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
									<Field title="Max header bytes" hint="TCP routing prelude cap">
										<input
											type="number"
											value={draft.max_header_bytes}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													max_header_bytes: Number(event.target.value),
												}))
											}
											className={fieldClassName}
										/>
									</Field>
									<Field title="Buffer size" hint="Copy buffer hint (bytes)">
										<input
											type="number"
											value={draft.buffer_size}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													buffer_size: Number(event.target.value),
												}))
											}
											className={fieldClassName}
										/>
									</Field>
									<Field title="Dial timeout" hint="Upstream timeout in ms">
										<input
											type="number"
											value={draft.upstream_dial_timeout_ms}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													upstream_dial_timeout_ms: Number(event.target.value),
												}))
											}
											className={fieldClassName}
										/>
									</Field>
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
										Inject PROXY protocol v2
									</label>
								</div>
								<div className="grid gap-4 md:grid-cols-2">
									<Field title="Handshake timeout (ms)" hint="TCP prelude capture timeout">
										<input
											type="number"
											value={draft.timeouts?.handshake_timeout_ms ?? 0}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													timeouts: {
														handshake_timeout_ms: Number(event.target.value),
														idle_timeout_ms: current.timeouts?.idle_timeout_ms ?? 0,
													},
												}))
											}
											className={fieldClassName}
										/>
									</Field>
									<Field
										title="Idle timeout (ms)"
										hint="Bidirectional copy lifetime cap (0 = no limit)"
									>
										<input
											type="number"
											value={draft.timeouts?.idle_timeout_ms ?? 0}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													timeouts: {
														handshake_timeout_ms: current.timeouts?.handshake_timeout_ms ?? 0,
														idle_timeout_ms: Number(event.target.value),
													},
												}))
											}
											className={fieldClassName}
										/>
									</Field>
								</div>
								<div className="flex items-center gap-2 text-sm text-slate-400">
									<Clock className="h-4 w-4 text-cyan-300" />
									These knobs are hot-reload safe for managed workers when the revision applies.
								</div>
							</div>
						) : null}
					</section>

					<SectionCard
						title="Revision preview"
						description="Normalized document that will be submitted on save."
						icon={<CheckCircle2 className="h-5 w-5" />}
					>
						<pre className="max-h-[20rem] overflow-auto rounded-3xl border border-white/8 bg-slate-950 p-4 text-sm leading-6 whitespace-pre-wrap break-all text-cyan-100/85">
							{JSON.stringify(normalizedDraft, null, 2)}
						</pre>
					</SectionCard>
				</>
			)}

			<div className="rounded-3xl border border-white/8 bg-slate-950/75 p-6">
				{saveError ? (
					<div className="mb-4 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm text-red-100">
						{saveError}
					</div>
				) : null}
				{mode === "form" && issues.length > 0 ? (
					<div className="mb-4 rounded-2xl border border-amber-400/20 bg-amber-400/8 px-4 py-3 text-sm text-amber-100">
						Fix {issues.length} validation issue{issues.length === 1 ? "" : "s"} before saving.
						<ul className="mt-2 list-disc space-y-1 pl-5">
							{issues.slice(0, 8).map((issue) => (
								<li key={`${issue.path}-${issue.message}`}>
									<span className="font-mono text-amber-50/90">{issue.path}</span>: {issue.message}
								</li>
							))}
						</ul>
					</div>
				) : null}
				<div className="flex flex-wrap items-center gap-3">
					<PrimaryButton onClick={save} disabled={!canSave}>
						<Save className="h-4 w-4" />
						{isSaving ? "Saving revision…" : dirty ? "Save managed revision" : "Save revision"}
					</PrimaryButton>
					{dirty ? (
						<SecondaryButton
							onClick={() => {
								const next = initialConfig ?? createEmptyManagedConfig();
								setDraft(next);
								setRawJson(JSON.stringify(normalizeManagedConfig(next), null, 2));
								setRawError(null);
							}}
							disabled={isSaving}
						>
							Discard changes
						</SecondaryButton>
					) : null}
				</div>
			</div>
		</div>
	);
}

function SummaryPill({ label, value }: { label: string; value: string | number }) {
	return (
		<span className="rounded-full border border-white/10 bg-white/4 px-3 py-1 text-slate-300">
			<span className="text-slate-500">{label}</span>{" "}
			<span className="font-medium text-white">{value}</span>
		</span>
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
				<div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
					<div>
						<div className="flex items-center gap-3 text-white">
							<div className="rounded-2xl border border-cyan-400/25 bg-cyan-400/10 p-2 text-cyan-300">
								<Waypoints className="h-5 w-5" />
							</div>
							<h2 className="text-lg font-semibold">Tunnel</h2>
						</div>
						<p className="mt-2 max-w-3xl text-sm leading-6 text-slate-400">
							Enable reverse tunnel mode for endpoints, client connectivity, and service
							registrations.
						</p>
					</div>
					<SecondaryButton onClick={() => onChange(createEmptyTunnel())}>
						<Plus className="h-4 w-4" />
						Enable tunnel
					</SecondaryButton>
				</div>
			</section>
		);
	}

	const updateTunnel = (partial: Partial<ManagedTunnelDocument>) => {
		onChange({ ...tunnel, ...partial });
	};

	return (
		<SectionCard
			title="Tunnel"
			description="Reverse tunnel endpoints, client dial settings, and registered services."
			icon={<Waypoints className="h-5 w-5" />}
			actions={
				<DangerButton onClick={() => onChange(undefined)}>
					<Trash2 className="h-4 w-4" />
					Disable tunnel
				</DangerButton>
			}
		>
			<div className="space-y-6">
				<div className="grid gap-4 md:grid-cols-2">
					<Field title="Auth token" hint="Shared secret for tunnel authentication">
						<input
							type="password"
							value={tunnel.auth_token}
							onChange={(event) => updateTunnel({ auth_token: event.target.value })}
							className={fieldClassName}
						/>
					</Field>
					<label className="flex items-end gap-3 rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-sm text-slate-300">
						<input
							type="checkbox"
							checked={tunnel.auto_listen_services}
							onChange={(event) =>
								updateTunnel({
									auto_listen_services: event.target.checked,
								})
							}
							className="h-4 w-4 accent-cyan-400"
						/>
						Auto-listen services that set remote_addr
					</label>
				</div>

				<div className="space-y-3">
					<div className="text-sm font-medium text-white">Endpoints</div>
					<div className="text-xs leading-5 text-slate-500">
						Server-side tunnel listeners that accept client connections.
					</div>
					{tunnel.endpoints.map((endpoint, index) => (
						<div
							key={`endpoint-${index}`}
							className="rounded-3xl border border-white/8 bg-white/3 p-5"
						>
							<div className="grid gap-4 lg:grid-cols-[1.3fr,0.7fr,auto]">
								<Field
									title="Listen address"
									hint="e.g. :7000"
									error={issueMap[`tunnel.endpoints.${index}.listen_addr`]}
								>
									<input
										value={endpoint.listen_addr}
										onChange={(event) => {
											const endpoints = [...tunnel.endpoints];
											endpoints[index] = {
												...endpoint,
												listen_addr: event.target.value,
											};
											updateTunnel({ endpoints });
										}}
										className={fieldClassName}
									/>
								</Field>
								<Field
									title="Transport"
									hint="tcp, udp (KCP), or quic"
									error={issueMap[`tunnel.endpoints.${index}.transport`]}
								>
									<select
										value={endpoint.transport}
										onChange={(event) => {
											const endpoints = [...tunnel.endpoints];
											endpoints[index] = {
												...endpoint,
												transport: event.target.value,
											};
											updateTunnel({ endpoints });
										}}
										className={fieldClassName}
									>
										<option value="tcp">tcp</option>
										<option value="udp">udp (KCP)</option>
										<option value="quic">quic</option>
									</select>
								</Field>
								<div className="flex items-end">
									<DangerButton
										onClick={() =>
											updateTunnel({
												endpoints: tunnel.endpoints.filter((_, i) => i !== index),
											})
										}
									>
										<Trash2 className="h-4 w-4" />
										Remove
									</DangerButton>
								</div>
							</div>
							{endpoint.transport === "quic" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-2">
									<Field title="QUIC cert file" hint="Empty = auto self-signed at startup">
										<input
											value={endpoint.quic?.cert_file ?? ""}
											onChange={(event) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													quic: {
														cert_file: event.target.value,
														key_file: endpoint.quic?.key_file ?? "",
													},
												};
												updateTunnel({ endpoints });
											}}
											className={fieldClassName}
										/>
									</Field>
									<Field title="QUIC key file" hint="TLS private key path">
										<input
											value={endpoint.quic?.key_file ?? ""}
											onChange={(event) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													quic: {
														cert_file: endpoint.quic?.cert_file ?? "",
														key_file: event.target.value,
													},
												};
												updateTunnel({ endpoints });
											}}
											className={fieldClassName}
										/>
									</Field>
								</div>
							) : null}
						</div>
					))}
					<SecondaryButton
						onClick={() =>
							updateTunnel({
								endpoints: [...tunnel.endpoints, createEmptyTunnelEndpoint()],
							})
						}
					>
						<Plus className="h-4 w-4" />
						Add endpoint
					</SecondaryButton>
				</div>

				<div className="space-y-3">
					<div className="text-sm font-medium text-white">Client</div>
					<div className="text-xs leading-5 text-slate-500">
						Outgoing tunnel connection to a Prism tunnel server.
					</div>
					{tunnel.client ? (
						<div className="rounded-3xl border border-white/8 bg-white/3 p-5">
							<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
								<Field
									title="Server address"
									hint="e.g. server.example.com:7000"
									error={issueMap["tunnel.client.server_addr"]}
								>
									<input
										value={tunnel.client.server_addr}
										onChange={(event) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													server_addr: event.target.value,
												},
											})
										}
										className={fieldClassName}
									/>
								</Field>
								<Field
									title="Transport"
									hint="tcp, udp (KCP), or quic"
									error={issueMap["tunnel.client.transport"]}
								>
									<select
										value={tunnel.client.transport}
										onChange={(event) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													transport: event.target.value,
												},
											})
										}
										className={fieldClassName}
									>
										<option value="tcp">tcp</option>
										<option value="udp">udp (KCP)</option>
										<option value="quic">quic</option>
									</select>
								</Field>
								<Field title="Dial timeout (ms)" hint="Connection timeout">
									<input
										type="number"
										value={tunnel.client.dial_timeout_ms ?? 5000}
										onChange={(event) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													dial_timeout_ms: Number(event.target.value),
												},
											})
										}
										className={fieldClassName}
									/>
								</Field>
								<div className="flex items-end">
									<DangerButton onClick={() => updateTunnel({ client: null })}>
										<Trash2 className="h-4 w-4" />
										Remove client
									</DangerButton>
								</div>
							</div>
							{tunnel.client.transport === "quic" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-2">
									<Field title="QUIC server name" hint="TLS SNI">
										<input
											value={tunnel.client.quic?.server_name ?? ""}
											onChange={(event) =>
												tunnel.client &&
												updateTunnel({
													client: {
														...tunnel.client,
														quic: {
															server_name: event.target.value,
															insecure_skip_verify:
																tunnel.client.quic?.insecure_skip_verify ?? false,
														},
													},
												})
											}
											className={fieldClassName}
										/>
									</Field>
									<label className="flex items-end gap-3 rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-sm text-slate-300">
										<input
											type="checkbox"
											checked={tunnel.client.quic?.insecure_skip_verify ?? false}
											onChange={(event) =>
												tunnel.client &&
												updateTunnel({
													client: {
														...tunnel.client,
														quic: {
															server_name: tunnel.client.quic?.server_name ?? "",
															insecure_skip_verify: event.target.checked,
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
						<SecondaryButton onClick={() => updateTunnel({ client: createEmptyTunnelClient() })}>
							<Globe className="h-4 w-4" />
							Add client connection
						</SecondaryButton>
					)}
				</div>

				<div className="space-y-3">
					<div className="text-sm font-medium text-white">Services</div>
					<div className="text-xs leading-5 text-slate-500">
						Services registered through the tunnel. Use route_only for tunnel:name upstreams only.
					</div>
					{tunnel.services.map((service, index) => (
						<div
							key={`service-${index}`}
							className="rounded-3xl border border-white/8 bg-white/3 p-5"
						>
							<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
								<Field
									title="Service name"
									hint="Unique identifier"
									error={issueMap[`tunnel.services.${index}.name`]}
								>
									<input
										value={service.name}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = { ...service, name: event.target.value };
											updateTunnel({ services });
										}}
										className={fieldClassName}
									/>
								</Field>
								<Field
									title="Protocol"
									hint="tcp or udp"
									error={issueMap[`tunnel.services.${index}.proto`]}
								>
									<select
										value={service.proto}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = { ...service, proto: event.target.value };
											updateTunnel({ services });
										}}
										className={fieldClassName}
									>
										<option value="tcp">tcp</option>
										<option value="udp">udp</option>
									</select>
								</Field>
								<Field
									title="Local address"
									hint="Backend behind this tunnel"
									error={issueMap[`tunnel.services.${index}.local_addr`]}
								>
									<input
										value={service.local_addr}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = { ...service, local_addr: event.target.value };
											updateTunnel({ services });
										}}
										className={fieldClassName}
									/>
								</Field>
								<Field
									title="Remote address"
									hint="Server-side bind for auto-listen"
									error={issueMap[`tunnel.services.${index}.remote_addr`]}
								>
									<input
										value={service.remote_addr}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = { ...service, remote_addr: event.target.value };
											updateTunnel({ services });
										}}
										className={fieldClassName}
									/>
								</Field>
								<Field title="Masquerade host" hint="Optional rewrite label">
									<input
										value={service.masquerade_host}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = {
												...service,
												masquerade_host: event.target.value,
											};
											updateTunnel({ services });
										}}
										className={fieldClassName}
									/>
								</Field>
								<div className="flex items-end gap-3">
									<label className="flex items-center gap-3 rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-sm text-slate-300">
										<input
											type="checkbox"
											checked={service.route_only}
											onChange={(event) => {
												const services = [...tunnel.services];
												services[index] = {
													...service,
													route_only: event.target.checked,
												};
												updateTunnel({ services });
											}}
											className="h-4 w-4 accent-cyan-400"
										/>
										Route only
									</label>
									<DangerButton
										onClick={() =>
											updateTunnel({
												services: tunnel.services.filter((_, i) => i !== index),
											})
										}
									>
										<Trash2 className="h-4 w-4" />
										Remove
									</DangerButton>
								</div>
							</div>
						</div>
					))}
					<div className="flex flex-wrap gap-2">
						<SecondaryButton
							onClick={() =>
								updateTunnel({
									services: [...tunnel.services, createEmptyTunnelService()],
								})
							}
						>
							<Plus className="h-4 w-4" />
							Add service
						</SecondaryButton>
					</div>
				</div>
			</div>
		</SectionCard>
	);
}
