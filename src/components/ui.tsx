import { AlertTriangle, RefreshCw, Search } from "lucide-react";
import type { ReactNode } from "react";

import { Badge as ShadcnBadge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

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
		<div className="relative mb-6 rounded-xl border border-border bg-card p-6 shadow-xs">
			<div className="flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
				<div className="max-w-3xl">
					<div className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
						{eyebrow}
					</div>
					<h1 className="mt-2 text-2xl font-bold tracking-tight text-foreground md:text-3xl">
						{title}
					</h1>
					{description ? (
						<p className="mt-2 text-sm leading-relaxed text-muted-foreground">{description}</p>
					) : null}
				</div>
				{actions ? <div className="flex flex-wrap items-center gap-2.5">{actions}</div> : null}
			</div>
		</div>
	);
}

export function StateCard({ label }: { label: string }) {
	return (
		<div className="rounded-xl border border-border bg-card px-6 py-8 text-center text-sm text-muted-foreground">
			{label}
		</div>
	);
}

export function EmptyState({ icon, label }: { icon?: ReactNode; label: string }) {
	return (
		<div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border bg-muted/20 px-6 py-10 text-center text-sm text-muted-foreground">
			{icon ? <div className="mb-3 text-muted-foreground/60">{icon}</div> : null}
			{label}
		</div>
	);
}

export function ErrorBanner({ message, onRetry }: { message: string; onRetry?: () => void }) {
	return (
		<div className="flex items-center justify-between gap-4 rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive">
			<div className="flex items-start gap-2.5">
				<AlertTriangle className="mt-0.5 h-4 w-4 flex-none" />
				<span className="break-all font-medium">{message}</span>
			</div>
			{onRetry ? (
				<Button
					variant="outline"
					size="xs"
					onClick={onRetry}
					className="border-destructive/30 hover:bg-destructive/20"
				>
					Retry
				</Button>
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
		<Card className="shadow-xs">
			<CardHeader className="flex flex-row items-center justify-between pb-2">
				<span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
					{label}
				</span>
				{icon ? <div className="text-muted-foreground">{icon}</div> : null}
			</CardHeader>
			<CardContent>
				<div
					className={cn(
						"font-semibold text-foreground",
						compact ? "break-all text-sm" : "text-2xl",
					)}
				>
					{value}
				</div>
			</CardContent>
		</Card>
	);
}

export function InfoValue({ label, value }: { label: string; value: string | number }) {
	return (
		<div className="rounded-lg border border-border bg-muted/30 px-3.5 py-2.5">
			<div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
				{label}
			</div>
			<div className="mt-1 break-all text-sm font-semibold text-foreground">{value}</div>
		</div>
	);
}

export function Badge({
	tone = "neutral",
	children,
}: {
	tone?: "neutral" | "ok" | "warn" | "danger" | "info" | "cyan";
	children: ReactNode;
}) {
	const variant =
		tone === "ok"
			? "secondary"
			: tone === "warn"
				? "outline"
				: tone === "danger"
					? "destructive"
					: "secondary";

	const customClass =
		tone === "ok"
			? "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
			: tone === "warn"
				? "bg-amber-500/15 text-amber-600 dark:text-amber-400 border-amber-500/20"
				: tone === "danger"
					? "bg-destructive/15 text-destructive border-destructive/20"
					: tone === "cyan" || tone === "info"
						? "bg-primary/15 text-primary border-primary/20"
						: "bg-muted text-muted-foreground border-border";

	return (
		<ShadcnBadge variant={variant} className={cn("text-xs font-semibold uppercase", customClass)}>
			{children}
		</ShadcnBadge>
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
		<Button type={type} variant="outline" onClick={onClick} disabled={disabled}>
			{children}
		</Button>
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
		<Button type="button" variant="destructive" onClick={onClick} disabled={disabled}>
			{children}
		</Button>
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
		<Button type={type} variant="default" onClick={onClick} disabled={disabled}>
			{children}
		</Button>
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
		<Button variant="outline" onClick={onClick} disabled={loading} className="gap-2">
			<RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
			{label}
		</Button>
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
		<div className="relative min-w-[12rem] flex-1">
			<Search className="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
			<Input
				value={value}
				onChange={(event) => onChange(event.target.value)}
				placeholder={placeholder}
				className="pl-9"
			/>
		</div>
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
		<Button type="button" variant={active ? "default" : "outline"} size="sm" onClick={onClick}>
			{children}
		</Button>
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
		<label className="block space-y-1.5">
			<div>
				<div className="text-sm font-medium text-foreground">{title}</div>
				{hint ? <div className="mt-0.5 text-xs text-muted-foreground">{hint}</div> : null}
			</div>
			{children}
			{error?.length ? (
				<div className="flex flex-col gap-1 text-xs text-destructive">
					{error.map((message) => (
						<div key={message} className="flex items-center gap-1.5">
							<AlertTriangle className="h-3.5 w-3.5 flex-none" />
							<span>{message}</span>
						</div>
					))}
				</div>
			) : null}
		</label>
	);
}

export const fieldClassName =
	"w-full rounded-lg border border-input bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground outline-none transition focus:border-ring focus:ring-1 focus:ring-ring";

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
		<Card className="shadow-xs">
			<CardHeader className="flex flex-row items-start justify-between gap-4">
				<div>
					<div className="flex items-center gap-2.5">
						{icon ? (
							<div className="rounded-lg bg-primary/10 p-1.5 text-primary ring-1 ring-primary/20">
								{icon}
							</div>
						) : null}
						<CardTitle className="text-base font-semibold">{title}</CardTitle>
					</div>
					{description ? <CardDescription className="mt-1.5">{description}</CardDescription> : null}
				</div>
				{actions}
			</CardHeader>
			<CardContent>{children}</CardContent>
		</Card>
	);
}
