import { useEffect, useRef } from "react";

/**
 * Call `tick` immediately when enabled, then on a fixed interval.
 * `tick` identity changes restart the timer (pass a stable callback).
 */
export function usePolling(tick: () => void, intervalMs: number, enabled = true) {
	const tickRef = useRef(tick);
	tickRef.current = tick;

	useEffect(() => {
		if (!enabled || intervalMs <= 0) {
			return;
		}

		tickRef.current();
		const id = window.setInterval(() => {
			tickRef.current();
		}, intervalMs);

		return () => window.clearInterval(id);
	}, [enabled, intervalMs]);
}
