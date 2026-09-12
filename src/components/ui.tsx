import { AlertTriangle, RefreshCw, Search } from "lucide-react";
import type { ReactNode } from "react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge as ShadcnBadge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
	Empty,
	EmptyDescription,
	EmptyHeader,
	EmptyMedia,
	EmptyTitle,
} from "@/components/ui/empty";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

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

export function EmptyState({
	icon,
	label,
	title,
	description,
}: {
	icon?: ReactNode;
	label?: string;
	title?: string;
	description?: string;
}) {
	return (
		<Empty className="border border-dashed border-border bg-muted/20">
			<EmptyHeader>
				{icon ? <EmptyMedia variant="icon">{icon}</EmptyMedia> : null}
				{title ? <EmptyTitle>{title}</EmptyTitle> : null}
				{label || description ? (
					<EmptyDescription>{label ?? description}</EmptyDescription>
				) : null}
				{label && description ? (
					<EmptyDescription className="text-xs">{description}</EmptyDescription>
				) : null}
			</EmptyHeader>
		</Empty>
	);
}

export function ErrorBanner({ message, onRetry }: { message: string; onRetry?: () => void }) {
	return (
		<Alert variant="destructive" className="border-destructive/30 bg-destructive/10">
			<AlertTriangle />
			<AlertDescription className="flex items-center justify-between gap-4">
				<span className="break-all font-medium">{message}</span>
				{onRetry ? (
					<Button
						variant="outline"
						size="xs"
						onClick={onRetry}
						className="border-destructive/30 hover:bg-destructive/20"
					>
						{m.common_retry()}
					</Button>
				) : null}
			</AlertDescription>
		</Alert>
	);
}

export function ResultBanner({ ok, children }: { ok: boolean; children: ReactNode }) {
	return (
		<Alert
			variant={ok ? "default" : "destructive"}
			className={
				ok
					? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
					: "border-destructive/30 bg-destructive/10"
			}
		>
			<AlertDescription>{children}</AlertDescription>
		</Alert>
	);
}

export function WarningBanner({
	title,
	children,
}: {
	title?: string;
	children: ReactNode;
}) {
	return (
		<Alert className="border-amber-500/20 bg-amber-500/10 text-amber-800 dark:text-amber-200">
			<AlertTriangle />
			{title ? <AlertTitle>{title}</AlertTitle> : null}
			<AlertDescription>{children}</AlertDescription>
		</Alert>
	);
}

export function NestedPanel({
	children,
	className,
}: {
	children: ReactNode;
	className?: string;
}) {
	return (
		<div className={cn("rounded-xl border border-border bg-muted/20 p-4", className)}>
			{children}
		</div>
	);
}

export function SwitchRow({
	checked,
	onCheckedChange,
	children,
}: {
	checked: boolean;
	onCheckedChange: (checked: boolean) => void;
	children: ReactNode;
}) {
	return (
		<label className="flex items-center justify-between gap-3 rounded-lg border border-border bg-muted/20 px-3 py-2.5 text-sm text-foreground">
			<span>{children}</span>
			<Switch checked={checked} onCheckedChange={onCheckedChange} />
		</label>
	);
}

export function CountChip({ icon, children }: { icon?: ReactNode; children: ReactNode }) {
	return (
		<div className="inline-flex items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
			{icon}
			{children}
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
	variant: variantProp,
	className,
	children,
}: {
	tone?: "neutral" | "ok" | "warn" | "danger" | "info" | "cyan";
	variant?: string;
	className?: string;
	children: ReactNode;
}) {
	const resolvedTone =
		tone !== "neutral"
			? tone
			: variantProp === "success" || variantProp === "ok"
				? "ok"
				: variantProp === "destructive" || variantProp === "danger"
					? "danger"
					: variantProp === "warning" || variantProp === "warn"
						? "warn"
						: variantProp === "info"
							? "info"
							: "neutral";

	const variant =
		resolvedTone === "ok"
			? "secondary"
			: resolvedTone === "warn"
				? "outline"
				: resolvedTone === "danger"
					? "destructive"
					: "secondary";

	const customClass =
		resolvedTone === "ok"
			? "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
			: resolvedTone === "warn"
				? "bg-amber-500/15 text-amber-600 dark:text-amber-400 border-amber-500/20"
				: resolvedTone === "danger"
					? "bg-destructive/15 text-destructive border-destructive/20"
					: resolvedTone === "cyan" || resolvedTone === "info"
						? "bg-primary/15 text-primary border-primary/20"
						: "bg-muted text-muted-foreground border-border";

	return (
		<ShadcnBadge
			variant={variant}
			className={cn("text-xs font-semibold uppercase", customClass, className)}
		>
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
	label = m.common_refresh(),
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
	placeholder = m.common_filter(),
}: {
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
}) {
	return (
		<div className="relative min-w-48 flex-1">
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
