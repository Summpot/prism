import { AlertCircle, Download, Loader2, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { useClientConfig } from "@/hooks/useClientConfig";
import { checkForUpdate, installUpdate } from "@/lib/client/clientIpc";
import { isDesktopApp } from "@/lib/desktopWindow";
import { m } from "@/paraglide/messages";
import type { UpdateCheckResult } from "@/types/client";

export function UpdatePromptModal() {
	const isDesktop = isDesktopApp();
	const { autoCheckUpdate, updateChannel, configLoaded } = useClientConfig();

	const [open, setOpen] = useState(false);
	const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null);
	const [isInstalling, setIsInstalling] = useState(false);
	const [errorMessage, setErrorMessage] = useState<string | null>(null);

	useEffect(() => {
		if (!isDesktop || !configLoaded || !autoCheckUpdate) {
			return;
		}

		let cancelled = false;
		const timer = setTimeout(async () => {
			try {
				const res = await checkForUpdate(updateChannel);
				if (cancelled || !res.available || !res.version) return;

				// Check if user dismissed this specific version during the current session
				const dismissed = sessionStorage.getItem("prism_dismissed_update");
				if (dismissed === res.version) {
					return;
				}

				setUpdateInfo(res);
				setOpen(true);
			} catch (err) {
				console.debug("Background update check failed:", err);
			}
		}, 4000);

		return () => {
			cancelled = true;
			clearTimeout(timer);
		};
	}, [isDesktop, configLoaded, autoCheckUpdate, updateChannel]);

	const handleDismiss = () => {
		if (updateInfo?.version) {
			sessionStorage.setItem("prism_dismissed_update", updateInfo.version);
		}
		setOpen(false);
		setErrorMessage(null);
	};

	const handleConfirmUpdate = async () => {
		if (!updateInfo) return;
		setIsInstalling(true);
		setErrorMessage(null);

		try {
			await installUpdate(updateInfo.channel || updateChannel);
		} catch (err) {
			setIsInstalling(false);
			setErrorMessage(err instanceof Error ? err.message : String(err));
		}
	};

	if (!isDesktop || !updateInfo) {
		return null;
	}

	return (
		<Dialog
			open={open}
			onOpenChange={(nextOpen) => {
				if (!isInstalling) {
					if (!nextOpen) handleDismiss();
					else setOpen(true);
				}
			}}
		>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle className="flex items-center gap-2 text-base">
						<Sparkles className="h-4 w-4 text-primary shrink-0" />
						<span>{m.client_update_prompt_title()}</span>
					</DialogTitle>
					<DialogDescription className="text-xs text-muted-foreground pt-1">
						{m.client_update_prompt_desc({ version: updateInfo.version || "latest" })}
					</DialogDescription>
				</DialogHeader>

				<div className="space-y-3 py-1 text-xs">
					{updateInfo.body ? (
						<div className="space-y-1">
							<span className="text-[11px] font-medium text-muted-foreground">
								{m.client_update_notes()}
							</span>
							<div className="max-h-36 overflow-y-auto rounded-md border border-border/60 bg-muted/40 p-2.5 text-[11px] text-muted-foreground whitespace-pre-wrap font-mono select-text">
								{updateInfo.body}
							</div>
						</div>
					) : null}

					{isInstalling ? (
						<div className="flex items-center gap-2 rounded-md bg-primary/10 border border-primary/20 p-2.5 text-primary text-xs">
							<Loader2 className="h-4 w-4 animate-spin shrink-0" />
							<span>{m.client_update_prompt_updating()}</span>
						</div>
					) : null}

					{errorMessage ? (
						<div className="flex items-center gap-2 rounded-md bg-destructive/10 border border-destructive/20 p-2.5 text-destructive text-xs">
							<AlertCircle className="h-4 w-4 shrink-0" />
							<span className="truncate">{m.client_update_failed({ error: errorMessage })}</span>
						</div>
					) : null}
				</div>

				<DialogFooter className="gap-2 sm:gap-0 pt-1">
					<Button
						variant="outline"
						size="sm"
						onClick={handleDismiss}
						disabled={isInstalling}
						className="text-xs"
					>
						{m.client_update_prompt_later()}
					</Button>
					<Button
						size="sm"
						onClick={handleConfirmUpdate}
						disabled={isInstalling}
						className="text-xs gap-1.5"
					>
						{isInstalling ? (
							<Loader2 className="h-3.5 w-3.5 animate-spin" />
						) : (
							<Download className="h-3.5 w-3.5" />
						)}
						<span>{m.client_update_prompt_confirm()}</span>
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
