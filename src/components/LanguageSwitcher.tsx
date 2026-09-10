import { Check, Languages } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";
import { getLocale, type locales, setLocale } from "@/paraglide/runtime";

export default function LanguageSwitcher({ className }: { className?: string }) {
	const currentLocale = getLocale();

	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				render={
					<Button
						variant="ghost"
						size="xs"
						className={cn(
							"h-6 gap-1 px-1.5 text-[11px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground rounded cursor-pointer",
							className,
						)}
						title={m.language()}
						aria-label={m.language()}
					/>
				}
			>
				<Languages className="h-3.5 w-3.5" />
				<span className="font-mono text-[10.5px]">
					{currentLocale === "zh-CN" ? m.language_chinese_short() : m.language_english_short()}
				</span>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" className="min-w-32 text-xs">
				<DropdownMenuItem
					onClick={() => void setLocale("zh-CN" as (typeof locales)[number])}
					className="flex items-center justify-between cursor-pointer"
				>
					<span>{m.language_chinese()}</span>
					{currentLocale === "zh-CN" ? <Check className="h-3.5 w-3.5 text-primary" /> : null}
				</DropdownMenuItem>
				<DropdownMenuItem
					onClick={() => void setLocale("en" as (typeof locales)[number])}
					className="flex items-center justify-between cursor-pointer"
				>
					<span>{m.language_english()}</span>
					{currentLocale === "en" ? <Check className="h-3.5 w-3.5 text-primary" /> : null}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
