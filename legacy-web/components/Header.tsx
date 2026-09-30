import { useEffect, useState } from "react";

import { AppSidebarContent } from "@/components/AppSidebar";
import { cn } from "@/lib/utils";

export { AppSidebarContent };

export default function Header() {
	const [collapsed, setCollapsed] = useState(false);

	useEffect(() => {
		const handleToggle = () => setCollapsed((prev) => !prev);
		window.addEventListener("prism:toggle-sidebar", handleToggle);
		return () => {
			window.removeEventListener("prism:toggle-sidebar", handleToggle);
		};
	}, []);

	// Auto-collapse sidebar on narrower viewports on initial load
	useEffect(() => {
		if (typeof window !== "undefined" && window.innerWidth < 960) {
			setCollapsed(true);
		}
	}, []);

	return (
		<aside
			className={cn(
				"flex flex-none flex-col border-r border-border bg-card/60 h-full overflow-hidden transition-[width] duration-200 ease-in-out",
				collapsed ? "w-14" : "w-60 sm:w-64",
			)}
		>
			<AppSidebarContent collapsed={collapsed} />
		</aside>
	);
}

