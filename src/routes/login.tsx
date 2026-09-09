import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { ArrowRight, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";

import { Github } from "@/components/icons/Github";

import { fieldClassName, PrimaryButton } from "@/components/ui";
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
		<section className="mx-auto flex min-h-[70vh] w-full max-w-3xl items-center justify-center">
			<div className="w-full rounded-[2rem] border border-white/8 bg-slate-950/75 p-8 shadow-[0_24px_80px_rgba(2,6,23,0.45)] md:p-10">
				<div className="flex items-center gap-4">
					<div className="rounded-3xl border border-cyan-400/30 bg-cyan-400/10 p-4 text-cyan-300">
						<ShieldCheck className="h-7 w-7" />
					</div>
					<div>
						<div className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">
							{m.login_eyebrow()}
						</div>
						<h1 className="mt-2 text-3xl font-semibold text-white">{m.login_title()}</h1>
					</div>
				</div>

				<p className="mt-6 text-base leading-7 text-slate-400">{m.login_description()}</p>

				<div className="mt-8 space-y-6">
					<label className="block space-y-2">
						<span className="text-sm font-medium text-white">{m.login_api_url()}</span>
						<input
							value={baseUrl}
							onChange={(event) => setBaseUrl(event.target.value)}
							placeholder="http://127.0.0.1:8080"
							className={fieldClassName}
						/>
					</label>

					{/* GitHub Sign In Section */}
					<div className="rounded-2xl border border-white/10 bg-white/3 p-5 space-y-4">
						<div className="flex items-center justify-between">
							<div className="flex items-center gap-2">
								<Github className="h-5 w-5 text-white" />
								<span className="text-sm font-semibold text-white">{m.login_github()}</span>
							</div>
							{githubEnabled ? (
								<span className="rounded-full bg-emerald-500/15 border border-emerald-500/30 px-2.5 py-0.5 text-xs font-medium text-emerald-300">
									{m.login_oauth_enabled()}
								</span>
							) : (
								<span className="text-xs text-slate-500">{m.login_oauth_detected()}</span>
							)}
						</div>
						<div className="pt-1 space-y-2">
							<button
								type="button"
								onClick={loginWithGitHub}
								disabled={oauthLoading}
								className="w-full flex items-center justify-center gap-2.5 rounded-xl border border-white/20 bg-white/10 px-4 py-3 text-sm font-semibold text-white transition hover:border-white/40 hover:bg-white/15 cursor-pointer disabled:opacity-50"
							>
								<Github className="h-4 w-4" />
								<span>{oauthLoading ? m.login_requesting() : m.login_sign_in()}</span>
							</button>
							{isDesktopApp() ? (
								<p className="text-xs text-slate-400 text-center">{m.login_desktop_hint()}</p>
							) : null}
						</div>
					</div>

					<div className="relative my-6">
						<div className="absolute inset-0 flex items-center">
							<div className="w-full border-t border-white/10" />
						</div>
						<div className="relative flex justify-center text-xs uppercase">
							<span className="bg-slate-950 px-3 text-slate-400">{m.login_or_token()}</span>
						</div>
					</div>

					<form onSubmit={connect} className="space-y-5">
						<label className="block space-y-2">
							<span className="text-sm font-medium text-white">{m.login_token()}</span>
							<input
								value={token}
								onChange={(event) => setToken(event.target.value)}
								type="password"
								placeholder="prism_adm_... or panel-secret"
								className={fieldClassName}
							/>
						</label>

						{error ? (
							<div className="rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm text-red-100">
								{error}
							</div>
						) : null}

						<PrimaryButton type="submit" disabled={submitting || !baseUrl.trim() || !token.trim()}>
							{submitting ? m.login_verifying() : m.login_connect()}
							<ArrowRight className="h-4 w-4" />
						</PrimaryButton>
					</form>
				</div>
			</div>
		</section>
	);
}
