import { Link } from "@tanstack/react-router";
import { Activity, ArrowRight, Check, Cpu, Gauge, Layers, RotateCcw, Zap } from "lucide-react";
import { useState } from "react";

import { PageHeader } from "@/components/ui";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useClientConfig } from "@/hooks/useClientConfig";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

export function ClientOptimizer() {
	const {
		optimizerEnabled,
		setOptimizerEnabled,
		optimizerZstdLevel,
		setOptimizerZstdLevel,
		optimizerAdaptiveFlush,
		setOptimizerAdaptiveFlush,
		optimizerFlushIntervalMs,
		setOptimizerFlushIntervalMs,
		optimizerBufferThreshold,
		setOptimizerBufferThreshold,
	} = useClientConfig();

	const [savedNotice, setSavedNotice] = useState(false);

	const triggerSaved = () => {
		setSavedNotice(true);
		setTimeout(() => setSavedNotice(false), 2000);
	};

	const handleResetDefaults = () => {
		setOptimizerEnabled(true);
		setOptimizerZstdLevel(3);
		setOptimizerAdaptiveFlush(true);
		setOptimizerFlushIntervalMs(20);
		setOptimizerBufferThreshold(65536);
		triggerSaved();
	};

	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-3 p-3 sm:p-4 overflow-y-auto">
			<PageHeader
				icon={<Zap className="h-4 w-4 text-primary" />}
				title={m.optimizer_title()}
				badge={
					<Badge
						variant={optimizerEnabled ? "default" : "secondary"}
						className={cn(
							"text-[10px] font-mono px-1.5 py-0.5",
							optimizerEnabled
								? "bg-primary/20 text-primary border border-primary/30"
								: "text-muted-foreground",
						)}
					>
						{optimizerEnabled ? "ACTIVE" : "DISABLED"}
					</Badge>
				}
				description={m.optimizer_description()}
				actions={
					<div className="flex items-center gap-1.5 flex-none">
						<Link to="/traffic">
							<Button
								variant="outline"
								size="xs"
								className="h-7 text-xs gap-1 cursor-pointer"
								title="View Traffic & Compression Stats"
							>
								<Activity className="h-3 w-3 text-emerald-500" />
								<span>{m.nav_traffic()}</span>
							</Button>
						</Link>
						<Button
							variant="ghost"
							size="xs"
							onClick={handleResetDefaults}
							className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground cursor-pointer gap-1"
							title="Reset Defaults"
						>
							<RotateCcw className="h-3 w-3" />
							<span>Reset</span>
						</Button>
						{savedNotice ? (
							<span className="flex items-center gap-1 text-[11px] text-emerald-500 font-medium">
								<Check className="h-3.5 w-3.5" />
								{m.optimizer_save_success()}
							</span>
						) : null}
					</div>
				}
				className="pb-2.5 mb-0"
			/>

			{/* Main Settings Grid */}
			<div className="grid grid-cols-1 md:grid-cols-2 gap-3 flex-1 min-h-0">
				{/* 1. Optimizer Master Switch Card */}
				<div className="flex flex-col rounded-lg border border-border bg-card p-3.5 shadow-xs space-y-3">
					<div className="flex items-start justify-between gap-3">
						<div className="space-y-0.5">
							<div className="flex items-center gap-2">
								<Zap className="h-4 w-4 text-primary" />
								<span className="text-xs font-semibold text-foreground">
									{m.optimizer_general_card()}
								</span>
							</div>
							<p className="text-[11px] text-muted-foreground">{m.optimizer_general_card_hint()}</p>
						</div>
						<Switch
							checked={optimizerEnabled}
							onCheckedChange={(val) => {
								setOptimizerEnabled(val);
								triggerSaved();
							}}
						/>
					</div>

					<div className="rounded-md border border-border/60 bg-muted/20 p-2.5 text-[11px] text-muted-foreground leading-relaxed">
						{m.optimizer_enabled_hint()}
					</div>
				</div>

				{/* 2. Zstandard Compression Level Card */}
				<div
					className={cn(
						"flex flex-col rounded-lg border border-border bg-card p-3.5 shadow-xs space-y-3 transition-opacity",
						!optimizerEnabled && "opacity-50 pointer-events-none",
					)}
				>
					<div className="space-y-0.5">
						<div className="flex items-center justify-between">
							<div className="flex items-center gap-2">
								<Cpu className="h-4 w-4 text-primary" />
								<span className="text-xs font-semibold text-foreground">
									{m.optimizer_compression_card()}
								</span>
							</div>
							<Badge variant="outline" className="font-mono text-xs">
								Level {optimizerZstdLevel}
							</Badge>
						</div>
						<p className="text-[11px] text-muted-foreground">
							{m.optimizer_compression_card_hint()}
						</p>
					</div>

					<div className="space-y-2 pt-1">
						<div className="flex items-center justify-between text-xs">
							<span className="font-medium text-foreground">{m.optimizer_level_label()}</span>
							<div className="flex gap-1">
								{[1, 3, 5, 9].map((lvl) => (
									<button
										key={lvl}
										type="button"
										onClick={() => {
											setOptimizerZstdLevel(lvl);
											triggerSaved();
										}}
										className={cn(
											"h-6 px-2 text-[10px] font-mono rounded border transition-colors cursor-pointer",
											optimizerZstdLevel === lvl
												? "bg-primary text-primary-foreground border-primary font-bold"
												: "border-border/60 hover:bg-accent hover:text-foreground text-muted-foreground",
										)}
									>
										{lvl === 1 ? "1 (Fast)" : lvl === 3 ? "3 (Balanced)" : `${lvl}`}
									</button>
								))}
							</div>
						</div>
						<input
							type="range"
							min={1}
							max={19}
							step={1}
							value={optimizerZstdLevel}
							onChange={(e) => {
								setOptimizerZstdLevel(Number(e.target.value));
								triggerSaved();
							}}
							className="w-full h-1.5 bg-muted rounded-lg appearance-none cursor-pointer accent-primary"
						/>
						<p className="text-[10px] text-muted-foreground">{m.optimizer_level_hint()}</p>
					</div>
				</div>

				{/* 3. Batching & Latency Control Card */}
				<div
					className={cn(
						"flex flex-col rounded-lg border border-border bg-card p-3.5 shadow-xs space-y-3 transition-opacity md:col-span-2",
						!optimizerEnabled && "opacity-50 pointer-events-none",
					)}
				>
					<div className="flex items-center gap-2">
						<Gauge className="h-4 w-4 text-primary" />
						<div className="space-y-0.5">
							<span className="text-xs font-semibold text-foreground">
								{m.optimizer_batching_card()}
							</span>
							<p className="text-[11px] text-muted-foreground">
								{m.optimizer_batching_card_hint()}
							</p>
						</div>
					</div>

					<div className="grid grid-cols-1 md:grid-cols-3 gap-3 pt-1">
						{/* Adaptive Flush */}
						<div className="flex flex-col justify-between rounded-lg border border-border/60 bg-muted/10 p-3 space-y-2">
							<div className="flex items-center justify-between">
								<span className="text-xs font-medium text-foreground">
									{m.optimizer_adaptive_flush_label()}
								</span>
								<Switch
									checked={optimizerAdaptiveFlush}
									onCheckedChange={(val) => {
										setOptimizerAdaptiveFlush(val);
										triggerSaved();
									}}
								/>
							</div>
							<p className="text-[10px] text-muted-foreground">
								{m.optimizer_adaptive_flush_hint()}
							</p>
						</div>

						{/* Base Flush Interval */}
						<div className="flex flex-col justify-between rounded-lg border border-border/60 bg-muted/10 p-3 space-y-2">
							<div className="flex items-center justify-between">
								<span className="text-xs font-medium text-foreground">
									{m.optimizer_flush_interval_label()}
								</span>
								<Badge variant="outline" className="font-mono text-[10px]">
									{optimizerFlushIntervalMs} ms
								</Badge>
							</div>
							<input
								type="range"
								min={2}
								max={60}
								step={2}
								value={optimizerFlushIntervalMs}
								onChange={(e) => {
									setOptimizerFlushIntervalMs(Number(e.target.value));
									triggerSaved();
								}}
								className="w-full h-1.5 bg-muted rounded-lg appearance-none cursor-pointer accent-primary"
							/>
							<p className="text-[10px] text-muted-foreground">
								{m.optimizer_flush_interval_hint()}
							</p>
						</div>

						{/* Buffer Threshold */}
						<div className="flex flex-col justify-between rounded-lg border border-border/60 bg-muted/10 p-3 space-y-2">
							<div className="flex items-center justify-between">
								<span className="text-xs font-medium text-foreground">
									{m.optimizer_buffer_threshold_label()}
								</span>
								<Badge variant="outline" className="font-mono text-[10px]">
									{(optimizerBufferThreshold / 1024).toFixed(0)} KB
								</Badge>
							</div>
							<input
								type="range"
								min={8192}
								max={262144}
								step={8192}
								value={optimizerBufferThreshold}
								onChange={(e) => {
									setOptimizerBufferThreshold(Number(e.target.value));
									triggerSaved();
								}}
								className="w-full h-1.5 bg-muted rounded-lg appearance-none cursor-pointer accent-primary"
							/>
							<p className="text-[10px] text-muted-foreground">
								{m.optimizer_buffer_threshold_hint()}
							</p>
						</div>
					</div>
				</div>

				{/* 4. Multi-Lane Middleware Integration Banner */}
				<div className="md:col-span-2 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 rounded-lg border border-primary/25 bg-primary/5 p-3.5 shadow-xs">
					<div className="flex items-start gap-2.5">
						<Layers className="h-4 w-4 text-primary mt-0.5 flex-none" />
						<div className="space-y-0.5">
							<div className="text-xs font-semibold text-foreground">
								{m.optimizer_middleware_lanes_card()}
							</div>
							<p className="text-[11px] text-muted-foreground">
								{m.optimizer_middleware_lanes_desc()}
							</p>
						</div>
					</div>
					<Link to="/middleware">
						<Button
							size="xs"
							variant="outline"
							className="h-7 text-xs gap-1 flex-none cursor-pointer"
						>
							<span>{m.optimizer_go_to_middleware()}</span>
							<ArrowRight className="h-3 w-3" />
						</Button>
					</Link>
				</div>
			</div>
		</div>
	);
}
