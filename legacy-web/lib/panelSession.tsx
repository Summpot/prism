/**
 * @deprecated This module is maintained for backwards compatibility.
 * Use `@/lib/admin/adminSession` for administrative session management.
 */

export type { AdminSessionContextValue, PanelSessionContextValue } from "@/lib/admin/adminSession";

export {
	AdminSessionProvider,
	PanelSessionProvider,
	useAdminSession,
	usePanelSession,
} from "@/lib/admin/adminSession";
