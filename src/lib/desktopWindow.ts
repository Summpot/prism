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
