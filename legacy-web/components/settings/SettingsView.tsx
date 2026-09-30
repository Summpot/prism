import { Link } from "@tanstack/react-router";
import {
	AlertCircle,
	ArrowUpCircle,
	Check,
	CheckCircle2,
	Copy,
	Download,
	ExternalLink,
	Info,
	Languages,
	Monitor,
	Moon,
	Palette,
	Plug,
	RefreshCw,
	Settings,
	SlidersHorizontal,
	Sparkles,
	Sun,
	Zap,
} from "lucide-react";
import { useState } from "react";

import { Github } from "@/components/icons/Github";
import { PageHeader } from "@/components/ui";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useClientConfig } from "@/hooks/useClientConfig";
import { checkForUpdate, installUpdate } from "@/lib/client/clientIpc";
import { usePanelSession } from "@/lib/panelSession";
import { useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";
import { getLocale, type locales, setLocale } from "@/paraglide/runtime";
import type { UpdateCheckResult } from "@/types/client";

type SettingsTab = "general" | "connection" | "updates" | "about";

export function SettingsView() {
	const [activeTab, setActiveTab] = useState<SettingsTab>("general");
	const { theme, setTheme } = useTheme();
	const currentLocale = getLocale();
	const { isAdmin } = usePanelSession();

	const {
		autoConnect,
		setAutoConnect,
		autostart,
		setAutostart,
		silentAutostart,
		setSilentAutostart,
		autoConnectPanel,
		setAutoConnectPanel,
		fakeLanBroadcast,
		setFakeLanBroadcast,
		updateChannel,
		setUpdateChannel,
		autoCheckUpdate,
		setAutoCheckUpdate,
		managementUrl,
		deviceId,
	} = useClientConfig();

	const [copiedDeviceId, setCopiedDeviceId] = useState(false);

	const handleCopyDeviceId = async () => {
		if (!deviceId) return;
		try {
			await navigator.clipboard.writeText(deviceId);
			setCopiedDeviceId(true);
			setTimeout(() => setCopiedDeviceId(false), 2000);
		} catch (err) {
			console.error("Failed to copy device ID:", err);
		}
	};

	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-3 p-3 sm:p-4 overflow-y-auto">
			<PageHeader
				icon={<Settings className="h-4 w-4 text-primary" />}
				title={m.settings_title()}
				badge={
					<Badge variant="outline" className="text-[10px] font-mono px-1.5 py-0.5">
						v0.1.0
					</Badge>
				}
				description={m.settings_description()}
				className="pb-2.5 mb-0"
			/>

			{/* Navigation Tabs */}
			<div className="flex flex-none gap-1 border-b border-border/60 pb-1 overflow-x-auto scrollbar-none">
				<button
					type="button"
					onClick={() => setActiveTab("general")}
					className={cn(
						"flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors cursor-pointer whitespace-nowrap",
						activeTab === "general"
							? "bg-primary/15 text-primary font-semibold"
							: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
					)}
				>
					<Palette className="h-3.5 w-3.5" />
					<span>{m.settings_tab_general()}</span>
				</button>

				<button
					type="button"
					onClick={() => setActiveTab("connection")}
					className={cn(
						"flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors cursor-pointer whitespace-nowrap",
						activeTab === "connection"
							? "bg-primary/15 text-primary font-semibold"
							: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
					)}
				>
					<Zap className="h-3.5 w-3.5" />
					<span>{m.settings_tab_connection()}</span>
				</button>

				<button
					type="button"
					onClick={() => setActiveTab("updates")}
					className={cn(
						"flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors cursor-pointer whitespace-nowrap",
						activeTab === "updates"
							? "bg-primary/15 text-primary font-semibold"
							: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
					)}
				>
					<ArrowUpCircle className="h-3.5 w-3.5" />
					<span>{m.settings_tab_updates()}</span>
				</button>

				<button
					type="button"
					onClick={() => setActiveTab("about")}
					className={cn(
						"flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-colors cursor-pointer whitespace-nowrap",
						activeTab === "about"
							? "bg-primary/15 text-primary font-semibold"
							: "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
					)}
				>
					<Info className="h-3.5 w-3.5" />
					<span>{m.settings_tab_about()}</span>
				</button>
			</div>

			{/* Tab 1: General & Appearance */}
			{activeTab === "general" && (
				<div className="space-y-3 flex-1 min-h-0">
					{/* Theme Selection Card */}
					<div className="rounded-lg border border-border bg-card p-3.5 shadow-xs space-y-2.5">
						<div>
							<h2 className="text-xs font-semibold text-foreground flex items-center gap-1.5">
								<Palette className="h-3.5 w-3.5 text-primary" />
								<span>{m.settings_theme()}</span>
							</h2>
							<p className="text-[11px] text-muted-foreground mt-0.5">{m.settings_theme_desc()}</p>
						</div>

						<div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5 pt-1">
							<ThemeOptionButton
								active={theme === "system"}
								onClick={() => setTheme("system")}
								icon={<Monitor className="h-4 w-4" />}
								title={m.settings_theme_system()}
							/>
							<ThemeOptionButton
								active={theme === "light"}
								onClick={() => setTheme("light")}
								icon={<Sun className="h-4 w-4" />}
								title={m.settings_theme_light()}
							/>
							<ThemeOptionButton
								active={theme === "dark"}
								onClick={() => setTheme("dark")}
								icon={<Moon className="h-4 w-4" />}
								title={m.settings_theme_dark()}
							/>
						</div>
					</div>

					{/* Language Selection Card */}
					<div className="rounded-lg border border-border bg-card p-3.5 shadow-xs space-y-2.5">
						<div>
							<h2 className="text-xs font-semibold text-foreground flex items-center gap-1.5">
								<Languages className="h-3.5 w-3.5 text-primary" />
								<span>{m.settings_language()}</span>
							</h2>
							<p className="text-[11px] text-muted-foreground mt-0.5">
								{m.settings_language_desc()}
							</p>
						</div>

						<div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5 pt-1">
							<button
								type="button"
								onClick={() => void setLocale("zh-CN" as (typeof locales)[number])}
								className={cn(
									"flex items-center justify-between p-3 rounded-lg border text-xs transition-all cursor-pointer",
									currentLocale === "zh-CN"
										? "border-primary bg-primary/10 text-primary font-semibold shadow-xs ring-1 ring-primary/25"
										: "border-border/70 hover:bg-accent/50 text-foreground",
								)}
							>
								<div className="flex items-center gap-2">
									<span className="font-semibold text-xs">简体中文</span>
									<span className="text-[10px] text-muted-foreground">Chinese (Simplified)</span>
								</div>
								{currentLocale === "zh-CN" && <Check className="h-3.5 w-3.5 text-primary" />}
							</button>

							<button
								type="button"
								onClick={() => void setLocale("en" as (typeof locales)[number])}
								className={cn(
									"flex items-center justify-between p-3 rounded-lg border text-xs transition-all cursor-pointer",
									currentLocale === "en"
										? "border-primary bg-primary/10 text-primary font-semibold shadow-xs ring-1 ring-primary/25"
										: "border-border/70 hover:bg-accent/50 text-foreground",
								)}
							>
								<div className="flex items-center gap-2">
									<span className="font-semibold text-xs">English</span>
									<span className="text-[10px] text-muted-foreground">English (US)</span>
								</div>
								{currentLocale === "en" && <Check className="h-3.5 w-3.5 text-primary" />}
							</button>
						</div>
					</div>
				</div>
			)}

			{/* Tab 2: Startup & Connection */}
			{activeTab === "connection" && (
				<div className="space-y-3 flex-1 min-h-0">
					{/* Behavior Switches */}
					<div className="rounded-lg border border-border bg-card p-3.5 shadow-xs space-y-3">
						<h2 className="text-xs font-semibold text-foreground flex items-center gap-1.5">
							<Zap className="h-3.5 w-3.5 text-primary" />
							<span>{m.settings_tab_connection()}</span>
						</h2>

						<div className="space-y-2">
							<div className="flex items-center justify-between rounded-lg border border-border/60 p-2.5 text-xs">
								<div className="space-y-0.5">
									<div className="font-medium text-xs text-foreground">
										{m.client_auto_connect()}
									</div>
									<div className="text-[10px] text-muted-foreground">
										{m.client_auto_connect_hint()}
									</div>
								</div>
								<Switch checked={autoConnect} onCheckedChange={setAutoConnect} />
							</div>

							<div className="flex items-center justify-between rounded-lg border border-border/60 p-2.5 text-xs">
								<div className="space-y-0.5">
									<div className="font-medium text-xs text-foreground">
										{m.client_autostart()}
									</div>
									<div className="text-[10px] text-muted-foreground">
										{m.client_autostart_hint()}
									</div>
								</div>
								<Switch checked={autostart} onCheckedChange={setAutostart} />
							</div>

							<div
								className={cn(
									"flex items-center justify-between rounded-lg border border-border/60 p-2.5 text-xs transition-opacity",
									!autostart && "opacity-60",
								)}
							>
								<div className="space-y-0.5">
									<div className="font-medium text-xs text-foreground">
										{m.client_silent_autostart()}
									</div>
									<div className="text-[10px] text-muted-foreground">
										{m.client_silent_autostart_hint()}
									</div>
								</div>
								<Switch
									checked={silentAutostart}
									onCheckedChange={setSilentAutostart}
									disabled={!autostart}
								/>
							</div>

							<div className="flex items-center justify-between rounded-lg border border-border/60 p-2.5 text-xs">
								<div className="space-y-0.5">
									<div className="font-medium text-xs text-foreground">
										{m.client_auto_connect_panel()}
									</div>
									<div className="text-[10px] text-muted-foreground">
										{m.client_auto_connect_panel_hint({ url: managementUrl })}
									</div>
								</div>
								<Switch checked={autoConnectPanel} onCheckedChange={setAutoConnectPanel} />
							</div>

							<div className="flex items-center justify-between rounded-lg border border-border/60 p-2.5 text-xs">
								<div className="space-y-0.5">
									<div className="font-medium text-xs text-foreground">
										{m.client_lan_broadcast()}
									</div>
									<div className="text-[10px] text-muted-foreground">
										{m.client_lan_broadcast_hint()}
									</div>
								</div>
								<Switch checked={fakeLanBroadcast} onCheckedChange={setFakeLanBroadcast} />
							</div>
						</div>
					</div>

					{/* Backend Management URL & Session */}
					<div className="rounded-lg border border-border bg-card p-3.5 shadow-xs space-y-2.5">
						<div className="flex items-center justify-between">
							<h2 className="text-xs font-semibold text-foreground flex items-center gap-1.5">
								<Plug className="h-3.5 w-3.5 text-primary" />
								<span>{m.settings_management_url()}</span>
							</h2>
							{isAdmin ? (
								<Badge className="bg-primary text-primary-foreground text-[10px] px-1.5 py-0 h-4">
									{m.session_logged_in()}
								</Badge>
							) : (
								<Badge
									variant="outline"
									className="text-[10px] text-muted-foreground px-1.5 py-0 h-4"
								>
									{m.session_not_connected()}
								</Badge>
							)}
						</div>
						<p className="text-[11px] text-muted-foreground">{m.settings_management_url_desc()}</p>

						<div className="flex items-center gap-2 pt-1">
							<div className="font-mono text-xs bg-muted/60 px-3 py-1.5 rounded-md border border-border/70 flex-1 truncate text-foreground">
								{managementUrl}
							</div>
							<Link to={isAdmin ? "/admin" : "/login"}>
								<Button variant="outline" size="xs" className="h-7 text-xs gap-1 cursor-pointer">
									<span>{isAdmin ? m.nav_admin() : m.login_title()}</span>
								</Button>
							</Link>
						</div>
					</div>

					{/* Tunnel Profiles Shortcut Card */}
					<div className="rounded-lg border border-border/80 bg-gradient-to-r from-primary/5 via-card to-card p-3.5 shadow-xs flex flex-col sm:flex-row sm:items-center justify-between gap-3">
						<div className="space-y-0.5 min-w-0">
							<div className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
								<SlidersHorizontal className="h-3.5 w-3.5 text-primary" />
								<span>{m.settings_profiles_link_title()}</span>
							</div>
							<p className="text-[10px] text-muted-foreground">{m.settings_profiles_link_desc()}</p>
						</div>

						<Link to="/profiles">
							<Button
								size="xs"
								variant="outline"
								className="gap-1 h-7 text-xs flex-none cursor-pointer"
							>
								<span>{m.settings_profiles_link_btn()}</span>
							</Button>
						</Link>
					</div>
				</div>
			)}

			{/* Tab 3: Software Updates */}
			{activeTab === "updates" && (
				<div className="space-y-3 flex-1 min-h-0">
					<SoftwareUpdatesCard
						updateChannel={updateChannel}
						setUpdateChannel={setUpdateChannel}
						autoCheckUpdate={autoCheckUpdate}
						setAutoCheckUpdate={setAutoCheckUpdate}
					/>
				</div>
			)}

			{/* Tab 4: About & System Info */}
			{activeTab === "about" && (
				<div className="space-y-3 flex-1 min-h-0">
					{/* App Info Card */}
					<div className="rounded-lg border border-border bg-card p-4 shadow-xs space-y-3.5">
						<div className="flex items-center gap-3">
							<img
								src="/logo192.png"
								alt="Prism"
								className="h-12 w-12 rounded-xl object-contain shadow-xs"
							/>
							<div>
								<h2 className="text-base font-bold text-foreground tracking-tight">Prism</h2>
								<p className="text-xs text-muted-foreground">{m.brand_tagline()}</p>
							</div>
						</div>

						<div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5 pt-1 text-xs border-t border-border/50">
							<div className="flex items-center justify-between p-2 rounded-md bg-muted/30 border border-border/40">
								<span className="text-muted-foreground text-[11px]">
									{m.settings_app_version()}
								</span>
								<span className="font-mono text-foreground font-medium">
									v0.1.0 ({updateChannel})
								</span>
							</div>
						</div>

						{/* Device ID */}
						<div className="space-y-1 pt-1">
							<label className="text-[10px] uppercase font-bold text-muted-foreground">
								{m.settings_device_id()}
							</label>
							<div className="flex items-center gap-2">
								<div className="font-mono text-[11px] bg-muted/60 px-3 py-1.5 rounded-md border border-border/70 flex-1 truncate text-foreground select-all">
									{deviceId || "00000000-0000-0000-0000-000000000000"}
								</div>
								<Button
									variant="outline"
									size="xs"
									onClick={handleCopyDeviceId}
									className="h-7 gap-1 text-xs px-2.5 cursor-pointer flex-none"
									title={copiedDeviceId ? m.settings_device_id_copied() : "Copy"}
								>
									{copiedDeviceId ? (
										<Check className="h-3.5 w-3.5 text-emerald-500" />
									) : (
										<Copy className="h-3.5 w-3.5" />
									)}
									<span>{copiedDeviceId ? m.common_copied() : "Copy"}</span>
								</Button>
							</div>
							<p className="text-[10px] text-muted-foreground">{m.settings_device_id_desc()}</p>
						</div>

						{/* External Link */}
						<div className="pt-2 border-t border-border/50 flex items-center justify-between flex-wrap gap-2">
							<a
								href="https://github.com/Summpot/prism"
								target="_blank"
								rel="noreferrer"
								className="inline-flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
							>
								<Github className="h-4 w-4" />
								<span>GitHub Repository</span>
								<ExternalLink className="h-3 w-3" />
							</a>

							<span className="text-[10px] text-muted-foreground font-mono">
								Prism High-Performance Tunnel
							</span>
						</div>
					</div>
				</div>
			)}
		</div>
	);
}

function ThemeOptionButton({
	active,
	onClick,
	icon,
	title,
}: {
	active: boolean;
	onClick: () => void;
	icon: React.ReactNode;
	title: string;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"flex items-center gap-2.5 p-3 rounded-lg border text-xs transition-all cursor-pointer",
				active
					? "border-primary bg-primary/10 text-primary font-semibold shadow-xs ring-1 ring-primary/25"
					: "border-border/70 hover:bg-accent/50 text-foreground",
			)}
		>
			<div className={cn(active ? "text-primary" : "text-muted-foreground")}>{icon}</div>
			<span className="font-medium text-xs">{title}</span>
		</button>
	);
}

function SoftwareUpdatesCard({
	updateChannel,
	setUpdateChannel,
	autoCheckUpdate,
	setAutoCheckUpdate,
}: {
	updateChannel: string;
	setUpdateChannel: (val: string) => void;
	autoCheckUpdate: boolean;
	setAutoCheckUpdate: (val: boolean) => void;
}) {
	const [status, setStatus] = useState<
		"idle" | "checking" | "up-to-date" | "available" | "installing" | "error"
	>("idle");
	const [updateResult, setUpdateResult] = useState<UpdateCheckResult | null>(null);
	const [errorMessage, setErrorMessage] = useState<string | null>(null);

	const handleCheck = async () => {
		setStatus("checking");
		setErrorMessage(null);
		try {
			const res = await checkForUpdate(updateChannel);
			setUpdateResult(res);
			if (res.available) {
				setStatus("available");
			} else {
				setStatus("up-to-date");
			}
		} catch (err) {
			setStatus("error");
			setErrorMessage(err instanceof Error ? err.message : String(err));
		}
	};

	const handleInstall = async () => {
		setStatus("installing");
		setErrorMessage(null);
		try {
			await installUpdate(updateChannel);
		} catch (err) {
			setStatus("error");
			setErrorMessage(err instanceof Error ? err.message : String(err));
		}
	};

	return (
		<div className="rounded-lg border border-border bg-card p-3.5 shadow-xs space-y-3">
			<div className="flex items-center justify-between pb-1.5 border-b border-border/50">
				<div>
					<h2 className="text-xs font-semibold text-foreground flex items-center gap-1.5">
						<Sparkles className="h-3.5 w-3.5 text-primary" />
						<span>{m.client_update_section()}</span>
					</h2>
					<p className="text-[11px] text-muted-foreground mt-0.5">
						{m.client_update_section_desc()}
					</p>
				</div>
			</div>

			<div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-0.5">
				<div className="space-y-1">
					<label className="text-[10px] uppercase font-bold text-muted-foreground">
						{m.client_update_channel()}
					</label>
					<select
						aria-label={m.client_update_channel()}
						value={updateChannel}
						onChange={(e) => {
							setUpdateChannel(e.target.value);
							setStatus("idle");
							setUpdateResult(null);
						}}
						className="h-8 w-full rounded-md border border-input bg-background px-2.5 text-xs text-foreground outline-none focus:ring-1 focus:ring-ring cursor-pointer"
					>
						<option value="release">{m.client_update_channel_release()}</option>
						<option value="dev">{m.client_update_channel_dev()}</option>
					</select>
				</div>

				<div className="flex items-end">
					<Button
						variant="outline"
						size="sm"
						onClick={handleCheck}
						disabled={status === "checking" || status === "installing"}
						className="h-8 w-full text-xs gap-1.5 cursor-pointer"
					>
						<RefreshCw className={cn("h-3.5 w-3.5", status === "checking" && "animate-spin")} />
						<span>
							{status === "checking" ? m.client_update_checking() : m.client_update_check_btn()}
						</span>
					</Button>
				</div>
			</div>

			<div className="flex items-center justify-between rounded-lg border border-border/60 p-2.5 text-xs">
				<div className="space-y-0.5">
					<div className="font-medium text-xs text-foreground">{m.client_update_auto_check()}</div>
					<div className="text-[10px] text-muted-foreground">
						{m.client_update_auto_check_hint()}
					</div>
				</div>
				<Switch checked={autoCheckUpdate} onCheckedChange={setAutoCheckUpdate} />
			</div>

			{/* Status Feedback */}
			{status === "up-to-date" && updateResult && (
				<div className="flex items-center gap-1.5 rounded-md bg-emerald-500/10 border border-emerald-500/20 p-2.5 text-emerald-600 dark:text-emerald-400 text-xs">
					<CheckCircle2 className="h-3.5 w-3.5 shrink-0" />
					<span>{m.client_update_up_to_date({ version: updateResult.current_version })}</span>
				</div>
			)}

			{status === "error" && errorMessage && (
				<div className="flex items-center gap-1.5 rounded-md bg-destructive/10 border border-destructive/20 p-2.5 text-destructive text-xs">
					<AlertCircle className="h-3.5 w-3.5 shrink-0" />
					<span className="truncate">{m.client_update_failed({ error: errorMessage })}</span>
				</div>
			)}

			{(status === "available" || status === "installing") && updateResult && (
				<div className="space-y-2 rounded-md bg-primary/10 border border-primary/25 p-3 text-xs">
					<div className="flex items-center justify-between">
						<div className="flex items-center gap-1.5 font-semibold text-foreground">
							<Sparkles className="h-3.5 w-3.5 text-primary" />
							<span>{m.client_update_found({ version: updateResult.version || "latest" })}</span>
						</div>
						<Button
							size="xs"
							onClick={handleInstall}
							disabled={status === "installing"}
							className="h-6 gap-1 px-2.5 text-xs cursor-pointer"
						>
							<Download className="h-3 w-3" />
							<span>
								{status === "installing"
									? m.client_update_installing()
									: m.client_update_install_btn()}
							</span>
						</Button>
					</div>
					{updateResult.body ? (
						<div className="max-h-32 overflow-y-auto rounded bg-background/60 p-2 text-[10px] text-muted-foreground whitespace-pre-wrap font-mono">
							{updateResult.body}
						</div>
					) : null}
				</div>
			)}
		</div>
	);
}
