export function formatTime(unixMs: number, style: "short" | "medium" = "medium") {
	if (!unixMs) {
		return "never";
	}

	return new Intl.DateTimeFormat("en-US", {
		dateStyle: style === "short" ? "medium" : "medium",
		timeStyle: style === "short" ? "short" : "medium",
	}).format(new Date(unixMs));
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
		return "never";
	}

	const delta = Date.now() - unixMs;
	if (delta < 0) {
		return formatTime(unixMs, "short");
	}
	if (delta < 5_000) {
		return "just now";
	}
	if (delta < 60_000) {
		return `${Math.floor(delta / 1000)}s ago`;
	}
	if (delta < 3_600_000) {
		return `${Math.floor(delta / 60_000)}m ago`;
	}
	if (delta < 86_400_000) {
		return `${Math.floor(delta / 3_600_000)}h ago`;
	}
	return formatTime(unixMs, "short");
}
