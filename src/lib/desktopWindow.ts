/**
 * Desktop Window management utilities for Tauri frameless application.
 */

export function isDesktopApp(): boolean {
	if (typeof window === "undefined") return false;
	return Boolean(
		(window as unknown as { __TAURI__?: unknown }).__TAURI__ ||
		(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__,
	);
}

interface TauriInternals {
	invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
}

function getTauriInvoke():
	| ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>)
	| null {
	if (typeof window === "undefined") return null;
	const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriInternals })
		.__TAURI_INTERNALS__;
	if (internals?.invoke) {
		return internals.invoke.bind(internals);
	}
	const globalTauri = (window as unknown as { __TAURI__?: { core?: TauriInternals } }).__TAURI__;
	if (globalTauri?.core?.invoke) {
		return globalTauri.core.invoke.bind(globalTauri.core);
	}
	return null;
}

/**
 * Minimize desktop application window to taskbar.
 */
export async function minimizeWindow(): Promise<void> {
	const invoke = getTauriInvoke();
	if (!invoke) return;
	try {
		await invoke("plugin:window|minimize");
	} catch (err) {
		console.warn("Failed to minimize window:", err);
	}
}

/**
 * Toggle maximize / restore desktop application window.
 */
export async function toggleMaximizeWindow(): Promise<void> {
	const invoke = getTauriInvoke();
	if (!invoke) return;
	try {
		await invoke("plugin:window|toggle_maximize");
	} catch (err) {
		console.warn("Failed to toggle maximize window:", err);
	}
}

/**
 * Check if desktop application window is currently maximized.
 */
export async function isWindowMaximized(): Promise<boolean> {
	const invoke = getTauriInvoke();
	if (!invoke) return false;
	try {
		return Boolean(await invoke("plugin:window|is_maximized"));
	} catch (err) {
		console.warn("Failed to check if window is maximized:", err);
		return false;
	}
}

/**
 * Close desktop application window (hides to system tray).
 */
export async function closeWindow(): Promise<void> {
	const invoke = getTauriInvoke();
	if (!invoke) return;
	try {
		await invoke("plugin:window|close");
	} catch (err) {
		console.warn("Failed to close window:", err);
	}
}

/**
 * Open external URL in default system browser directly.
 */
export async function openExternalUrl(url: string): Promise<void> {
	const invoke = getTauriInvoke();
	if (invoke) {
		try {
			await invoke("open_external_url", { url });
			return;
		} catch (err) {
			console.debug("Failed to invoke open_external_url, falling back to window.open:", err);
		}
	}
	if (typeof window !== "undefined") {
		window.open(url, "_blank");
	}
}

/**
 * Generic invoke wrapper for Tauri commands.
 * Throws an error if invoked outside of a Tauri desktop context.
 */
export async function invokeTauri<T = unknown>(
	cmd: string,
	args?: Record<string, unknown>,
): Promise<T> {
	const invoke = getTauriInvoke();
	if (!invoke) {
		throw new Error(`Tauri invoke is not available: cannot execute command '${cmd}'`);
	}
	return (await invoke(cmd, args)) as T;
}
