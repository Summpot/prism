import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { ArrowRight, ShieldCheck } from "lucide-react";
import { useState } from "react";

import { getManagementStatus } from "@/lib/managementApi";
import { usePanelSession } from "@/lib/panelSession";

export const Route = createFileRoute("/login")({ component: LoginPage });

function LoginPage() {
	const navigate = useNavigate();
	const { connection, saveConnection } = usePanelSession();
	const [baseUrl, setBaseUrl] = useState(
		connection?.baseUrl ?? "http://127.0.0.1:8080",
	);
	const [token, setToken] = useState(connection?.token ?? "");
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const connect = async (event: React.FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		setSubmitting(true);
		setError(null);

		try {
			const nextConnection = { baseUrl, token };
			await getManagementStatus(nextConnection);
			saveConnection(nextConnection);
			navigate({ to: "/" });
		} catch (nextError) {
			setError(
				nextError instanceof Error ? nextError.message : String(nextError),
			);
		} finally {
			setSubmitting(false);
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
							Prism panel session
						</div>
						<h1 className="mt-2 text-3xl font-semibold text-white">
							Attach this panel to a management node.
						</h1>
					</div>
				</div>

				<p className="mt-6 text-base leading-7 text-slate-400">
					Because the panel can be deployed separately, it authenticates
					directly against the management API with a base URL and bearer token.
					The connection is stored locally in your browser and can be cleared at
					any time.
				</p>

				<form onSubmit={connect} className="mt-8 space-y-5">
					<label className="block space-y-2">
						<span className="text-sm font-medium text-white">
							Management API base URL
						</span>
						<input
							value={baseUrl}
							onChange={(event) => setBaseUrl(event.target.value)}
							placeholder="http://127.0.0.1:8080"
							className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
						/>
					</label>
					<label className="block space-y-2">
						<span className="text-sm font-medium text-white">
							Panel bearer token
						</span>
						<input
							value={token}
							onChange={(event) => setToken(event.target.value)}
							type="password"
							placeholder="panel-secret"
							className="w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40"
						/>
					</label>

					{error ? (
						<div className="rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm text-red-100">
							{error}
						</div>
					) : null}

					<button
						type="submit"
						disabled={submitting || !baseUrl.trim() || !token.trim()}
						className="inline-flex items-center gap-3 rounded-2xl bg-cyan-400 px-5 py-3 text-sm font-semibold text-slate-950 transition hover:bg-cyan-300 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
					>
						{submitting ? "Verifying endpoint…" : "Connect panel"}
						<ArrowRight className="h-4 w-4" />
					</button>
				</form>
			</div>
		</section>
	);
}
