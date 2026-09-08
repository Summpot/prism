import { Plug, RotateCcw, X } from "lucide-react";

import { Github } from "@/components/icons/Github";

import { useClient } from "@/context/ClientContext";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardFooter,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";

export function ClientModals() {
	const {
		importModalOpen,
		setImportModalOpen,
		importUrl,
		setImportUrl,
		importError,
		setImportError,
		handleImportLink,

		loginModalOpen,
		setLoginModalOpen,
		serverAddr,
		checkingProviders,
		providersResult,
		providersError,
		authServerUrl,
		authError,
		setAuthError,
		setProvidersError,
		oauthLoading,
		oauthWaitingCallback,
		setOauthWaitingCallback,
		oauthExchanging,
		manualCallbackInput,
		setManualCallbackInput,
		startGitHubAuthWithUrl,
		handleRedetectProviders,
		handleConnectFromLink,
	} = useClient();

	return (
		<>
			{/* Import Modal */}
			{importModalOpen ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-3 sm:p-4 backdrop-blur-xs">
					<Card className="w-full max-w-sm sm:max-w-lg shadow-xl">
						<CardHeader>
							<CardTitle>导入 Prism 节点配置</CardTitle>
							<CardDescription>
								粘贴 <code className="text-primary">prism://</code> 链接或服务端地址
							</CardDescription>
						</CardHeader>
						<CardContent className="space-y-3">
							<Input
								value={importUrl}
								onChange={(e) => setImportUrl(e.target.value)}
								placeholder="quic://play.example.com:7000 或 prism://play.example.com:7000?name=..."
							/>
							{importError ? <p className="text-xs text-destructive">{importError}</p> : null}
						</CardContent>
						<CardFooter className="flex justify-end gap-2 border-t border-border pt-4">
							<Button
								variant="outline"
								onClick={() => {
									setImportModalOpen(false);
									setImportError(null);
								}}
							>
								取消
							</Button>
							<Button onClick={handleImportLink}>导入并应用</Button>
						</CardFooter>
					</Card>
				</div>
			) : null}

			{/* 选择登录方式对话框 (Select Login Method Modal) */}
			{loginModalOpen ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-3 sm:p-4 backdrop-blur-xs">
					<Card className="w-full max-w-sm sm:max-w-lg shadow-xl border-border bg-card">
						<CardHeader className="flex flex-row items-center justify-between pb-3">
							<div className="flex items-center gap-2 min-w-0">
								<Plug className="h-5 w-5 text-primary flex-none" />
								<div className="min-w-0">
									<CardTitle className="text-sm sm:text-base font-bold">选择登录方式</CardTitle>
									<CardDescription className="text-xs truncate">
										远端节点：
										<code className="text-foreground font-mono">{serverAddr || "未指定"}</code>
									</CardDescription>
								</div>
							</div>
							<Button
								variant="ghost"
								size="icon-xs"
								onClick={() => {
									setLoginModalOpen(false);
									setAuthError(null);
									setProvidersError(null);
									setOauthWaitingCallback(false);
									setManualCallbackInput("");
								}}
							>
								<X className="h-4 w-4" />
							</Button>
						</CardHeader>
						<CardContent className="space-y-3.5">
							{checkingProviders ? (
								<div className="flex flex-col items-center justify-center py-8 text-center text-muted-foreground gap-2.5">
									<RotateCcw className="h-7 w-7 animate-spin text-primary" />
									<p className="text-xs">正在探测远端支持的登录方式...</p>
								</div>
							) : oauthExchanging ? (
								<div className="flex flex-col items-center justify-center py-8 text-center text-muted-foreground gap-3">
									<RotateCcw className="h-8 w-8 animate-spin text-primary" />
									<div className="space-y-1">
										<p className="text-sm font-semibold text-foreground">
											正在兑换 GitHub 授权凭证...
										</p>
										<p className="text-xs text-muted-foreground">
											已接收到授权回调，正在与远端节点验证并申请访问令牌
										</p>
									</div>
								</div>
							) : oauthWaitingCallback ? (
								<div className="space-y-4 py-2">
									<div className="rounded-xl border border-primary/30 bg-primary/10 p-5 flex flex-col items-center text-center gap-3">
										<div className="relative flex items-center justify-center">
											<RotateCcw className="h-9 w-9 animate-spin text-primary" />
											<Github className="absolute h-4 w-4 text-primary" />
										</div>
										<div className="space-y-1">
											<p className="text-sm font-bold text-foreground">等待 GitHub 授权完成</p>
											<p className="text-xs text-muted-foreground leading-relaxed max-w-sm">
												已在默认浏览器中打开 GitHub
												授权页面。完成授权后，系统将自动唤起客户端完成登录并自动关闭此对话框。
											</p>
										</div>
										<Button
											variant="outline"
											size="sm"
											onClick={() => void startGitHubAuthWithUrl(authServerUrl, serverAddr)}
											disabled={oauthLoading}
											className="text-xs h-7.5 gap-1.5 mt-1"
										>
											<RotateCcw className="h-3.5 w-3.5" />
											重新打开授权页面
										</Button>
									</div>

									{authError ? (
										<div className="rounded-lg border border-destructive/30 bg-destructive/10 p-2.5 text-xs text-destructive">
											{authError}
										</div>
									) : null}

									{/* Manual paste fallback if deep-link fails */}
									<div className="rounded-lg border border-border/70 bg-card/60 p-3 space-y-2">
										<div className="flex items-center justify-between">
											<span className="text-[11px] font-semibold text-foreground">
												未自动唤起桌面端？手动粘贴回调链接
											</span>
										</div>
										<div className="flex items-center gap-2">
											<Input
												value={manualCallbackInput}
												onChange={(e) => setManualCallbackInput(e.target.value)}
												placeholder="粘贴浏览器最终跳转的 prism:// 链接或验证码"
												className="h-8 text-xs font-mono"
											/>
											<Button
												size="sm"
												className="h-8 text-xs flex-none px-3"
												disabled={!manualCallbackInput.trim() || oauthLoading}
												onClick={() => {
													void handleConnectFromLink(manualCallbackInput.trim());
												}}
											>
												验证
											</Button>
										</div>
									</div>
								</div>
							) : (
								<div className="space-y-3">
									{providersError ? (
										<div className="rounded-lg border border-destructive/30 bg-destructive/10 p-2.5 text-xs text-destructive flex items-start justify-between gap-2">
											<span>{providersError}</span>
											<Button
												variant="ghost"
												size="icon-xs"
												onClick={() => void handleRedetectProviders()}
												className="h-5 w-5 text-destructive hover:bg-destructive/20 -mr-1 -mt-0.5"
												title="重试"
											>
												<RotateCcw className="h-3 w-3" />
											</Button>
										</div>
									) : null}

									{authError ? (
										<div className="rounded-lg border border-destructive/30 bg-destructive/10 p-2.5 text-xs text-destructive">
											{authError}
										</div>
									) : null}

									<div className="space-y-2">
										{/* GitHub OAuth Option */}
										{providersResult?.github_enabled !== false ? (
											<button
												type="button"
												disabled={oauthLoading}
												onClick={() => void startGitHubAuthWithUrl(authServerUrl, serverAddr)}
												className="flex w-full items-center justify-between rounded-xl border border-border bg-card p-3 text-left transition hover:border-primary hover:bg-accent/40 cursor-pointer disabled:opacity-50"
											>
												<div className="flex items-center gap-3">
													<div className="flex h-9 w-9 items-center justify-center rounded-lg bg-foreground text-background">
														<Github className="h-5 w-5" />
													</div>
													<div>
														<div className="flex items-center gap-1.5">
															<span className="text-xs sm:text-sm font-semibold text-foreground">
																GitHub 授权登录
															</span>
															<Badge
																variant="secondary"
																className="text-[9px] px-1 py-0 h-4 uppercase"
															>
																推荐
															</Badge>
														</div>
														<p className="text-[11px] text-muted-foreground">
															在浏览器中安全完成 GitHub 账号认证并绑定权限
														</p>
													</div>
												</div>
												<div className="text-muted-foreground">&rarr;</div>
											</button>
										) : null}

										{/* Anonymous / Direct Connect Option */}
										<button
											type="button"
											onClick={() => {
												setLoginModalOpen(false);
												void handleConnectFromLink();
											}}
											className="flex w-full items-center justify-between rounded-xl border border-border bg-card p-3 text-left transition hover:border-primary hover:bg-accent/40 cursor-pointer"
										>
											<div className="flex items-center gap-3">
												<div className="flex h-9 w-9 items-center justify-center rounded-lg bg-muted text-foreground">
													<Plug className="h-5 w-5" />
												</div>
												<div>
													<div className="flex items-center gap-1.5">
														<span className="text-xs sm:text-sm font-semibold text-foreground">
															直接匿名连接
														</span>
													</div>
													<p className="text-[11px] text-muted-foreground">
														使用公共/访客权限直接接入隧道服务
													</p>
												</div>
											</div>
											<div className="text-muted-foreground">&rarr;</div>
										</button>
									</div>
								</div>
							)}
						</CardContent>
					</Card>
				</div>
			) : null}
		</>
	);
}
