import { m } from "@/paraglide/messages";
import { getLocale, locales, setLocale } from "@/paraglide/runtime";

export default function LanguageSwitcher() {
	const currentLocale = getLocale();

	return (
		<label className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
			<span>{m.language()}</span>
			<select
				value={currentLocale}
				onChange={(event) => void setLocale(event.target.value as (typeof locales)[number])}
				className="rounded border border-border bg-background px-1.5 py-1 text-[10px] text-foreground outline-none focus:ring-1 focus:ring-primary"
				aria-label={m.language()}
			>
				<option value="en">{m.language_english()}</option>
				<option value="zh-CN">{m.language_chinese()}</option>
			</select>
		</label>
	);
}
