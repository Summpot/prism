import { createFileRoute } from "@tanstack/react-router";
import { Check, Copy, Key, Plus, Trash2, UserCheck, Users } from "lucide-react";
import { useCallback, useMemo, useState } from "react";

import {
	Badge,
	EmptyState,
	ErrorBanner,
	fieldClassName,
	PageHeader,
	PrimaryButton,
	RefreshButton,
	SearchInput,
	StateCard,
} from "@/components/ui";
import {
	type AuthSessionResponse,
	createAuthToken,
	getAuthSession,
	listAuthTokens,
	listManagedUsers,
	revokeAuthToken,
	type TokenRecord,
	updateManagedUser,
	type UserRecord,
} from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";
import { usePolling } from "@/lib/usePolling";

export const Route = createFileRoute("/users")({
	component: UsersPage,
});

function UsersPage() {
	const { connection, ready } = usePanelSession();
	const [users, setUsers] = useState<UserRecord[]>([]);
	const [tokens, setTokens] = useState<TokenRecord[]>([]);
	const [session, setSession] = useState<AuthSessionResponse | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [query, setQuery] = useState("");
	const [activeTab, setActiveTab] = useState<"users" | "tokens">("users");

	// Token creation modal state
	const [createTokenOpen, setCreateTokenOpen] = useState(false);
	const [tokenName, setTokenName] = useState("");
	const [tokenType, setTokenType] = useState<"client" | "admin" | "connector">("client");
	const [tokenExpiryDays, setTokenExpiryDays] = useState<number | undefined>(undefined);
	const [createdRawToken, setCreatedRawToken] = useState<string | null>(null);
	const [copiedToken, setCopiedToken] = useState(false);
	const [tokenCreating, setTokenCreating] = useState(false);

	// User editing modal state
	const [editingUser, setEditingUser] = useState<UserRecord | null>(null);
	const [editRole, setEditRole] = useState<"admin" | "member" | "disabled">("member");
	const [editServiceRules, setEditServiceRules] = useState<string>("");
	const [userSaving, setUserSaving] = useState(false);

	const fetchData = useCallback(() => {
		if (!connection) {
			setUsers([]);
			setTokens([]);
			setSession(null);
			return;
		}

		setLoading(true);
		setError(null);

		Promise.allSettled([
			listManagedUsers(connection),
			listAuthTokens(connection),
			getAuthSession(connection),
		])
			.then(([usersRes, tokensRes, sessionRes]) => {
				if (usersRes.status === "fulfilled") {
					setUsers(usersRes.value);
				}
				if (tokensRes.status === "fulfilled") {
					setTokens(tokensRes.value);
				}
				if (sessionRes.status === "fulfilled") {
					setSession(sessionRes.value);
				}
			})
			.catch((err) => {
				setError(err instanceof Error ? err.message : String(err));
			})
			.finally(() => {
				setLoading(false);
			});
	}, [connection]);

	usePolling(fetchData, 10_000, Boolean(connection));

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
			fetchData();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setTokenCreating(false);
		}
	};

	const handleRevokeToken = async (tokenId: string) => {
		if (!connection || !confirm("Are you sure you want to revoke this token?")) {
			return;
		}
		try {
			await revokeAuthToken(connection, tokenId);
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
		return <StateCard label="Restoring session…" />;
	}

	if (!connection) {
		return (
			<StateCard label="Connect the panel to a management node before managing users and access." />
		);
	}

	return (
		<div className="space-y-6">
			<PageHeader
				eyebrow="Security & Access Control"
				title="Users & Access Management"
				description="Manage authenticated OAuth users, fine-grained ACL service rules, and scoped tokens."
				actions={
					<div className="flex items-center gap-3">
						<RefreshButton onClick={fetchData} loading={loading} />
						{activeTab === "tokens" ? (
							<PrimaryButton
								onClick={() => {
									setCreatedRawToken(null);
									setCreateTokenOpen(true);
								}}
							>
								<Plus className="h-4 w-4" />
								Generate token
							</PrimaryButton>
						) : null}
					</div>
				}
			/>

			{error ? <ErrorBanner message={error} /> : null}

			{/* Session Identity Banner */}
			{session?.authenticated ? (
				<div className="flex items-center justify-between rounded-2xl border border-white/10 bg-slate-900/60 p-4">
					<div className="flex items-center gap-3">
						{session.avatar_url ? (
							<img
								src={session.avatar_url}
								alt={session.username ?? "Avatar"}
								className="h-10 w-10 rounded-full border border-white/20"
							/>
						) : (
							<div className="flex h-10 w-10 items-center justify-center rounded-full border border-cyan-400/30 bg-cyan-400/10 text-cyan-300">
								<UserCheck className="h-5 w-5" />
							</div>
						)}
						<div>
							<div className="flex items-center gap-2">
								<span className="font-semibold text-white">
									{session.display_name || session.username || "Authenticated Admin"}
								</span>
								<Badge variant={session.is_admin ? "success" : "default"}>
									{session.role ?? "admin"}
								</Badge>
							</div>
							<span className="text-xs text-slate-400">
								{session.username ? `@${session.username}` : "Connected via token"}
							</span>
						</div>
					</div>
					<div className="text-xs text-slate-400">
						{session.is_admin ? "Full management privileges" : "Standard member"}
					</div>
				</div>
			) : null}

			{/* Navigation Tabs */}
			<div className="flex items-center gap-2 border-b border-white/8 pb-4">
				<button
					type="button"
					onClick={() => setActiveTab("users")}
					className={`flex items-center gap-2 rounded-xl px-4 py-2 text-sm font-medium transition ${
						activeTab === "users"
							? "bg-cyan-500/15 text-cyan-300 border border-cyan-400/30"
							: "text-slate-400 hover:text-white"
					}`}
				>
					<Users className="h-4 w-4" />
					Users ({users.length})
				</button>
				<button
					type="button"
					onClick={() => setActiveTab("tokens")}
					className={`flex items-center gap-2 rounded-xl px-4 py-2 text-sm font-medium transition ${
						activeTab === "tokens"
							? "bg-cyan-500/15 text-cyan-300 border border-cyan-400/30"
							: "text-slate-400 hover:text-white"
					}`}
				>
					<Key className="h-4 w-4" />
					Tokens ({tokens.length})
				</button>
			</div>

			{/* Search */}
			<div className="max-w-md">
				<SearchInput
					value={query}
					onChange={setQuery}
					placeholder={
						activeTab === "users" ? "Search users by name or rule…" : "Search tokens by name or ID…"
					}
				/>
			</div>

			{/* Users Tab */}
			{activeTab === "users" ? (
				filteredUsers.length === 0 ? (
					<EmptyState
						icon={<Users className="h-8 w-8 text-slate-500" />}
						title="No users found"
						description={
							query
								? "No users match your filter."
								: "Users will automatically appear here when they sign in with GitHub OAuth."
						}
					/>
				) : (
					<div className="grid gap-4 md:grid-cols-2">
						{filteredUsers.map((user) => (
							<div
								key={user.id}
								className="rounded-2xl border border-white/8 bg-slate-950/70 p-5 shadow-lg"
							>
								<div className="flex items-start justify-between">
									<div className="flex items-center gap-3">
										{user.avatar_url ? (
											<img
												src={user.avatar_url}
												alt={user.username}
												className="h-10 w-10 rounded-full border border-white/20"
											/>
										) : (
											<div className="flex h-10 w-10 items-center justify-center rounded-full bg-white/10 text-white font-semibold">
												{user.username.slice(0, 2).toUpperCase()}
											</div>
										)}
										<div>
											<div className="flex items-center gap-2">
												<span className="font-medium text-white">
													{user.display_name || user.username}
												</span>
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
											<span className="text-xs text-slate-400">@{user.username}</span>
										</div>
									</div>

									<button
										type="button"
										onClick={() => {
											setEditingUser(user);
											setEditRole(user.role);
											setEditServiceRules(user.service_rules.join("\n"));
										}}
										className="rounded-xl border border-white/10 px-3 py-1.5 text-xs text-slate-300 transition hover:bg-white/5"
									>
										Edit
									</button>
								</div>

								<div className="mt-4 border-t border-white/6 pt-3">
									<span className="text-xs font-medium text-slate-400">
										Allowed Service Rules (ACL):
									</span>
									<div className="mt-1.5 flex flex-wrap gap-1.5">
										{user.service_rules.length === 0 ? (
											<span className="text-xs text-slate-500 italic">
												No rules (access blocked)
											</span>
										) : (
											user.service_rules.map((rule) => (
												<span
													key={rule}
													className="rounded-lg border border-cyan-400/20 bg-cyan-400/10 px-2.5 py-0.5 font-mono text-xs text-cyan-300"
												>
													{rule}
												</span>
											))
										)}
									</div>
								</div>
							</div>
						))}
					</div>
				)
			) : null}

			{/* Tokens Tab */}
			{activeTab === "tokens" ? (
				filteredTokens.length === 0 ? (
					<EmptyState
						icon={<Key className="h-8 w-8 text-slate-500" />}
						title="No tokens found"
						description="Generate client, admin, or connector tokens to authenticate nodes and sidecars."
					/>
				) : (
					<div className="space-y-3">
						{filteredTokens.map((token) => (
							<div
								key={token.id}
								className="flex items-center justify-between rounded-2xl border border-white/8 bg-slate-950/70 p-4"
							>
								<div className="space-y-1">
									<div className="flex items-center gap-2">
										<span className="font-semibold text-white">{token.name}</span>
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
										<span className="font-mono text-xs text-slate-500">{token.id}</span>
									</div>
									<div className="text-xs text-slate-400">
										Created: {new Date(token.created_at_unix_ms).toLocaleString()}
										{token.last_used_unix_ms > 0
											? ` · Last used: ${new Date(token.last_used_unix_ms).toLocaleString()}`
											: " · Never used"}
									</div>
								</div>

								<button
									type="button"
									onClick={() => handleRevokeToken(token.id)}
									className="flex items-center gap-1.5 rounded-xl border border-red-400/20 bg-red-400/10 px-3 py-1.5 text-xs text-red-200 transition hover:bg-red-400/20"
								>
									<Trash2 className="h-3.5 w-3.5" />
									Revoke
								</button>
							</div>
						))}
					</div>
				)
			) : null}

			{/* Create Token Modal */}
			{createTokenOpen ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 p-4 backdrop-blur-sm">
					<div className="w-full max-w-lg rounded-[2rem] border border-white/10 bg-slate-900 p-6 shadow-2xl">
						<h3 className="text-lg font-semibold text-white">Generate Scoped Token</h3>
						<p className="mt-1 text-sm text-slate-400">
							Create an authentication token for Prism clients, admin API, or connector daemons.
						</p>

						{createdRawToken ? (
							<div className="mt-5 space-y-4">
								<div className="rounded-xl border border-emerald-400/30 bg-emerald-500/10 p-4">
									<div className="flex items-center gap-2 text-sm font-semibold text-emerald-300">
										<Check className="h-4 w-4" /> Token created successfully!
									</div>
									<p className="mt-2 text-xs text-emerald-200">
										Please copy this token now. For security, it will not be shown again:
									</p>
									<div className="mt-3 flex items-center justify-between rounded-lg border border-emerald-400/30 bg-black/40 p-2.5 font-mono text-xs text-emerald-100">
										<span className="truncate">{createdRawToken}</span>
										<button
											type="button"
											onClick={() => {
												navigator.clipboard.writeText(createdRawToken);
												setCopiedToken(true);
												setTimeout(() => setCopiedToken(false), 2000);
											}}
											className="ml-2 flex items-center gap-1 rounded bg-emerald-400/20 px-2 py-1 text-xs text-emerald-200 hover:bg-emerald-400/30"
										>
											{copiedToken ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
											{copiedToken ? "Copied" : "Copy"}
										</button>
									</div>
								</div>

								<div className="flex justify-end">
									<button
										type="button"
										onClick={() => {
											setCreateTokenOpen(false);
											setCreatedRawToken(null);
										}}
										className="rounded-xl bg-cyan-500 px-4 py-2 text-sm font-semibold text-white hover:bg-cyan-400 transition"
									>
										Done
									</button>
								</div>
							</div>
						) : (
							<form onSubmit={handleCreateToken} className="mt-5 space-y-4">
								<label className="block space-y-1.5">
									<span className="text-xs font-medium text-slate-300">Token Name</span>
									<input
										value={tokenName}
										onChange={(e) => setTokenName(e.target.value)}
										placeholder="e.g. My Laptop Client or Production Connector"
										className={fieldClassName}
										required
									/>
								</label>

								<label className="block space-y-1.5">
									<span className="text-xs font-medium text-slate-300">Token Type</span>
									<select
										value={tokenType}
										onChange={(e) =>
											setTokenType(e.target.value as "client" | "admin" | "connector")
										}
										className={fieldClassName}
									>
										<option value="client">Client (Sidecar / Player)</option>
										<option value="connector">Connector (Server Publisher)</option>
										<option value="admin">Admin (Control Plane API)</option>
									</select>
								</label>

								<label className="block space-y-1.5">
									<span className="text-xs font-medium text-slate-300">
										Expires In (days, optional)
									</span>
									<input
										type="number"
										min="1"
										value={tokenExpiryDays ?? ""}
										onChange={(e) =>
											setTokenExpiryDays(e.target.value ? Number(e.target.value) : undefined)
										}
										placeholder="Leave empty for no expiry"
										className={fieldClassName}
									/>
								</label>

								<div className="mt-6 flex justify-end gap-3">
									<button
										type="button"
										onClick={() => setCreateTokenOpen(false)}
										className="rounded-xl border border-white/10 px-4 py-2 text-sm text-slate-300 hover:bg-white/5 transition"
									>
										Cancel
									</button>
									<PrimaryButton type="submit" disabled={tokenCreating || !tokenName.trim()}>
										{tokenCreating ? "Generating…" : "Generate Token"}
									</PrimaryButton>
								</div>
							</form>
						)}
					</div>
				</div>
			) : null}

			{/* Edit User Modal */}
			{editingUser ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 p-4 backdrop-blur-sm">
					<div className="w-full max-w-lg rounded-[2rem] border border-white/10 bg-slate-900 p-6 shadow-2xl">
						<h3 className="text-lg font-semibold text-white">Edit User: @{editingUser.username}</h3>
						<p className="mt-1 text-sm text-slate-400">
							Update authorization role and tunnel service ACL rules.
						</p>

						<form onSubmit={handleSaveUser} className="mt-5 space-y-4">
							<label className="block space-y-1.5">
								<span className="text-xs font-medium text-slate-300">Role</span>
								<select
									value={editRole}
									onChange={(e) => setEditRole(e.target.value as "admin" | "member" | "disabled")}
									className={fieldClassName}
								>
									<option value="admin">Admin (Full privileges)</option>
									<option value="member">Member (Service rules apply)</option>
									<option value="disabled">Disabled (All access blocked)</option>
								</select>
							</label>

							<label className="block space-y-1.5">
								<span className="text-xs font-medium text-slate-300">
									Allowed Service Rules (one per line or comma-separated)
								</span>
								<textarea
									rows={4}
									value={editServiceRules}
									onChange={(e) => setEditServiceRules(e.target.value)}
									placeholder={"mc-*\nsecret-db\n*"}
									className={`${fieldClassName} font-mono text-xs`}
								/>
								<span className="text-[11px] text-slate-500">
									Use wildcard patterns like <code className="text-cyan-300">mc-*</code> or{" "}
									<code className="text-cyan-300">*</code> for all services.
								</span>
							</label>

							<div className="mt-6 flex justify-end gap-3">
								<button
									type="button"
									onClick={() => setEditingUser(null)}
									className="rounded-xl border border-white/10 px-4 py-2 text-sm text-slate-300 hover:bg-white/5 transition"
								>
									Cancel
								</button>
								<PrimaryButton type="submit" disabled={userSaving}>
									{userSaving ? "Saving…" : "Save Changes"}
								</PrimaryButton>
							</div>
						</form>
					</div>
				</div>
			) : null}
		</div>
	);
}
