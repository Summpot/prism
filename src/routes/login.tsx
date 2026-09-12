import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { ArrowRight, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";

import { Github } from "@/components/icons/Github";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { ErrorBanner } from "@/components/ui";
import { isDesktopApp, openExternalUrl } from "@/lib/desktopWindow";
import {
	getAuthProviders,
	getGitHubLoginUrl,
	getHealth,
	getManagementStatus,
} from "@/lib/managementApi";
import { normalizeBaseUrl } from "@/lib/panelConnection";
import { usePanelSession } from "@/lib/panelSession";
import { m } from "@/paraglide/messages";

export const Route = createFileRoute("/login")({ component: LoginPage });

function LoginPage() {
	const navigate = useNavigate();
	const { connection, saveConnection } = usePanelSession();
	const [baseUrl, setBaseUrl] = useState(connection?.baseUrl ?? "http://127.0.0.1:8080");
	const [token, setToken] = useState(connection?.token ?? "");
	const [submitting, setSubmitting] = useState(false);
	const [oauthLoading, setOauthLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [githubEnabled, setGithubEnabled] = useState(false);

	// Handle OAuth redirect hash fragments: /login#token=prism_adm_...
	useEffect(() => {
		const hash = window.location.hash;
		if (hash && hash.includes("token=")) {
			const cleanHash = hash.startsWith("#") ? hash.slice(1) : hash;
			const params = new URLSearchParams(cleanHash);
			const urlToken = params.get("token");
			if (urlToken) {
				const nextConnection = {
					baseUrl: normalizeBaseUrl(baseUrl),
					token: urlToken,
				};
				saveConnection(nextConnection);
				window.history.replaceState(null, "", window.location.pathname);
				void navigate({ to: "/admin" });
			}
		}
	}, [baseUrl, navigate, saveConnection]);

	// Check if management endpoint has GitHub OAuth enabled
	useEffect(() => {
		let active = true;
		if (!baseUrl.trim()) {
			return;
		}
		try {
			const norm = normalizeBaseUrl(baseUrl);
			getAuthProviders(norm)
				.then((res) => {
					if (active) {
						setGithubEnabled(Boolean(res.github_enabled));
					}
				})
				.catch(() => {
					if (active) {
						setGithubEnabled(false);
					}
				});
		} catch {
			// ignore invalid URL format during typing
		}
		return () => {
			active = false;
		};
	}, [baseUrl]);

	const connect = async (event: React.FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		setSubmitting(true);
		setError(null);

		try {
			const nextConnection = {
				baseUrl: normalizeBaseUrl(baseUrl),
				token: token.trim(),
			};
			await getHealth(nextConnection);
			await getManagementStatus(nextConnection);
			saveConnection(nextConnection);
			navigate({ to: "/admin" });
		} catch (nextError) {
			setError(nextError instanceof Error ? nextError.message : String(nextError));
		} finally {
			setSubmitting(false);
		}
	};

	const loginWithGitHub = async () => {
		try {
			setOauthLoading(true);
			setError(null);
			const norm = normalizeBaseUrl(baseUrl);
			if (typeof window !== "undefined") {
				window.localStorage.setItem("prism_pending_auth_url", norm);
				window.sessionStorage.setItem("prism_pending_auth_url", norm);
			}
			const res = await getGitHubLoginUrl({ baseUrl: norm, token: "" });
			if (res.url) {
				await openExternalUrl(res.url);
			}
		} catch (nextError) {
			setError(nextError instanceof Error ? nextError.message : String(nextError));
		} finally {
			setOauthLoading(false);
		}
	};

	return (
		<section className="mx-auto flex min-h-[70vh] w-full max-w-lg items-center justify-center p-4">
			<Card className="w-full shadow-xs">
				<CardHeader>
					<div className="flex items-center gap-3">
						<div className="rounded-xl bg-primary/10 p-3 text-primary ring-1 ring-primary/20">
							<ShieldCheck className="size-6" />
						</div>
						<div>
							<div className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
								{m.login_eyebrow()}
							</div>
							<CardTitle className="mt-1 text-2xl">{m.login_title()}</CardTitle>
						</div>
					</div>
					<CardDescription className="mt-2">{m.login_description()}</CardDescription>
				</CardHeader>
				<CardContent className="space-y-6">
					<div className="space-y-2">
						<Label htmlFor="login-api-url">{m.login_api_url()}</Label>
						<Input
							id="login-api-url"
							value={baseUrl}
							onChange={(event) => setBaseUrl(event.target.value)}
							placeholder="http://127.0.0.1:8080"
						/>
					</div>

					<div className="space-y-3 rounded-xl border border-border bg-muted/20 p-4">
						<div className="flex items-center justify-between gap-3">
							<div className="flex items-center gap-2">
								<Github className="size-4" />
								<span className="text-sm font-semibold">{m.login_github()}</span>
							</div>
							{githubEnabled ? (
								<Badge
									variant="secondary"
									className="bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
								>
									{m.login_oauth_enabled()}
								</Badge>
							) : (
								<span className="text-xs text-muted-foreground">{m.login_oauth_detected()}</span>
							)}
						</div>
						<Button
							type="button"
							variant="outline"
							className="w-full"
							onClick={loginWithGitHub}
							disabled={oauthLoading}
						>
							<Github className="size-4" />
							{oauthLoading ? m.login_requesting() : m.login_sign_in()}
						</Button>
						{isDesktopApp() ? (
							<p className="text-center text-xs text-muted-foreground">{m.login_desktop_hint()}</p>
						) : null}
					</div>

					<div className="relative">
						<Separator />
						<span className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-card px-2 text-xs uppercase text-muted-foreground">
							{m.login_or_token()}
						</span>
					</div>

					<form onSubmit={connect} className="space-y-4">
						<div className="space-y-2">
							<Label htmlFor="login-token">{m.login_token()}</Label>
							<Input
								id="login-token"
								value={token}
								onChange={(event) => setToken(event.target.value)}
								type="password"
								placeholder="prism_adm_... or panel-secret"
							/>
						</div>

						{error ? <ErrorBanner message={error} /> : null}

						<Button
							type="submit"
							className="w-full"
							disabled={submitting || !baseUrl.trim() || !token.trim()}
						>
							{submitting ? m.login_verifying() : m.login_connect()}
							<ArrowRight className="size-4" />
						</Button>
					</form>
				</CardContent>
			</Card>
		</section>
	);
}
