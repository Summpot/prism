import { AlertTriangle, RefreshCw, Search } from "lucide-react";
import type { ReactNode } from "react";

export function PageHeader({
	eyebrow,
	title,
	description,
	actions,
}: {
	eyebrow: string;
	title: string;
	description?: string;
	actions?: ReactNode;
}) {
	return (
		<section className="rounded-[2rem] border border-white/8 bg-slate-950/70 p-6 md:p-8">
			<div className="flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
				<div className="max-w-3xl">
					<div className="text-[11px] uppercase tracking-[0.35em] text-cyan-300/70">{eyebrow}</div>
					<h1 className="mt-3 text-4xl font-semibold text-white">{title}</h1>
					{description ? (
						<p className="mt-3 text-base leading-7 text-slate-400">{description}</p>
					) : null}
				</div>
				{actions ? <div className="flex flex-wrap items-center gap-3">{actions}</div> : null}
			</div>
		</section>
	);
}

export function StateCard({ label }: { label: string }) {
	return (
		<div className="rounded-3xl border border-white/8 bg-slate-950/70 px-6 py-10 text-sm text-slate-400">
			{label}
		</div>
	);
}

export function EmptyState({ icon, label }: { icon?: ReactNode; label: string }) {
	return (
		<div className="rounded-3xl border border-dashed border-white/10 bg-white/3 px-6 py-10 text-center text-sm text-slate-400">
			{icon ? <div className="mx-auto mb-3 flex justify-center text-slate-500">{icon}</div> : null}
			{label}
		</div>
	);
}

export function ErrorBanner({ message, onRetry }: { message: string; onRetry?: () => void }) {
	return (
		<div className="flex items-center justify-between gap-4 rounded-3xl border border-red-400/20 bg-red-400/8 px-5 py-4 text-sm text-red-100">
			<div className="flex items-start gap-2">
				<AlertTriangle className="mt-0.5 h-4 w-4 flex-none" />
				<span className="break-all">{message}</span>
			</div>
			{onRetry ? (
				<button
					type="button"
					onClick={onRetry}
					className="rounded-xl border border-red-400/30 px-3 py-1.5 text-xs font-medium transition hover:bg-red-400/15"
				>
					Retry
				</button>
			) : null}
		</div>
	);
}

export function MetricCard({
	label,
	value,
	icon,
	compact = false,
}: {
	label: string;
	value: string | number;
	icon?: ReactNode;
	compact?: boolean;
}) {
	return (
		<div className="rounded-3xl border border-white/8 bg-slate-950/70 p-5">
			<div className="flex items-center justify-between text-slate-400">
				<span className="text-xs uppercase tracking-[0.2em]">{label}</span>
				{icon ? <div className="text-cyan-300">{icon}</div> : null}
			</div>
			<div
				className={`mt-4 ${compact ? "break-all text-sm text-white" : "text-3xl font-semibold text-white"}`}
			>
				{value}
			</div>
		</div>
	);
}

export function InfoValue({ label, value }: { label: string; value: string | number }) {
	return (
		<div className="rounded-2xl border border-white/8 bg-white/4 px-4 py-3">
			<div className="text-xs uppercase tracking-[0.2em] text-slate-500">{label}</div>
			<div className="mt-2 break-all text-sm font-medium text-white">{value}</div>
		</div>
	);
}

export function Badge({
	tone = "neutral",
	children,
}: {
	tone?: "neutral" | "ok" | "warn" | "danger" | "info";
	children: ReactNode;
}) {
	const tones = {
		neutral: "bg-slate-400/12 text-slate-300",
		ok: "bg-emerald-400/12 text-emerald-100",
		warn: "bg-amber-400/12 text-amber-100",
		danger: "bg-red-400/12 text-red-100",
		info: "bg-cyan-400/12 text-cyan-100",
	};

	return (
		<span
			className={`rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-[0.2em] ${tones[tone]}`}
		>
			{children}
		</span>
	);
}

export function SecondaryButton({
	children,
	onClick,
	disabled,
	type = "button",
}: {
	children: ReactNode;
	onClick?: () => void;
	disabled?: boolean;
	type?: "button" | "submit";
}) {
	return (
		<button
			type={type}
			onClick={onClick}
			disabled={disabled}
			className="inline-flex items-center gap-2 rounded-2xl border border-white/10 bg-white/5 px-4 py-3 text-sm font-medium text-white transition hover:border-cyan-400/30 hover:bg-cyan-400/10 disabled:cursor-not-allowed disabled:opacity-50"
		>
			{children}
		</button>
	);
}

export function DangerButton({
	children,
	onClick,
	disabled,
}: {
	children: ReactNode;
	onClick?: () => void;
	disabled?: boolean;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			disabled={disabled}
			className="inline-flex items-center gap-2 rounded-2xl border border-red-400/20 bg-red-400/8 px-4 py-3 text-sm font-medium text-red-200 transition hover:border-red-400/40 hover:bg-red-400/16 disabled:cursor-not-allowed disabled:opacity-50"
		>
			{children}
		</button>
	);
}

export function PrimaryButton({
	children,
	onClick,
	disabled,
	type = "button",
}: {
	children: ReactNode;
	onClick?: () => void;
	disabled?: boolean;
	type?: "button" | "submit";
}) {
	return (
		<button
			type={type}
			onClick={onClick}
			disabled={disabled}
			className="inline-flex items-center gap-3 rounded-2xl bg-cyan-400 px-5 py-3 text-sm font-semibold text-slate-950 transition hover:bg-cyan-300 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
		>
			{children}
		</button>
	);
}

export function RefreshButton({
	onClick,
	loading,
	label = "Refresh",
}: {
	onClick: () => void;
	loading?: boolean;
	label?: string;
}) {
	return (
		<SecondaryButton onClick={onClick} disabled={loading}>
			<RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
			{label}
		</SecondaryButton>
	);
}

export function SearchInput({
	value,
	onChange,
	placeholder = "Filter…",
}: {
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
}) {
	return (
		<label className="relative block min-w-[12rem] flex-1">
			<Search className="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-slate-500" />
			<input
				value={value}
				onChange={(event) => onChange(event.target.value)}
				placeholder={placeholder}
				className="w-full rounded-2xl border border-white/10 bg-slate-900 py-3 pr-4 pl-10 text-sm text-white outline-none transition focus:border-cyan-400/40"
			/>
		</label>
	);
}

export function ToggleChip({
	active,
	onClick,
	children,
}: {
	active: boolean;
	onClick: () => void;
	children: ReactNode;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={`inline-flex items-center gap-2 rounded-2xl border px-4 py-3 text-sm font-medium transition ${
				active
					? "border-cyan-400/40 bg-cyan-400/12 text-white"
					: "border-white/10 bg-white/5 text-slate-300 hover:border-cyan-400/30 hover:bg-cyan-400/10 hover:text-white"
			}`}
		>
			{children}
		</button>
	);
}

export function Field({
	title,
	hint,
	children,
	error,
}: {
	title: string;
	hint?: string;
	children: ReactNode;
	error?: string[];
}) {
	return (
		<label className="block space-y-2">
			<div>
				<div className="text-sm font-medium text-white">{title}</div>
				{hint ? <div className="mt-1 text-xs leading-5 text-slate-500">{hint}</div> : null}
			</div>
			{children}
			{error?.length ? (
				<div className="flex flex-col gap-1 text-sm text-amber-200">
					{error.map((message) => (
						<div key={message} className="flex items-center gap-2">
							<AlertTriangle className="h-4 w-4 flex-none" />
							<span>{message}</span>
						</div>
					))}
				</div>
			) : null}
		</label>
	);
}

export const fieldClassName =
	"w-full rounded-2xl border border-white/10 bg-slate-900 px-4 py-3 text-white outline-none transition focus:border-cyan-400/40";

export function SectionCard({
	title,
	description,
	icon,
	actions,
	children,
}: {
	title: string;
	description?: string;
	icon?: ReactNode;
	actions?: ReactNode;
	children: ReactNode;
}) {
	return (
		<section className="rounded-3xl border border-white/8 bg-slate-950/75 p-6 shadow-[0_24px_80px_rgba(15,23,42,0.45)]">
			<div className="flex items-start justify-between gap-4">
				<div>
					<div className="flex items-center gap-3 text-white">
						{icon ? (
							<div className="rounded-2xl border border-cyan-400/25 bg-cyan-400/10 p-2 text-cyan-300">
								{icon}
							</div>
						) : null}
						<h2 className="text-lg font-semibold">{title}</h2>
					</div>
					{description ? (
						<p className="mt-2 max-w-3xl text-sm leading-6 text-slate-400">{description}</p>
					) : null}
				</div>
				{actions}
			</div>
			<div className="mt-6">{children}</div>
		</section>
	);
}
