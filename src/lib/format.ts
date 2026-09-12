import { m } from "@/paraglide/messages";
import { getLocale } from "@/paraglide/runtime";

export function formatTime(unixMs: number, style: "short" | "medium" = "medium") {
	if (!unixMs) {
		return m.format_never();
	}

	return new Intl.DateTimeFormat(getLocale() === "zh-CN" ? "zh-CN" : "en-US", {
		dateStyle: style === "short" ? "medium" : "medium",
		timeStyle: style === "short" ? "short" : "medium",
	}).format(new Date(unixMs));
}

export function formatUptime(seconds: number): string {
	const hrs = Math.floor(seconds / 3600);
	const mins = Math.floor((seconds % 3600) / 60);
	const secs = seconds % 60;
	return `${hrs.toString().padStart(2, "0")}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}

export function formatDuration(startUnixMs: number) {
	if (!startUnixMs) {
		return "—";
	}

	const seconds = Math.max(0, Math.floor((Date.now() - startUnixMs) / 1000));
	if (seconds < 60) {
		return `${seconds}s`;
	}

	const minutes = Math.floor(seconds / 60);
	if (minutes < 60) {
		return `${minutes}m ${seconds % 60}s`;
	}

	const hours = Math.floor(minutes / 60);
	if (hours < 48) {
		return `${hours}h ${minutes % 60}m`;
	}

	const days = Math.floor(hours / 24);
	return `${days}d ${hours % 24}h`;
}

export function formatRelative(unixMs: number) {
	if (!unixMs) {
		return m.format_never();
	}

	const delta = Date.now() - unixMs;
	if (delta < 0) {
		return formatTime(unixMs, "short");
	}
	if (delta < 5_000) {
		return m.format_just_now();
	}
	if (delta < 60_000) {
		return m.format_seconds_ago({ count: Math.floor(delta / 1000) });
	}
	if (delta < 3_600_000) {
		return m.format_minutes_ago({ count: Math.floor(delta / 60_000) });
	}
	if (delta < 86_400_000) {
		return m.format_hours_ago({ count: Math.floor(delta / 3_600_000) });
	}
	return formatTime(unixMs, "short");
}

export function formatBytes(bytes: number | undefined | null): string {
	if (!bytes || bytes <= 0) {
		return "0 B";
	}
	const units = ["B", "KB", "MB", "GB", "TB"];
	const i = Math.floor(Math.log(bytes) / Math.log(1024));
	const idx = Math.min(i, units.length - 1);
	const val = bytes / 1024 ** idx;
	return `${val >= 10 || idx === 0 ? val.toFixed(0) : val.toFixed(1)} ${units[idx]}`;
}

export function formatPercentage(ratio: number | undefined | null): string {
	if (!ratio || ratio <= 0) {
		return "0%";
	}
	return `${(ratio * 100).toFixed(1)}%`;
}

/** Positive values are latency gains, shown as a reduction (-X.Xms). */
export function formatGainMs(value: number): string {
	if (value > 0) return `-${value.toFixed(1)}ms`;
	if (value < 0) return `+${Math.abs(value).toFixed(1)}ms`;
	return "0.0ms";
}

/** Positive values are latency costs, shown as an increase (+X.Xms). */
export function formatCostMs(value: number): string {
	if (value > 0) return `+${value.toFixed(1)}ms`;
	if (value < 0) return `-${Math.abs(value).toFixed(1)}ms`;
	return "0.0ms";
}

export function formatBitsPerSecond(bps: number | undefined | null): string {
	if (!bps || bps <= 0) {
		return "0 bps";
	}
	const units = ["bps", "Kbps", "Mbps", "Gbps"];
	const i = Math.min(Math.floor(Math.log(bps) / Math.log(1000)), units.length - 1);
	const val = bps / 1000 ** i;
	return `${val >= 10 || i === 0 ? val.toFixed(0) : val.toFixed(1)} ${units[i]}`;
}
