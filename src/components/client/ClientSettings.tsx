import { Check, Download, Plus, Share2, Trash2 } from "lucide-react";

import { useClientConfig } from "@/hooks/useClientConfig";
import { useClientLink } from "@/hooks/useClientLink";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { SUPPORTED_LINK_PROTOCOLS } from "@/lib/prismLink";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

export function ClientSettings() {
	const {
		profiles,
		selectedProfileId,
		handleSelectProfile,
		profileName,
		setProfileName,
		serverAddr,
		setServerAddr,
		transport,
		setTransport,
		listenAddr,
		setListenAddr,
		fakeLanBroadcast,
		setFakeLanBroadcast,
		autoConnectPanel,
		setAutoConnectPanel,
		autoConnect,
		setAutoConnect,
		managementUrl,
		handleSaveProfile,
		handleDeleteProfile,
	} = useClientConfig();
	const { setLinkProtocol, setImportModalOpen, handleShareLink, copied } = useClientLink();

	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-hidden">
			{/* Header Bar */}
			<div className="flex flex-none select-none items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
				<div className="flex items-center gap-2 min-w-0">
					<div className="min-w-0">
						<h1 className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground">
							{m.client_settings_title()}
						</h1>
						<p className="truncate text-[10px] text-muted-foreground hidden sm:block">
							{m.client_settings_description()}
						</p>
					</div>
				</div>

				{/* Top Actions */}
				<div className="flex items-center gap-1.5 flex-none">
					<Button
						variant="outline"
						size="xs"
						onClick={() => {
							const id = `profile-${Date.now()}`;
							setProfileName(m.client_new_profile_name());
							setServerAddr("relay.example.com");
							setTransport("auto");
							setListenAddr("127.0.0.1:25565");
							setFakeLanBroadcast(true);
							handleSelectProfile(id);
						}}
						className="h-7 gap-1 text-xs px-2.5 cursor-pointer"
					>
						<Plus className="h-3.5 w-3.5" />
						<span>{m.client_new_profile()}</span>
					</Button>

					<Button
						variant="outline"
						size="xs"
						onClick={() => setImportModalOpen(true)}
						className="h-7 gap-1 text-xs px-2.5 cursor-pointer"
					>
						<Download className="h-3.5 w-3.5 text-primary" />
						<span>{m.client_import_link()}</span>
					</Button>

					<Button
						variant="outline"
						size="xs"
						onClick={handleShareLink}
						className="h-7 gap-1 text-xs px-2.5 cursor-pointer"
					>
						{copied === "share" ? (
							<Check className="h-3.5 w-3.5 text-emerald-500" />
						) : (
							<Share2 className="h-3.5 w-3.5 text-primary" />
						)}
						<span>{copied === "share" ? m.common_copied() : m.client_share_config()}</span>
					</Button>
				</div>
			</div>

			{/* 2-Column Master-Detail Layout */}
			<div className="grid grid-cols-1 md:grid-cols-12 gap-3 flex-1 min-h-0 overflow-hidden">
				{/* Left Column: Saved Profiles List */}
				<div className="md:col-span-5 flex flex-col min-h-0 rounded-lg border border-border bg-card p-3 shadow-xs space-y-2">
					<div className="flex items-center justify-between pb-1.5 border-b border-border/50 flex-none">
						<span className="text-xs font-semibold text-foreground">
							{m.client_saved_profiles()}
						</span>
						<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4">
							{m.client_profile_count({ count: profiles.length })}
						</Badge>
					</div>

					<div className="flex-1 min-h-0 overflow-y-auto divide-y divide-border/40 pr-1 space-y-1">
						{profiles.length > 0 ? (
							profiles.map((p) => {
								const isSelected = p.id === selectedProfileId;
								return (
									<div
										key={p.id}
										onClick={() => handleSelectProfile(p.id)}
										className={cn(
											"flex items-center justify-between p-2 rounded-lg cursor-pointer transition-colors gap-2",
											isSelected ? "bg-primary/10 border border-primary/30" : "hover:bg-accent/50",
										)}
									>
										<div className="min-w-0 flex-1">
											<div className="flex items-center gap-1.5">
												<span className="font-semibold text-foreground truncate text-xs">
													{p.name}
												</span>
												{isSelected ? (
													<Badge className="bg-primary text-primary-foreground text-[9px] px-1 py-0 h-3.5">
														{m.client_current()}
													</Badge>
												) : null}
											</div>
											<div className="font-mono text-[10px] text-muted-foreground truncate">
												{p.server_addr} ({p.transport.toUpperCase()}) &bull; {m.client_local()}:{" "}
												{p.listen_addr}
											</div>
										</div>

										<div className="flex items-center gap-1 flex-none">
											<Button
												variant="ghost"
												size="icon-xs"
												onClick={(e) => {
													e.stopPropagation();
													void handleDeleteProfile(p.id);
												}}
												className="h-6 w-6 p-0 text-destructive hover:bg-destructive/10 cursor-pointer"
												title={m.client_delete_profile()}
											>
												<Trash2 className="h-3 w-3" />
											</Button>
										</div>
									</div>
								);
							})
						) : (
							<div className="py-8 text-center text-xs text-muted-foreground">
								{m.client_no_profiles()}
							</div>
						)}
					</div>
				</div>

				{/* Right Column: Configuration Form */}
				<div className="md:col-span-7 flex flex-col min-h-0 rounded-lg border border-border bg-card p-3 shadow-xs overflow-y-auto space-y-3">
					<div className="flex items-center justify-between pb-1.5 border-b border-border/50 flex-none">
						<span className="text-xs font-semibold text-foreground truncate">
							{m.client_edit_profile({ name: profileName || m.client_new() })}
						</span>
						<span className="text-[10px] text-muted-foreground">
							ID: {selectedProfileId || m.client_new()}
						</span>
					</div>

					<div className="space-y-2.5 flex-1">
						<div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
							<div className="space-y-1">
								<label className="text-[10px] uppercase font-bold text-muted-foreground">
									{m.client_profile_name()}
								</label>
								<Input
									value={profileName}
									onChange={(e) => setProfileName(e.target.value)}
									placeholder={m.client_profile_name_placeholder()}
									className="h-8 text-xs"
								/>
							</div>

							<div className="space-y-1">
								<label className="text-[10px] uppercase font-bold text-muted-foreground">
									{m.client_relay_address()}
								</label>
								<Input
									value={serverAddr}
									onChange={(e) => setServerAddr(e.target.value)}
									placeholder={m.client_relay_placeholder()}
									className="h-8 text-xs font-mono"
								/>
							</div>
						</div>

						<div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
							<div className="space-y-1">
								<label className="text-[10px] uppercase font-bold text-muted-foreground">
									{m.client_transport()}
								</label>
								<select
									aria-label={m.client_transport()}
									value={transport}
									onChange={(e) => {
										const val = e.target.value;
										setTransport(val);
										const matched = SUPPORTED_LINK_PROTOCOLS.find((item) => item.transport === val);
										if (matched) setLinkProtocol(matched.value);
									}}
									className="h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-ring"
								>
									<option value="auto">{m.transport_auto()}</option>
									<option value="webtransport">{m.transport_webtransport()}</option>
									<option value="quic">{m.transport_quic()}</option>
									<option value="tcp">{m.transport_tcp()}</option>
									<option value="kcp">{m.transport_kcp()}</option>
									<option value="websocket">{m.transport_websocket()}</option>
								</select>
							</div>

							<div className="space-y-1">
								<label className="text-[10px] uppercase font-bold text-muted-foreground">
									{m.client_listen_address()}
								</label>
								<Input
									value={listenAddr}
									onChange={(e) => setListenAddr(e.target.value)}
									placeholder="127.0.0.1:25565"
									className="h-8 text-xs font-mono"
								/>
							</div>
						</div>

						{/* Toggles */}
						<div className="space-y-2 pt-1">
							<div className="flex items-center justify-between rounded-lg border border-border/60 p-2 text-xs">
								<div>
									<div className="font-medium text-xs text-foreground">
										{m.client_lan_broadcast()}
									</div>
									<div className="text-[10px] text-muted-foreground">
										{m.client_lan_broadcast_hint()}
									</div>
								</div>
								<Switch checked={fakeLanBroadcast} onCheckedChange={setFakeLanBroadcast} />
							</div>

							<div className="flex items-center justify-between rounded-lg border border-border/60 p-2 text-xs">
								<div>
									<div className="font-medium text-xs text-foreground">
										{m.client_auto_connect()}
									</div>
									<div className="text-[10px] text-muted-foreground">
										{m.client_auto_connect_hint()}
									</div>
								</div>
								<Switch checked={autoConnect} onCheckedChange={setAutoConnect} />
							</div>

							<div className="flex items-center justify-between rounded-lg border border-border/60 p-2 text-xs">
								<div>
									<div className="font-medium text-xs text-foreground">
										{m.client_auto_connect_panel()}
									</div>
									<div className="text-[10px] text-muted-foreground">
										{m.client_auto_connect_panel_hint({ url: managementUrl })}
									</div>
								</div>
								<Switch checked={autoConnectPanel} onCheckedChange={setAutoConnectPanel} />
							</div>
						</div>
					</div>

					{/* Footer Controls */}
					<div className="flex items-center justify-between pt-2 border-t border-border/50 flex-none">
						{selectedProfileId ? (
							<Button
								variant="outline"
								size="sm"
								onClick={() => void handleDeleteProfile(selectedProfileId)}
								className="h-7 text-xs text-destructive hover:bg-destructive/10 cursor-pointer"
							>
								{m.client_delete_this_profile()}
							</Button>
						) : (
							<div />
						)}

						<Button
							size="sm"
							onClick={() => void handleSaveProfile()}
							className="h-7 text-xs gap-1 px-3.5 cursor-pointer"
						>
							<Check className="h-3.5 w-3.5" />
							<span>{m.client_save_profile()}</span>
						</Button>
					</div>
				</div>
			</div>
		</div>
	);
}
