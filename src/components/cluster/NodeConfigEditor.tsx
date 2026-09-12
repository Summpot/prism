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
	ResultBanner,
	SecondaryButton,
	SectionCard,
	SwitchRow,
	ToggleChip,
	WarningBanner,
} from "@/components/ui";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Textarea } from "@/components/ui/textarea";
import { m } from "@/paraglide/messages";

type EditorMode = "form" | "json";

export interface NodeConfigEditorProps {
	initialConfig?: ManagedConfigDocument | null;
	isSaving: boolean;
	saveError?: string | null;
	onSave: (value: ManagedConfigDocument) => Promise<void>;
}

export type ManagedConfigEditorProps = NodeConfigEditorProps;

export function NodeConfigEditor({
	initialConfig,
	isSaving,
	saveError,
	onSave,
}: NodeConfigEditorProps) {
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
			<Card className="shadow-xs">
				<CardContent className="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
					<div className="flex flex-wrap gap-2 text-sm">
						<SummaryPill label={m.nodecfg_listeners()} value={summary.listeners} />
						<SummaryPill label={m.nodecfg_hostname_routing()} value={summary.hostnameRoutingListeners} />
						<SummaryPill label={m.nodecfg_routes()} value={summary.routes} />
						<SummaryPill
							label={m.nodecfg_tunnel()}
							value={
								summary.tunnelEnabled
									? m.nodecfg_tunnel_summary({ endpoints: summary.tunnelEndpoints, services: summary.tunnelServices })
									: m.admin_off()
							}
						/>
						{dirty ? (
							<Badge
								variant="outline"
								className="bg-amber-500/15 text-amber-700 dark:text-amber-300 border-amber-500/20"
							>
								{m.nodecfg_unsaved()}
							</Badge>
						) : (
							<Badge
								variant="outline"
								className="bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-500/20"
							>
								{m.nodecfg_saved_baseline()}
							</Badge>
						)}
					</div>
					<div className="flex flex-wrap gap-2">
						<ToggleChip active={mode === "form"} onClick={() => switchMode("form")}>
							<FormInput className="h-4 w-4" />
							{m.nodecfg_mode_form()}
						</ToggleChip>
						<ToggleChip active={mode === "json"} onClick={() => switchMode("json")}>
							<Braces className="h-4 w-4" />
							{m.nodecfg_mode_json()}
						</ToggleChip>
					</div>
				</CardContent>
				{issueMap.listeners?.length ? (
					<CardContent className="pt-0">
						<WarningBanner>{issueMap.listeners.join(" ")}</WarningBanner>
					</CardContent>
				) : null}
			</Card>

			{mode === "json" ? (
				<SectionCard
					title={m.nodecfg_raw_title()}
					description={m.nodecfg_raw_description()}
					icon={<Braces className="h-5 w-5" />}
					actions={<SecondaryButton onClick={applyRawToDraft}>{m.nodecfg_apply_to_form()}</SecondaryButton>}
				>
					<Textarea
						value={rawJson}
						onChange={(event) => {
							setRawJson(event.target.value);
							setRawError(null);
						}}
						spellCheck={false}
						rows={28}
						className="min-h-96 font-mono text-sm leading-6"
					/>
					{rawError ? (
						<div className="mt-4">
							<ResultBanner ok={false}>{rawError}</ResultBanner>
						</div>
					) : null}
				</SectionCard>
			) : (
				<>
					<SectionCard
						title={m.nodecfg_listeners()}
						description={m.nodecfg_listeners_description()}
						icon={<Cable className="h-5 w-5" />}
					>
						<div className="space-y-4">
							{draft.listeners.map((listener, index) => (
								<div
									key={`listener-${index}`}
									className="rounded-xl border border-border bg-muted/20 p-4"
								>
									<div className="grid gap-4 lg:grid-cols-[1.3fr,0.7fr,1.6fr,auto]">
										<Field
											title={m.nodecfg_listen_address()}
											hint={m.nodecfg_listen_address_hint()}
											error={issueMap[`listeners.${index}.listen_addr`]}
										>
											<Input
												value={listener.listen_addr}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														listeners: current.listeners.map((item, i) =>
															i === index ? { ...item, listen_addr: event.target.value } : item,
														),
													}))
												}
											/>
										</Field>
										<Field
											title={m.nodecfg_protocol()}
											hint={m.nodecfg_protocol_hint()}
											error={issueMap[`listeners.${index}.protocol`]}
										>
											<NativeSelect
												value={listener.protocol}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														listeners: current.listeners.map((item, i) =>
															i === index ? { ...item, protocol: event.target.value } : item,
														),
													}))
												}
												className="w-full"
											>
												<NativeSelectOption value="tcp">{m.transport_tcp()}</NativeSelectOption>
												<NativeSelectOption value="udp">{m.transport_udp()}</NativeSelectOption>
											</NativeSelect>
										</Field>
										<Field
											title={m.nodecfg_upstream()}
											hint={m.nodecfg_upstream_hint()}
											error={issueMap[`listeners.${index}.upstream`]}
										>
											<Input
												value={listener.upstream}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														listeners: current.listeners.map((item, i) =>
															i === index ? { ...item, upstream: event.target.value } : item,
														),
													}))
												}
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
												{m.nodecfg_duplicate()}
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
												{m.nodecfg_remove()}
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
								{m.nodecfg_add_listener()}
							</SecondaryButton>
						</div>
					</SectionCard>

					<SectionCard
						title={m.nodecfg_routes()}
						description={m.nodecfg_routes_description()}
						icon={<Router className="h-5 w-5" />}
					>
						<div className="space-y-4">
							{draft.routes.map((route, index) => (
								<div
									key={`route-${index}`}
									className="rounded-xl border border-border bg-muted/20 p-4"
								>
									<div className="mb-4 flex flex-wrap items-center justify-between gap-3">
										<div className="text-sm font-medium text-foreground">{m.nodecfg_route_number({ index: index + 1 })}</div>
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
												{m.nodecfg_up()}
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
												{m.nodecfg_down()}
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
												{m.nodecfg_duplicate()}
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
												{m.nodecfg_remove()}
											</DangerButton>
										</div>
									</div>
									<div className="grid gap-4 xl:grid-cols-2">
										<Field
											title={m.nodecfg_hosts()}
											hint={m.nodecfg_hosts_hint()}
											error={issueMap[`routes.${index}.hosts`]}
										>
											<Textarea
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
											/>
										</Field>
										<Field
											title={m.nodecfg_upstreams()}
											hint={m.nodecfg_upstreams_hint()}
											error={issueMap[`routes.${index}.upstreams`]}
										>
											<Textarea
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
											/>
										</Field>
										<Field
											title={m.nodecfg_middlewares()}
											hint={m.nodecfg_middlewares_hint()}
											error={issueMap[`routes.${index}.middlewares`]}
										>
											<Textarea
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
											/>
										</Field>
										<Field
											title={m.nodecfg_strategy()}
											hint={m.nodecfg_strategy_hint()}
											error={issueMap[`routes.${index}.strategy`]}
										>
											<NativeSelect
												value={route.strategy}
												onChange={(event) =>
													setDraft((current) => ({
														...current,
														routes: current.routes.map((item, i) =>
															i === index ? { ...item, strategy: event.target.value } : item,
														),
													}))
												}
												className="w-full"
											>
												<NativeSelectOption value="sequential">{m.strategy_sequential()}</NativeSelectOption>
												<NativeSelectOption value="random">{m.strategy_random()}</NativeSelectOption>
												<NativeSelectOption value="round-robin">{m.strategy_round_robin()}</NativeSelectOption>
											</NativeSelect>
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
								{m.nodecfg_add_route()}
							</SecondaryButton>
						</div>
					</SectionCard>

					<TunnelSection
						tunnel={draft.tunnel ?? null}
						issueMap={issueMap}
						onChange={(tunnel) => setDraft((current) => ({ ...current, tunnel }))}
					/>

					<Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen}>
						<Card className="shadow-xs">
							<CollapsibleTrigger className="flex w-full items-center justify-between gap-4 px-6 py-5 text-left">
								<div className="flex items-center gap-3">
									<div className="rounded-lg bg-primary/10 p-2 text-primary ring-1 ring-primary/20">
										<FolderSync className="h-5 w-5" />
									</div>
									<div>
										<div className="text-base font-semibold text-foreground">{m.nodecfg_advanced_runtime()}</div>
										<div className="mt-1 text-sm text-muted-foreground">
											{m.nodecfg_advanced_runtime_description()}
										</div>
									</div>
								</div>
								{advancedOpen ? (
									<ChevronDown className="h-5 w-5 text-muted-foreground" />
								) : (
									<ChevronRight className="h-5 w-5 text-muted-foreground" />
								)}
							</CollapsibleTrigger>
							<CollapsibleContent>
							<div className="space-y-6 border-t border-border px-6 py-6">
								<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
									<Field title={m.nodecfg_max_header_bytes()} hint={m.nodecfg_max_header_bytes_hint()}>
										<Input
											type="number"
											value={draft.max_header_bytes}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													max_header_bytes: Number(event.target.value),
												}))
											}
										/>
									</Field>
									<Field title={m.nodecfg_buffer_size()} hint={m.nodecfg_buffer_size_hint()}>
										<Input
											type="number"
											value={draft.buffer_size}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													buffer_size: Number(event.target.value),
												}))
											}
										/>
									</Field>
									<Field title={m.nodecfg_dial_timeout()} hint={m.nodecfg_dial_timeout_hint()}>
										<Input
											type="number"
											value={draft.upstream_dial_timeout_ms}
											onChange={(event) =>
												setDraft((current) => ({
													...current,
													upstream_dial_timeout_ms: Number(event.target.value),
												}))
											}
										/>
									</Field>
									<SwitchRow
										checked={draft.proxy_protocol_v2}
										onCheckedChange={(checked) =>
											setDraft((current) => ({
												...current,
												proxy_protocol_v2: checked,
											}))
										}
									>
										{m.nodecfg_proxy_protocol()}
									</SwitchRow>
								</div>
								<div className="grid gap-4 md:grid-cols-2">
									<Field title={m.nodecfg_handshake_timeout()} hint={m.nodecfg_handshake_timeout_hint()}>
										<Input
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
										/>
									</Field>
									<Field
										title={m.nodecfg_idle_timeout()}
										hint={m.nodecfg_idle_timeout_hint()}
									>
										<Input
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
										/>
									</Field>
								</div>
								<div className="flex items-center gap-2 text-sm text-muted-foreground">
									<Clock className="h-4 w-4 text-primary" />
									{m.nodecfg_hot_reload_note()}
								</div>
							</div>
							</CollapsibleContent>
						</Card>
					</Collapsible>

					<SectionCard
						title={m.nodecfg_revision_preview()}
						description={m.nodecfg_revision_preview_description()}
						icon={<CheckCircle2 className="h-5 w-5" />}
					>
						<pre className="max-h-[20rem] overflow-auto rounded-xl border border-border bg-muted/30 p-4 font-mono text-sm leading-6 whitespace-pre-wrap break-all text-foreground">
							{JSON.stringify(normalizedDraft, null, 2)}
						</pre>
					</SectionCard>
				</>
			)}

			<Card className="shadow-xs">
				<CardContent className="space-y-4">
				{saveError ? <ResultBanner ok={false}>{saveError}</ResultBanner> : null}
				{mode === "form" && issues.length > 0 ? (
					<WarningBanner
						title={
							issues.length === 1
								? m.nodecfg_fix_issue({ count: issues.length })
								: m.nodecfg_fix_issues({ count: issues.length })
						}
					>
						<ul className="mt-2 list-disc space-y-1 pl-5">
							{issues.slice(0, 8).map((issue) => (
								<li key={`${issue.path}-${issue.message}`}>
									<span className="font-mono">{issue.path}</span>: {issue.message}
								</li>
							))}
						</ul>
					</WarningBanner>
				) : null}
				<div className="flex flex-wrap items-center gap-3">
					<PrimaryButton onClick={save} disabled={!canSave}>
						<Save className="h-4 w-4" />
						{isSaving ? m.nodecfg_saving_revision() : dirty ? m.nodecfg_save_managed_revision() : m.nodecfg_save_revision()}
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
							{m.nodecfg_discard_changes()}
						</SecondaryButton>
					) : null}
				</div>
				</CardContent>
			</Card>
		</div>
	);
}

function SummaryPill({ label, value }: { label: string; value: string | number }) {
	return (
		<span className="rounded-full border border-border bg-muted/40 px-3 py-1 text-sm text-muted-foreground">
			<span>{label}</span>{" "}
			<span className="font-medium text-foreground">{value}</span>
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
			<section className="rounded-xl border border-dashed border-border bg-muted/20 p-6">
				<div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
					<div>
						<div className="flex items-center gap-3 text-foreground">
							<div className="rounded-lg bg-primary/10 p-2 text-primary ring-1 ring-primary/20">
								<Waypoints className="h-5 w-5" />
							</div>
							<h2 className="text-lg font-semibold">{m.nodecfg_tunnel()}</h2>
						</div>
						<p className="mt-2 max-w-3xl text-sm leading-6 text-muted-foreground">{m.nodecfg_tunnel_enable_hint()}</p>
					</div>
					<SecondaryButton onClick={() => onChange(createEmptyTunnel())}>
						<Plus className="h-4 w-4" />
						{m.nodecfg_tunnel_enable()}
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
			title={m.nodecfg_tunnel()}
			description={m.nodecfg_tunnel_description()}
			icon={<Waypoints className="h-5 w-5" />}
			actions={
				<DangerButton onClick={() => onChange(undefined)}>
					<Trash2 className="h-4 w-4" />
					{m.nodecfg_tunnel_disable()}
				</DangerButton>
			}
		>
			<div className="space-y-6">
				<div className="grid gap-4 md:grid-cols-2">
					<Field title={m.nodecfg_auth_token()} hint={m.nodecfg_auth_token_hint()}>
						<Input
							type="password"
							value={tunnel.auth_token}
							onChange={(event) => updateTunnel({ auth_token: event.target.value })}
						/>
					</Field>
					<SwitchRow
						checked={tunnel.auto_listen_services}
						onCheckedChange={(checked) =>
							updateTunnel({
								auto_listen_services: checked,
							})
						}
					>
						{m.nodecfg_auto_listen()}
					</SwitchRow>
				</div>

				<div className="space-y-3">
					<div className="text-sm font-medium text-foreground">{m.nodecfg_endpoints()}</div>
					<div className="text-xs leading-5 text-muted-foreground">
						{m.nodecfg_endpoints_description()}
					</div>
					{tunnel.endpoints.map((endpoint, index) => (
						<div
							key={`endpoint-${index}`}
							className="rounded-xl border border-border bg-muted/20 p-4"
						>
							<div className="grid gap-4 lg:grid-cols-[1.3fr,0.7fr,auto]">
								<Field
									title={m.nodecfg_listen_address()}
									hint={m.nodecfg_listen_address_short_hint()}
									error={issueMap[`tunnel.endpoints.${index}.listen_addr`]}
								>
									<Input
										value={endpoint.listen_addr}
										onChange={(event) => {
											const endpoints = [...tunnel.endpoints];
											endpoints[index] = {
												...endpoint,
												listen_addr: event.target.value,
											};
											updateTunnel({ endpoints });
										}}
									/>
								</Field>
								<Field
									title={m.nodecfg_transport()}
									hint={m.nodecfg_transport_hint()}
									error={issueMap[`tunnel.endpoints.${index}.transport`]}
								>
									<NativeSelect
										value={endpoint.transport}
										onChange={(event) => {
											const endpoints = [...tunnel.endpoints];
											endpoints[index] = {
												...endpoint,
												transport: event.target.value,
											};
											updateTunnel({ endpoints });
										}}
										className="w-full"
									>
										<NativeSelectOption value="tcp">{m.transport_tcp()}</NativeSelectOption>
										<NativeSelectOption value="udp">{m.transport_udp_kcp()}</NativeSelectOption>
										<NativeSelectOption value="quic">{m.transport_quic()}</NativeSelectOption>
										<NativeSelectOption value="websocket">{m.transport_websocket()}</NativeSelectOption>
										<NativeSelectOption value="webtransport">{m.transport_webtransport()}</NativeSelectOption>
									</NativeSelect>
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
										{m.nodecfg_remove()}
									</DangerButton>
								</div>
							</div>
							{endpoint.transport === "quic" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-2">
									<Field title={m.nodecfg_quic_cert_file()} hint={m.nodecfg_quic_cert_file_hint()}>
										<Input
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
										/>
									</Field>
									<Field title={m.nodecfg_quic_key_file()} hint={m.nodecfg_tls_key_path_hint()}>
										<Input
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
										/>
									</Field>
								</div>
							) : null}
							{endpoint.transport === "websocket" ||
							endpoint.transport === "ws" ||
							endpoint.transport === "wss" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-3">
									<Field title={m.nodecfg_ws_cert_file()} hint={m.nodecfg_ws_cert_file_hint()}>
										<Input
											value={endpoint.websocket?.cert_file ?? ""}
											onChange={(event) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													websocket: {
														cert_file: event.target.value,
														key_file: endpoint.websocket?.key_file ?? "",
														url_path: endpoint.websocket?.url_path ?? "",
													},
												};
												updateTunnel({ endpoints });
											}}
										/>
									</Field>
									<Field title={m.nodecfg_ws_key_file()} hint={m.nodecfg_tls_key_path_hint()}>
										<Input
											value={endpoint.websocket?.key_file ?? ""}
											onChange={(event) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													websocket: {
														cert_file: endpoint.websocket?.cert_file ?? "",
														key_file: event.target.value,
														url_path: endpoint.websocket?.url_path ?? "",
													},
												};
												updateTunnel({ endpoints });
											}}
										/>
									</Field>
									<Field title={m.nodecfg_ws_path()} hint={m.nodecfg_ws_path_hint()}>
										<Input
											value={endpoint.websocket?.url_path ?? ""}
											onChange={(event) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													websocket: {
														cert_file: endpoint.websocket?.cert_file ?? "",
														key_file: endpoint.websocket?.key_file ?? "",
														url_path: event.target.value,
													},
												};
												updateTunnel({ endpoints });
											}}
											placeholder="/ws"
										/>
									</Field>
								</div>
							) : null}
							{endpoint.transport === "webtransport" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-2">
									<Field title={m.nodecfg_wt_cert_file()} hint={m.nodecfg_wt_cert_file_hint()}>
										<Input
											value={endpoint.webtransport?.cert_file ?? ""}
											onChange={(event) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													webtransport: {
														cert_file: event.target.value,
														key_file: endpoint.webtransport?.key_file ?? "",
													},
												};
												updateTunnel({ endpoints });
											}}
										/>
									</Field>
									<Field title={m.nodecfg_wt_key_file()} hint={m.nodecfg_wt_cert_file_hint()}>
										<Input
											value={endpoint.webtransport?.key_file ?? ""}
											onChange={(event) => {
												const endpoints = [...tunnel.endpoints];
												endpoints[index] = {
													...endpoint,
													webtransport: {
														cert_file: endpoint.webtransport?.cert_file ?? "",
														key_file: event.target.value,
													},
												};
												updateTunnel({ endpoints });
											}}
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
						{m.nodecfg_add_endpoint()}
					</SecondaryButton>
				</div>

				<div className="space-y-3">
					<div className="text-sm font-medium text-foreground">{m.nodecfg_client()}</div>
					<div className="text-xs leading-5 text-muted-foreground">
						{m.nodecfg_client_description()}
					</div>
					{tunnel.client ? (
						<div className="rounded-xl border border-border bg-muted/20 p-4">
							<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
								<Field
									title={m.nodecfg_server_address()}
									hint={m.nodecfg_server_address_hint()}
									error={issueMap["tunnel.client.server_addr"]}
								>
									<Input
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
									/>
								</Field>
								<Field
									title={m.nodecfg_transport()}
									hint={m.nodecfg_transport_hint()}
									error={issueMap["tunnel.client.transport"]}
								>
									<NativeSelect
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
										className="w-full"
									>
										<NativeSelectOption value="auto">{m.transport_auto()}</NativeSelectOption>
										<NativeSelectOption value="webtransport">{m.transport_webtransport()}</NativeSelectOption>
										<NativeSelectOption value="quic">{m.transport_quic()}</NativeSelectOption>
										<NativeSelectOption value="tcp">{m.transport_tcp()}</NativeSelectOption>
										<NativeSelectOption value="udp">{m.transport_udp_kcp()}</NativeSelectOption>
										<NativeSelectOption value="websocket">{m.transport_websocket()}</NativeSelectOption>
									</NativeSelect>
								</Field>
								<Field title={m.nodecfg_dial_timeout_ms()} hint={m.nodecfg_dial_timeout_ms_hint()}>
									<Input
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
									/>
								</Field>
								<div className="flex items-end">
									<DangerButton onClick={() => updateTunnel({ client: null })}>
										<Trash2 className="h-4 w-4" />
										{m.nodecfg_remove_client()}
									</DangerButton>
								</div>
							</div>
							{tunnel.client.transport === "quic" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-2">
									<Field title={m.nodecfg_quic_server_name()} hint={m.nodecfg_tls_sni()}>
										<Input
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
										/>
									</Field>
									<SwitchRow
										checked={tunnel.client.quic?.insecure_skip_verify ?? false}
										onCheckedChange={(checked) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													quic: {
														server_name: tunnel.client.quic?.server_name ?? "",
														insecure_skip_verify: checked,
													},
												},
											})
										}
									>
										{m.nodecfg_skip_tls_verify()}
									</SwitchRow>
								</div>
							) : null}
							{tunnel.client.transport === "websocket" ||
							tunnel.client.transport === "ws" ||
							tunnel.client.transport === "wss" ? (
								<div className="mt-4">
									<SwitchRow
										checked={tunnel.client.websocket?.insecure_skip_verify ?? false}
										onCheckedChange={(checked) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													websocket: {
														insecure_skip_verify: checked,
													},
												},
											})
										}
									>
										{m.nodecfg_skip_tls_verify_wss()}
									</SwitchRow>
								</div>
							) : null}
							{tunnel.client.transport === "webtransport" ? (
								<div className="mt-4 grid gap-4 md:grid-cols-2">
									<Field title={m.nodecfg_wt_server_name()} hint={m.nodecfg_tls_sni()}>
										<Input
											value={tunnel.client.webtransport?.server_name ?? ""}
											onChange={(event) =>
												tunnel.client &&
												updateTunnel({
													client: {
														...tunnel.client,
														webtransport: {
															server_name: event.target.value,
															insecure_skip_verify:
																tunnel.client.webtransport?.insecure_skip_verify ?? false,
														},
													},
												})
											}
										/>
									</Field>
									<SwitchRow
										checked={tunnel.client.webtransport?.insecure_skip_verify ?? false}
										onCheckedChange={(checked) =>
											tunnel.client &&
											updateTunnel({
												client: {
													...tunnel.client,
													webtransport: {
														server_name: tunnel.client.webtransport?.server_name ?? "",
														insecure_skip_verify: checked,
													},
												},
											})
										}
									>
										{m.nodecfg_skip_tls_verify()}
									</SwitchRow>
								</div>
							) : null}
						</div>
					) : (
						<SecondaryButton onClick={() => updateTunnel({ client: createEmptyTunnelClient() })}>
							<Globe className="h-4 w-4" />
							{m.nodecfg_add_client()}
						</SecondaryButton>
					)}
				</div>

				<div className="space-y-3">
					<div className="text-sm font-medium text-foreground">{m.nodecfg_services()}</div>
					<div className="text-xs leading-5 text-muted-foreground">
						{m.nodecfg_services_description()}
					</div>
					{tunnel.services.map((service, index) => (
						<div
							key={`service-${index}`}
							className="rounded-xl border border-border bg-muted/20 p-4"
						>
							<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
								<Field
									title={m.nodecfg_service_name()}
									hint={m.nodecfg_service_name_hint()}
									error={issueMap[`tunnel.services.${index}.name`]}
								>
									<Input
										value={service.name}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = { ...service, name: event.target.value };
											updateTunnel({ services });
										}}
									/>
								</Field>
								<Field
									title={m.nodecfg_protocol()}
									hint={m.nodecfg_service_protocol_hint()}
									error={issueMap[`tunnel.services.${index}.proto`]}
								>
									<NativeSelect
										value={service.proto}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = { ...service, proto: event.target.value };
											updateTunnel({ services });
										}}
										className="w-full"
									>
										<NativeSelectOption value="tcp">{m.transport_tcp()}</NativeSelectOption>
										<NativeSelectOption value="udp">{m.transport_udp()}</NativeSelectOption>
									</NativeSelect>
								</Field>
								<Field
									title={m.nodecfg_local_address()}
									hint={m.nodecfg_local_address_hint()}
									error={issueMap[`tunnel.services.${index}.local_addr`]}
								>
									<Input
										value={service.local_addr}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = { ...service, local_addr: event.target.value };
											updateTunnel({ services });
										}}
									/>
								</Field>
								<Field
									title={m.nodecfg_remote_address()}
									hint={m.nodecfg_remote_address_hint()}
									error={issueMap[`tunnel.services.${index}.remote_addr`]}
								>
									<Input
										value={service.remote_addr}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = { ...service, remote_addr: event.target.value };
											updateTunnel({ services });
										}}
									/>
								</Field>
								<Field title={m.nodecfg_masquerade_host()} hint={m.nodecfg_masquerade_host_hint()}>
									<Input
										value={service.masquerade_host}
										onChange={(event) => {
											const services = [...tunnel.services];
											services[index] = {
												...service,
												masquerade_host: event.target.value,
											};
											updateTunnel({ services });
										}}
									/>
								</Field>
								<div className="flex items-end gap-3">
									<SwitchRow
										checked={service.route_only}
										onCheckedChange={(checked) => {
											const services = [...tunnel.services];
											services[index] = {
												...service,
												route_only: checked,
											};
											updateTunnel({ services });
										}}
									>
										{m.nodecfg_route_only()}
									</SwitchRow>
									<DangerButton
										onClick={() =>
											updateTunnel({
												services: tunnel.services.filter((_, i) => i !== index),
											})
										}
									>
										<Trash2 className="h-4 w-4" />
										{m.nodecfg_remove()}
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
							{m.nodecfg_add_service()}
						</SecondaryButton>
					</div>
				</div>
			</div>
		</SectionCard>
	);
}

export const ManagedConfigEditor = NodeConfigEditor;
