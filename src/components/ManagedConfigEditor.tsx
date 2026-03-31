import {
	AlertTriangle,
	Cable,
	CheckCircle2,
	CopyPlus,
	FolderSync,
	Plus,
	Router,
	Save,
	Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import {
	createEmptyManagedConfig,
	formatIssuesByPath,
	normalizeManagedConfig,
	validateManagedConfig,
} from "@/lib/managedConfig";
import type { ManagedConfigDocument } from "@/lib/managementApi";

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
					<p className="mt-2 max-w-3xl text-sm leading-6 text-slate-400">
						{description}
					</p>
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
			listeners: [
				...current.listeners,
				{ listen_addr: "", protocol: "tcp", upstream: "" },
			],
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
						<SectionLabel
							title="Max header bytes"
							hint="TCP routing prelude cap"
						/>
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
									<SectionLabel
										title="Listen address"
										hint="Examples: :25565 or 127.0.0.1:8081"
									/>
									<input
										value={listener.listen_addr}
										onChange={(event) =>
											updateListener(index, "listen_addr", event.target.value)
										}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError
										messages={issueMap[`listeners.${index}.listen_addr`]}
									/>
								</label>
								<label className="space-y-2">
									<SectionLabel
										title="Protocol"
										hint="TCP routing or UDP forwarding"
									/>
									<select
										value={listener.protocol}
										onChange={(event) =>
											updateListener(index, "protocol", event.target.value)
										}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									>
										<option value="tcp">tcp</option>
										<option value="udp">udp</option>
									</select>
								</label>
								<label className="space-y-2">
									<SectionLabel
										title="Upstream"
										hint="Leave empty for TCP hostname-routing"
									/>
									<input
										value={listener.upstream}
										onChange={(event) =>
											updateListener(index, "upstream", event.target.value)
										}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError
										messages={issueMap[`listeners.${index}.upstream`]}
									/>
								</label>
								<div className="flex items-end">
									<button
										type="button"
										onClick={() =>
											setDraft((current) => ({
												...current,
												listeners: current.listeners.filter(
													(_, i) => i !== index,
												),
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
									<SectionLabel
										title="Hosts"
										hint="One hostname or pattern per line"
									/>
									<textarea
										value={route.hosts.join("\n")}
										onChange={(event) =>
											updateRoute(index, "hosts", event.target.value)
										}
										rows={4}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError messages={issueMap[`routes.${index}.hosts`]} />
								</label>
								<label className="space-y-2">
									<SectionLabel
										title="Upstreams"
										hint="One upstream per line"
									/>
									<textarea
										value={route.upstreams.join("\n")}
										onChange={(event) =>
											updateRoute(index, "upstreams", event.target.value)
										}
										rows={4}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError
										messages={issueMap[`routes.${index}.upstreams`]}
									/>
								</label>
								<label className="space-y-2">
									<SectionLabel
										title="Middlewares"
										hint="One middleware name per line"
									/>
									<textarea
										value={route.middlewares.join("\n")}
										onChange={(event) =>
											updateRoute(index, "middlewares", event.target.value)
										}
										rows={4}
										className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
									/>
									<FieldError
										messages={issueMap[`routes.${index}.middlewares`]}
									/>
								</label>
								<div className="space-y-4">
									<label className="space-y-2">
										<SectionLabel
											title="Strategy"
											hint="Failover ordering for multi-upstream routes"
										/>
										<select
											value={route.strategy}
											onChange={(event) =>
												updateRoute(index, "strategy", event.target.value)
											}
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
						Fix {issues.length} validation issue{issues.length === 1 ? "" : "s"}{" "}
						before saving this revision.
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
