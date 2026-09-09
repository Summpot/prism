import { Check, Loader2, RefreshCw, RotateCcw, Sliders, Zap } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	type ConfigFieldSchema,
	type MiddlewareItem,
	listMiddlewares,
	resetMiddlewareConfig,
	updateMiddlewareConfig,
} from "@/lib/managementApi";
import type { PanelConnection } from "@/lib/panelConnection";

interface MiddlewareConfigEditorProps {
	connection: PanelConnection;
}

export function MiddlewareConfigEditor({ connection }: MiddlewareConfigEditorProps) {
	const [middlewares, setMiddlewares] = useState<MiddlewareItem[]>([]);
	const [selectedName, setSelectedName] = useState<string>("");
	const [formValues, setFormValues] = useState<Record<string, any>>({});
	const [loading, setLoading] = useState(false);
	const [saving, setSaving] = useState(false);
	const [resetting, setResetting] = useState(false);
	const [statusMessage, setStatusMessage] = useState<{
		type: "success" | "error";
		text: string;
	} | null>(null);

	const loadData = useCallback(async () => {
		setLoading(true);
		setStatusMessage(null);
		try {
			const list = await listMiddlewares(connection);
			setMiddlewares(list);
			if (list.length > 0) {
				const current = selectedName
					? list.find((m) => m.name === selectedName) || list[0]
					: list[0];
				setSelectedName(current.name);
				setFormValues(current.effective_config || {});
			}
		} catch (err) {
			setStatusMessage({
				type: "error",
				text: `Failed to load middlewares: ${err instanceof Error ? err.message : String(err)}`,
			});
		} finally {
			setLoading(false);
		}
	}, [connection, selectedName]);

	useEffect(() => {
		loadData();
	}, [loadData]);

	const selectedMiddleware = middlewares.find((m) => m.name === selectedName);
	const schema = selectedMiddleware?.schema;

	const handleSelect = (mw: MiddlewareItem) => {
		setSelectedName(mw.name);
		setFormValues(mw.effective_config || {});
		setStatusMessage(null);
	};

	const handleFieldChange = (key: string, value: any) => {
		setFormValues((prev) => ({
			...prev,
			[key]: value,
		}));
	};

	const handleSave = async () => {
		if (!selectedName) return;
		setSaving(true);
		setStatusMessage(null);
		try {
			const res = await updateMiddlewareConfig(connection, selectedName, formValues);
			setStatusMessage({
				type: "success",
				text: `Configuration for "${selectedName}" applied and hot-updated to running sessions!`,
			});
			setFormValues(res.config);
			// Refresh list state
			const updated = await listMiddlewares(connection);
			setMiddlewares(updated);
		} catch (err) {
			setStatusMessage({
				type: "error",
				text: `Failed to save configuration: ${err instanceof Error ? err.message : String(err)}`,
			});
		} finally {
			setSaving(false);
		}
	};

	const handleReset = async () => {
		if (!selectedName) return;
		setResetting(true);
		setStatusMessage(null);
		try {
			await resetMiddlewareConfig(connection, selectedName);
			setStatusMessage({
				type: "success",
				text: `Configuration for "${selectedName}" reset to schema defaults.`,
			});
			// Reload values
			const updated = await listMiddlewares(connection);
			setMiddlewares(updated);
			const current = updated.find((m) => m.name === selectedName);
			if (current) {
				setFormValues(current.effective_config || {});
			}
		} catch (err) {
			setStatusMessage({
				type: "error",
				text: `Failed to reset configuration: ${err instanceof Error ? err.message : String(err)}`,
			});
		} finally {
			setResetting(false);
		}
	};

	const renderFieldControl = (field: ConfigFieldSchema) => {
		const val = formValues[field.key] ?? field.default_value;
		const isDefault = JSON.stringify(val) === JSON.stringify(field.default_value);

		switch (field.field_type) {
			case "u8":
			case "u16":
			case "u32":
			case "i32":
			case "i64": {
				const numVal = typeof val === "number" ? val : Number(val) || 0;
				const max = field.field_type === "u8" ? 9 : field.field_type === "u16" ? 65535 : undefined;
				const min = field.field_type.startsWith("u") ? 0 : undefined;

				return (
					<div
						key={field.key}
						className="space-y-1.5 p-3 rounded-lg border border-border/60 bg-muted/20"
					>
						<div className="flex items-center justify-between gap-2">
							<label className="text-sm font-medium text-foreground flex items-center gap-1.5">
								{field.label}
								<span className="text-xs text-muted-foreground font-mono">
									({field.field_type})
								</span>
							</label>
							{!isDefault && (
								<Badge variant="secondary" className="text-[10px] px-1.5 py-0 h-4">
									Modified
								</Badge>
							)}
						</div>
						{field.description && (
							<p className="text-xs text-muted-foreground">{field.description}</p>
						)}
						<div className="flex items-center gap-3 pt-1">
							<Input
								type="number"
								min={min}
								max={max}
								value={numVal}
								onChange={(e) => handleFieldChange(field.key, Number(e.target.value))}
								className="font-mono text-sm max-w-[200px]"
							/>
							<span className="text-xs text-muted-foreground font-mono">
								Default: {JSON.stringify(field.default_value)}
							</span>
						</div>
					</div>
				);
			}

			case "bool": {
				const boolVal = Boolean(val);
				return (
					<div
						key={field.key}
						className="flex items-center justify-between p-3 rounded-lg border border-border/60 bg-muted/20"
					>
						<div className="space-y-0.5 pr-4">
							<div className="flex items-center gap-2">
								<label className="text-sm font-medium text-foreground">{field.label}</label>
								{!isDefault && (
									<Badge variant="secondary" className="text-[10px] px-1.5 py-0 h-4">
										Modified
									</Badge>
								)}
							</div>
							{field.description && (
								<p className="text-xs text-muted-foreground">{field.description}</p>
							)}
						</div>
						<input
							type="checkbox"
							checked={boolVal}
							onChange={(e) => handleFieldChange(field.key, e.target.checked)}
							className="size-4 rounded border-border text-primary focus:ring-primary/40"
						/>
					</div>
				);
			}

			case "string":
			default: {
				const strVal = typeof val === "string" ? val : String(val ?? "");
				const isMultiline = strVal.length > 50 || strVal.includes(",") || strVal.includes("\n");

				return (
					<div
						key={field.key}
						className="space-y-1.5 p-3 rounded-lg border border-border/60 bg-muted/20"
					>
						<div className="flex items-center justify-between gap-2">
							<label className="text-sm font-medium text-foreground flex items-center gap-1.5">
								{field.label}
								<span className="text-xs text-muted-foreground font-mono">
									({field.field_type})
								</span>
							</label>
							{!isDefault && (
								<Badge variant="secondary" className="text-[10px] px-1.5 py-0 h-4">
									Modified
								</Badge>
							)}
						</div>
						{field.description && (
							<p className="text-xs text-muted-foreground">{field.description}</p>
						)}
						{isMultiline ? (
							<textarea
								rows={2}
								value={strVal}
								onChange={(e) => handleFieldChange(field.key, e.target.value)}
								className="w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 py-1.5 text-sm font-mono transition-colors outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
							/>
						) : (
							<Input
								type="text"
								value={strVal}
								onChange={(e) => handleFieldChange(field.key, e.target.value)}
								className="font-mono text-sm"
							/>
						)}
						<p className="text-xs text-muted-foreground font-mono truncate">
							Default: {JSON.stringify(field.default_value)}
						</p>
					</div>
				);
			}
		}
	};

	return (
		<div className="rounded-xl border border-border bg-card p-5 shadow-xs space-y-5">
			<div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-border pb-4">
				<div className="flex items-center gap-2.5">
					<div className="p-2 rounded-lg bg-primary/10 text-primary">
						<Sliders className="size-5" />
					</div>
					<div>
						<h3 className="text-base font-semibold text-foreground flex items-center gap-2">
							WASM Middleware Dynamic Configuration
							<Badge
								variant="outline"
								className="text-[11px] font-mono gap-1 text-primary border-primary/30"
							>
								<Zap className="size-3" /> Component Model
							</Badge>
						</h3>
						<p className="text-xs text-muted-foreground">
							Reflected from WAT Component records. Values hot-update running sessions in real-time.
						</p>
					</div>
				</div>

				<div className="flex items-center gap-2">
					<Button
						variant="outline"
						size="sm"
						onClick={loadData}
						disabled={loading || saving}
						className="gap-1.5 text-xs"
					>
						<RefreshCw className={`size-3.5 ${loading ? "animate-spin" : ""}`} />
						Refresh
					</Button>
				</div>
			</div>

			{/* Middleware Selector Pills */}
			<div className="flex items-center gap-2 flex-wrap">
				<span className="text-xs font-medium text-muted-foreground mr-1">Middleware:</span>
				{middlewares.map((mw) => {
					const isSelected = mw.name === selectedName;
					const fieldCount = mw.schema?.fields?.length ?? 0;
					return (
						<button
							key={mw.name}
							type="button"
							onClick={() => handleSelect(mw)}
							className={`inline-flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-medium border transition-colors cursor-pointer ${
								isSelected
									? "bg-primary text-primary-foreground border-primary"
									: "bg-muted/40 hover:bg-muted text-foreground border-border"
							}`}
						>
							<span>{mw.name}.wat</span>
							<span
								className={`text-[10px] px-1 py-0.2 rounded-full ${
									isSelected
										? "bg-primary-foreground/20 text-primary-foreground"
										: "bg-muted text-muted-foreground"
								}`}
							>
								{fieldCount} {fieldCount === 1 ? "field" : "fields"}
							</span>
						</button>
					);
				})}
			</div>

			{/* Status Message */}
			{statusMessage && (
				<div
					className={`p-3 rounded-lg text-xs font-medium flex items-center gap-2 ${
						statusMessage.type === "success"
							? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20"
							: "bg-destructive/10 text-destructive border border-destructive/20"
					}`}
				>
					{statusMessage.type === "success" ? <Check className="size-4" /> : null}
					{statusMessage.text}
				</div>
			)}

			{/* Dynamic Field Controls */}
			{schema && schema.fields.length > 0 ? (
				<div className="space-y-3 pt-1">
					<div className="grid grid-cols-1 md:grid-cols-2 gap-3">
						{schema.fields.map(renderFieldControl)}
					</div>

					<div className="flex items-center justify-between pt-4 border-t border-border">
						<Button
							variant="outline"
							size="sm"
							onClick={handleReset}
							disabled={saving || resetting}
							className="text-xs gap-1.5 text-muted-foreground hover:text-foreground"
						>
							<RotateCcw className={`size-3.5 ${resetting ? "animate-spin" : ""}`} />
							Reset to Defaults
						</Button>

						<Button
							variant="default"
							size="sm"
							onClick={handleSave}
							disabled={saving || resetting}
							className="text-xs gap-1.5"
						>
							{saving ? (
								<Loader2 className="size-3.5 animate-spin" />
							) : (
								<Check className="size-3.5" />
							)}
							Apply Changes (Hot-Reload)
						</Button>
					</div>
				</div>
			) : (
				<div className="p-8 text-center text-xs text-muted-foreground border border-dashed border-border rounded-lg">
					No declarative configuration schema exported for {selectedName || "this middleware"}.
				</div>
			)}
		</div>
	);
}
