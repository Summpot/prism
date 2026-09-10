import { useClient } from "@/context/ClientContext";
import { Button } from "@/components/ui/button";
import {
	Card,
	CardContent,
	CardDescription,
	CardFooter,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
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
	} = useClient();

	return (
		<>
			{/* Import Modal */}
			{importModalOpen ? (
				<div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-3 sm:p-4 backdrop-blur-xs">
					<Card className="w-full max-w-sm sm:max-w-lg shadow-xl">
						<CardHeader>
							<CardTitle>{m.client_import_title()}</CardTitle>
							<CardDescription>
								{m.client_import_description()}
							</CardDescription>
						</CardHeader>
						<CardContent className="space-y-3">
							<Input
								value={importUrl}
								onChange={(e) => setImportUrl(e.target.value)}
								placeholder={m.client_import_placeholder()}
							/>
							{importError ? <p className="text-xs text-destructive">{importError}</p> : null}
						</CardContent>
						<CardFooter className="flex justify-end gap-2 border-t border-border pt-4">
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
						</CardFooter>
					</Card>
				</div>
			) : null}
		</>
	);
}
