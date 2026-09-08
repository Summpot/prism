import { Check, Download, Plus, Share2, Trash2 } from "lucide-react";

import { useClient } from "@/context/ClientContext";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { SUPPORTED_LINK_PROTOCOLS } from "@/lib/prismLink";
import { cn } from "@/lib/utils";

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
		setLinkProtocol,
		listenAddr,
		setListenAddr,
		fakeLanBroadcast,
		setFakeLanBroadcast,
		autoConnectPanel,
		setAutoConnectPanel,
		managementUrl,
		handleSaveProfile,
		handleDeleteProfile,
		setImportModalOpen,
		handleShareLink,
		copied,
	} = useClient();

	return (
		<div className="mx-auto flex h-full w-full max-w-5xl flex-1 min-h-0 flex-col gap-2.5 p-3 sm:p-4 overflow-hidden">
			{/* Header Bar */}
			<div className="flex flex-none select-none items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 shadow-xs">
				<div className="flex items-center gap-2 min-w-0">
					<div className="min-w-0">
						<h1 className="truncate text-xs sm:text-sm font-bold tracking-tight text-foreground">
							隧道配置与配置集
						</h1>
						<p className="truncate text-[10px] text-muted-foreground hidden sm:block">
							管理连接配置集 (Profiles) 与底层网络传输协议参数
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
							setProfileName("New Profile");
							setServerAddr("127.0.0.1:7000");
							setTransport("quic");
							setListenAddr("127.0.0.1:25565");
							setFakeLanBroadcast(true);
							handleSelectProfile(id);
						}}
						className="h-7 gap-1 text-xs px-2.5 cursor-pointer"
					>
						<Plus className="h-3.5 w-3.5" />
						<span>新建配置集</span>
					</Button>

					<Button
						variant="outline"
						size="xs"
						onClick={() => setImportModalOpen(true)}
						className="h-7 gap-1 text-xs px-2.5 cursor-pointer"
					>
						<Download className="h-3.5 w-3.5 text-primary" />
						<span>导入链接</span>
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
						<span>{copied === "share" ? "已复制" : "分享配置"}</span>
					</Button>
				</div>
			</div>

			{/* 2-Column Master-Detail Layout */}
			<div className="grid grid-cols-1 md:grid-cols-12 gap-3 flex-1 min-h-0 overflow-hidden">
				{/* Left Column: Saved Profiles List */}
				<div className="md:col-span-5 flex flex-col min-h-0 rounded-lg border border-border bg-card p-3 shadow-xs space-y-2">
					<div className="flex items-center justify-between pb-1.5 border-b border-border/50 flex-none">
						<span className="text-xs font-semibold text-foreground">已保存配置集</span>
						<Badge variant="outline" className="text-[10px] px-1.5 py-0 h-4">
							{profiles.length} 个配置
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
														当前
													</Badge>
												) : null}
											</div>
											<div className="font-mono text-[10px] text-muted-foreground truncate">
												{p.server_addr} ({p.transport.toUpperCase()}) &bull; 本地: {p.listen_addr}
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
												title="删除配置集"
											>
												<Trash2 className="h-3 w-3" />
											</Button>
										</div>
									</div>
								);
							})
						) : (
							<div className="py-8 text-center text-xs text-muted-foreground">
								暂无已保存配置集，点击上方“新建配置集”或“导入链接”创建。
							</div>
						)}
					</div>
				</div>

				{/* Right Column: Configuration Form */}
				<div className="md:col-span-7 flex flex-col min-h-0 rounded-lg border border-border bg-card p-3 shadow-xs overflow-y-auto space-y-3">
					<div className="flex items-center justify-between pb-1.5 border-b border-border/50 flex-none">
						<span className="text-xs font-semibold text-foreground truncate">
							编辑配置: {profileName || "未命名配置"}
						</span>
						<span className="text-[10px] text-muted-foreground">
							ID: {selectedProfileId || "新建"}
						</span>
					</div>

					<div className="space-y-2.5 flex-1">
						<div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
							<div className="space-y-1">
								<label className="text-[10px] uppercase font-bold text-muted-foreground">
									配置集名称
								</label>
								<Input
									value={profileName}
									onChange={(e) => setProfileName(e.target.value)}
									placeholder="例如: 我的游戏服务器"
									className="h-8 text-xs"
								/>
							</div>

							<div className="space-y-1">
								<label className="text-[10px] uppercase font-bold text-muted-foreground">
									远端中继服务器地址
								</label>
								<Input
									value={serverAddr}
									onChange={(e) => setServerAddr(e.target.value)}
									placeholder="relay.example.com:7000"
									className="h-8 text-xs font-mono"
								/>
							</div>
						</div>

						<div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
							<div className="space-y-1">
								<label className="text-[10px] uppercase font-bold text-muted-foreground">
									传输协议 (Transport)
								</label>
								<select
									aria-label="传输协议"
									value={transport}
									onChange={(e) => {
										const val = e.target.value;
										setTransport(val);
										const matched = SUPPORTED_LINK_PROTOCOLS.find((item) => item.transport === val);
										if (matched) setLinkProtocol(matched.value);
									}}
									className="h-8 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-ring"
								>
									<option value="quic">QUIC (极速抗丢包，推荐)</option>
									<option value="kcp">KCP (低延迟 UDP)</option>
									<option value="tcp">TCP (标准流传输)</option>
									<option value="websocket">WebSocket (穿透受限网络)</option>
								</select>
							</div>

							<div className="space-y-1">
								<label className="text-[10px] uppercase font-bold text-muted-foreground">
									本地监听端口 / Ingress
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
										Minecraft 局域网广播 (LAN Discovery)
									</div>
									<div className="text-[10px] text-muted-foreground">
										在局域网内自动广播游戏服务，便于客户端发现
									</div>
								</div>
								<Switch checked={fakeLanBroadcast} onCheckedChange={setFakeLanBroadcast} />
							</div>

							<div className="flex items-center justify-between rounded-lg border border-border/60 p-2 text-xs">
								<div>
									<div className="font-medium text-xs text-foreground">
										控制面板自动连接 (Auto-Connect Panel)
									</div>
									<div className="text-[10px] text-muted-foreground">
										自动同步管理面板与鉴权状态 ({managementUrl})
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
								删除此配置
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
							<span>保存配置集</span>
						</Button>
					</div>
				</div>
			</div>
		</div>
	);
}
