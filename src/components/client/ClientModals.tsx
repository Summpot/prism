import { useClientLink } from "@/hooks/useClientLink";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { m } from "@/paraglide/messages";

export function ClientModals() {
	const {
		importModalOpen,
		setImportModalOpen,
		importUrl,
		setImportUrl,
		importError,
		setImportError,
		handleImportLink,
	} = useClientLink();

	return (
		<Dialog
			open={importModalOpen}
			onOpenChange={(open) => {
				setImportModalOpen(open);
				if (!open) {
					setImportError(null);
				}
			}}
		>
			<DialogContent className="sm:max-w-lg">
				<DialogHeader>
					<DialogTitle>{m.client_import_title()}</DialogTitle>
					<DialogDescription>{m.client_import_description()}</DialogDescription>
				</DialogHeader>
				<div className="space-y-3">
					<Input
						value={importUrl}
						onChange={(e) => setImportUrl(e.target.value)}
						placeholder={m.client_import_placeholder()}
					/>
					{importError ? <p className="text-xs text-destructive">{importError}</p> : null}
				</div>
				<DialogFooter>
					<Button
						variant="outline"
						onClick={() => {
							setImportModalOpen(false);
							setImportError(null);
						}}
					>
						{m.common_cancel()}
					</Button>
					<Button onClick={handleImportLink}>{m.client_import_apply()}</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
