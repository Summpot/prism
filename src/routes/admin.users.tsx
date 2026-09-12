import { createFileRoute } from "@tanstack/react-router";
import { Check, Copy, Key, Plus, Trash2, UserCheck, Users } from "lucide-react";
import { useMemo, useState } from "react";

import {
	Badge,
	EmptyState,
	ErrorBanner,
	PageHeader,
	RefreshButton,
	SearchInput,
	StateCard,
} from "@/components/ui";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import {
	createAuthToken,
	listAuthTokens,
	listManagedUsers,
	revokeAuthToken,
	type TokenRecord,
	updateManagedUser,
	type UserRecord,
} from "@/lib/managementApi";
import { useAdminQuery } from "@/hooks/useAdminQuery";
import { usePanelSession } from "@/lib/panelSession";
import { invalidateAdminQueries } from "@/lib/state/queryClient";
import { queryKeys } from "@/lib/state/queryKeys";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/admin/users")({
	component: AdminUsersPage,
});

function AdminUsersPage() {
	const { connection, ready, authSession: session } = usePanelSession();
	const [error, setError] = useState<string | null>(null);
	const [query, setQuery] = useState("");
	const [activeTab, setActiveTab] = useState<"users" | "tokens">("users");

	const [createTokenOpen, setCreateTokenOpen] = useState(false);
	const [tokenName, setTokenName] = useState("");
	const [tokenType, setTokenType] = useState<"client" | "admin" | "connector">("client");
	const [tokenExpiryDays, setTokenExpiryDays] = useState<number | undefined>(undefined);
	const [createdRawToken, setCreatedRawToken] = useState<string | null>(null);
	const [copiedToken, setCopiedToken] = useState(false);
	const [tokenCreating, setTokenCreating] = useState(false);

	const [editingUser, setEditingUser] = useState<UserRecord | null>(null);
	const [editRole, setEditRole] = useState<"admin" | "member" | "disabled">("member");
	const [editServiceRules, setEditServiceRules] = useState<string>("");
	const [userSaving, setUserSaving] = useState(false);

	const usersQuery = useAdminQuery(
		queryKeys.admin.users(connection),
		(conn) => listManagedUsers(conn).catch(() => [] as UserRecord[]),
		{ refetchInterval: 10_000 },
	);
	const tokensQuery = useAdminQuery(
		queryKeys.admin.tokens(connection),
		(conn) => listAuthTokens(conn).catch(() => [] as TokenRecord[]),
		{ refetchInterval: 10_000 },
	);
	const users = usersQuery.data ?? [];
	const tokens = tokensQuery.data ?? [];
	const loading = usersQuery.isFetching && !usersQuery.data;
	const fetchData = () => {
		void usersQuery.refetch();
		void tokensQuery.refetch();
	};

	const handleCreateToken = async (e: React.FormEvent) => {
		e.preventDefault();
		if (!connection || !tokenName.trim()) {
			return;
		}

		setTokenCreating(true);
		setError(null);
		try {
			const res = await createAuthToken(connection, {
				name: tokenName.trim(),
				token_type: tokenType,
				expires_in_days: tokenExpiryDays || null,
			});
			setCreatedRawToken(res.raw_token);
			setTokenName("");
			await invalidateAdminQueries();
			fetchData();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setTokenCreating(false);
		}
	};

	const handleRevokeToken = async (tokenId: string) => {
		if (!connection || !confirm(m.users_revoke_confirm())) {
			return;
		}
		try {
			await revokeAuthToken(connection, tokenId);
			await invalidateAdminQueries();
			fetchData();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		}
	};

	const handleSaveUser = async (e: React.FormEvent) => {
		e.preventDefault();
		if (!connection || !editingUser) {
			return;
		}

		setUserSaving(true);
		setError(null);
		try {
			const rules = editServiceRules
				.split(/[\n,]/)
				.map((r) => r.trim())
				.filter(Boolean);
			await updateManagedUser(connection, editingUser.id, {
				role: editRole,
				service_rules: rules,
			});
			setEditingUser(null);
			await invalidateAdminQueries();
			fetchData();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setUserSaving(false);
		}
	};

	const filteredUsers = useMemo(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return users;
		return users.filter(
			(u) =>
				u.username.toLowerCase().includes(needle) ||
				(u.display_name && u.display_name.toLowerCase().includes(needle)) ||
				u.service_rules.some((r) => r.toLowerCase().includes(needle)),
		);
	}, [query, users]);

	const filteredTokens = useMemo(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return tokens;
		return tokens.filter(
			(t) =>
				t.name.toLowerCase().includes(needle) ||
				t.id.toLowerCase().includes(needle) ||
				t.token_type.toLowerCase().includes(needle),
		);
	}, [query, tokens]);

	if (!ready) {
		return <StateCard label={m.common_restoring_session()} />;
	}

	if (!connection) {
		return <StateCard label={m.users_connect_panel()} />;
	}

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow={m.users_eyebrow()}
				title={m.users_title()}
				description={m.users_description()}
				actions={
					<div className="flex items-center gap-3">
						<RefreshButton onClick={fetchData} loading={loading} />
						{activeTab === "tokens" ? (
							<Button
								onClick={() => {
									setCreatedRawToken(null);
									setCreateTokenOpen(true);
								}}
							>
								<Plus className="h-4 w-4" />
								{m.users_generate_token()}
							</Button>
						) : null}
					</div>
				}
			/>

			{error ? <ErrorBanner message={error} /> : null}

			{session?.authenticated ? (
				<Card className="shadow-xs">
					<CardContent className="flex items-center justify-between gap-4 py-4">
						<div className="flex items-center gap-3">
							<Avatar>
								{session.avatar_url ? (
									<AvatarImage
										src={session.avatar_url}
										alt={session.username ?? m.users_avatar()}
									/>
								) : null}
								<AvatarFallback>
									<UserCheck className="h-4 w-4" />
								</AvatarFallback>
							</Avatar>
							<div>
								<div className="flex items-center gap-2">
									<span className="font-semibold">
										{session.display_name || session.username || m.users_authenticated_admin()}
									</span>
									<Badge variant={session.is_admin ? "success" : "default"}>
										{session.role ?? "admin"}
									</Badge>
								</div>
								<span className="text-xs text-muted-foreground">
									{session.username ? `@${session.username}` : m.users_connected_via_token()}
								</span>
							</div>
						</div>
						<div className="text-xs text-muted-foreground">
							{session.is_admin ? m.users_full_privileges() : m.users_standard_member()}
						</div>
					</CardContent>
				</Card>
			) : null}

			<Tabs value={activeTab} onValueChange={(value) => setActiveTab(value as "users" | "tokens")}>
				<TabsList>
					<TabsTrigger value="users">
						<Users className="h-4 w-4" />
						{m.users_tab_users({ count: users.length })}
					</TabsTrigger>
					<TabsTrigger value="tokens">
						<Key className="h-4 w-4" />
						{m.users_tab_tokens({ count: tokens.length })}
					</TabsTrigger>
				</TabsList>
			</Tabs>

			<div className="max-w-md">
				<SearchInput
					value={query}
					onChange={setQuery}
					placeholder={activeTab === "users" ? m.users_search_users() : m.users_search_tokens()}
				/>
			</div>

			{activeTab === "users" ? (
				filteredUsers.length === 0 ? (
					<EmptyState
						icon={<Users className="h-8 w-8" />}
						title={m.users_no_users_title()}
						description={query ? m.users_no_users_match() : m.users_no_users_hint()}
					/>
				) : (
					<div className="grid gap-4 md:grid-cols-2">
						{filteredUsers.map((user) => (
							<Card key={user.id} className="shadow-xs">
								<CardContent className="space-y-4 pt-0">
									<div className="flex items-start justify-between">
										<div className="flex items-center gap-3">
											<Avatar>
												{user.avatar_url ? (
													<AvatarImage src={user.avatar_url} alt={user.username} />
												) : null}
												<AvatarFallback>{user.username.slice(0, 2).toUpperCase()}</AvatarFallback>
											</Avatar>
											<div>
												<div className="flex items-center gap-2">
													<span className="font-medium">{user.display_name || user.username}</span>
													<Badge
														variant={
															user.role === "admin"
																? "success"
																: user.role === "disabled"
																	? "danger"
																	: "default"
														}
													>
														{user.role}
													</Badge>
												</div>
												<span className="text-xs text-muted-foreground">@{user.username}</span>
											</div>
										</div>
										<Button
											variant="outline"
											size="xs"
											onClick={() => {
												setEditingUser(user);
												setEditRole(user.role);
												setEditServiceRules(user.service_rules.join("\n"));
											}}
										>
											{m.common_edit()}
										</Button>
									</div>
									<div className="border-t border-border pt-3">
										<span className="text-xs font-medium text-muted-foreground">
											{m.users_acl_label()}
										</span>
										<div className="mt-1.5 flex flex-wrap gap-1.5">
											{user.service_rules.length === 0 ? (
												<span className="text-xs text-muted-foreground italic">
													{m.users_no_rules()}
												</span>
											) : (
												user.service_rules.map((rule) => (
													<Badge key={rule} tone="info" className="font-mono normal-case">
														{rule}
													</Badge>
												))
											)}
										</div>
									</div>
								</CardContent>
							</Card>
						))}
					</div>
				)
			) : null}

			{activeTab === "tokens" ? (
				filteredTokens.length === 0 ? (
					<EmptyState
						icon={<Key className="h-8 w-8" />}
						title={m.users_no_tokens_title()}
						description={m.users_no_tokens_hint()}
					/>
				) : (
					<div className="space-y-3">
						{filteredTokens.map((token) => (
							<Card key={token.id} className="shadow-xs">
								<CardContent className="flex items-center justify-between gap-4 py-4">
									<div className="space-y-1">
										<div className="flex flex-wrap items-center gap-2">
											<span className="font-semibold">{token.name}</span>
											<Badge
												variant={
													token.token_type === "admin"
														? "success"
														: token.token_type === "connector"
															? "info"
															: "default"
												}
											>
												{token.token_type}
											</Badge>
											<span className="font-mono text-xs text-muted-foreground">{token.id}</span>
										</div>
										<div className="text-xs text-muted-foreground">
											{m.users_created({
												date: new Date(token.created_at_unix_ms).toLocaleString(),
											})}
											{token.last_used_unix_ms > 0
												? m.users_last_used({
														date: new Date(token.last_used_unix_ms).toLocaleString(),
													})
												: m.users_never_used()}
										</div>
									</div>
									<Button
										variant="destructive"
										size="xs"
										onClick={() => void handleRevokeToken(token.id)}
									>
										<Trash2 className="h-3.5 w-3.5" />
										{m.users_revoke()}
									</Button>
								</CardContent>
							</Card>
						))}
					</div>
				)
			) : null}

			<Dialog
				open={createTokenOpen}
				onOpenChange={(open) => {
					setCreateTokenOpen(open);
					if (!open) {
						setCreatedRawToken(null);
					}
				}}
			>
				<DialogContent className="sm:max-w-lg" showCloseButton>
					<DialogHeader>
						<DialogTitle>{m.users_token_modal_title()}</DialogTitle>
						<DialogDescription>{m.users_token_modal_description()}</DialogDescription>
					</DialogHeader>
					{createdRawToken ? (
						<div className="space-y-4">
							<div className="rounded-lg border border-emerald-500/20 bg-emerald-500/10 p-4">
								<div className="flex items-center gap-2 text-sm font-semibold text-emerald-600 dark:text-emerald-400">
									<Check className="h-4 w-4" /> {m.users_token_created()}
								</div>
								<p className="mt-2 text-xs text-muted-foreground">{m.users_token_copy_hint()}</p>
								<div className="mt-3 flex items-center justify-between gap-2 rounded-lg border border-border bg-muted/40 p-2.5 font-mono text-xs">
									<span className="truncate">{createdRawToken}</span>
									<Button
										type="button"
										variant="outline"
										size="xs"
										onClick={() => {
											navigator.clipboard.writeText(createdRawToken);
											setCopiedToken(true);
											setTimeout(() => setCopiedToken(false), 2000);
										}}
									>
										{copiedToken ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
										{copiedToken ? m.common_copied() : m.common_copy()}
									</Button>
								</div>
							</div>
							<DialogFooter>
								<Button
									type="button"
									onClick={() => {
										setCreateTokenOpen(false);
										setCreatedRawToken(null);
									}}
								>
									{m.common_done()}
								</Button>
							</DialogFooter>
						</div>
					) : (
						<form onSubmit={handleCreateToken} className="space-y-4">
							<div className="space-y-1.5">
								<Label htmlFor="token-name">{m.users_token_name()}</Label>
								<Input
									id="token-name"
									value={tokenName}
									onChange={(e) => setTokenName(e.target.value)}
									placeholder={m.users_token_name_placeholder()}
									required
								/>
							</div>
							<div className="space-y-1.5">
								<Label htmlFor="token-type">{m.users_token_type()}</Label>
								<NativeSelect
									id="token-type"
									className="w-full"
									value={tokenType}
									onChange={(e) => setTokenType(e.target.value as "client" | "admin" | "connector")}
								>
									<NativeSelectOption value="client">
										{m.users_token_type_client()}
									</NativeSelectOption>
									<NativeSelectOption value="connector">
										{m.users_token_type_connector()}
									</NativeSelectOption>
									<NativeSelectOption value="admin">
										{m.users_token_type_admin()}
									</NativeSelectOption>
								</NativeSelect>
							</div>
							<div className="space-y-1.5">
								<Label htmlFor="token-expiry">{m.users_token_expiry()}</Label>
								<Input
									id="token-expiry"
									type="number"
									min="1"
									value={tokenExpiryDays ?? ""}
									onChange={(e) =>
										setTokenExpiryDays(e.target.value ? Number(e.target.value) : undefined)
									}
									placeholder={m.users_token_expiry_placeholder()}
								/>
							</div>
							<DialogFooter>
								<Button type="button" variant="outline" onClick={() => setCreateTokenOpen(false)}>
									{m.common_cancel()}
								</Button>
								<Button type="submit" disabled={tokenCreating || !tokenName.trim()}>
									{tokenCreating ? m.users_token_generating() : m.users_token_generate()}
								</Button>
							</DialogFooter>
						</form>
					)}
				</DialogContent>
			</Dialog>

			<Dialog open={Boolean(editingUser)} onOpenChange={(open) => !open && setEditingUser(null)}>
				<DialogContent className="sm:max-w-lg" showCloseButton>
					<DialogHeader>
						<DialogTitle>
							{editingUser
								? m.users_edit_title({ username: editingUser.username })
								: m.common_edit()}
						</DialogTitle>
						<DialogDescription>{m.users_edit_description()}</DialogDescription>
					</DialogHeader>
					{editingUser ? (
						<form onSubmit={handleSaveUser} className="space-y-4">
							<div className="space-y-1.5">
								<Label htmlFor="edit-role">{m.users_role()}</Label>
								<NativeSelect
									id="edit-role"
									className="w-full"
									value={editRole}
									onChange={(e) => setEditRole(e.target.value as "admin" | "member" | "disabled")}
								>
									<NativeSelectOption value="admin">{m.users_role_admin()}</NativeSelectOption>
									<NativeSelectOption value="member">{m.users_role_member()}</NativeSelectOption>
									<NativeSelectOption value="disabled">
										{m.users_role_disabled()}
									</NativeSelectOption>
								</NativeSelect>
							</div>
							<div className="space-y-1.5">
								<Label htmlFor="edit-rules">{m.users_rules_label()}</Label>
								<Textarea
									id="edit-rules"
									rows={4}
									value={editServiceRules}
									onChange={(e) => setEditServiceRules(e.target.value)}
									placeholder={"mc-*\nsecret-db\n*"}
									className="font-mono text-xs"
								/>
								<p className="text-[11px] text-muted-foreground">
									{m.users_rules_hint_prefix()}
									<code className="text-primary">mc-*</code>
									{m.users_rules_hint_mid()}
									<code className="text-primary">*</code>
									{m.users_rules_hint_tail()}
								</p>
							</div>
							<DialogFooter>
								<Button type="button" variant="outline" onClick={() => setEditingUser(null)}>
									{m.common_cancel()}
								</Button>
								<Button type="submit" disabled={userSaving}>
									{userSaving ? m.common_saving() : m.users_save_changes()}
								</Button>
							</DialogFooter>
						</form>
					) : null}
				</DialogContent>
			</Dialog>
		</div>
	);
}
