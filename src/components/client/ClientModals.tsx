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
							<CardTitle>导入 Prism 节点配置</CardTitle>
							<CardDescription>
								粘贴 <code className="text-primary">prism://</code> 链接或服务端地址
							</CardDescription>
						</CardHeader>
						<CardContent className="space-y-3">
							<Input
								value={importUrl}
								onChange={(e) => setImportUrl(e.target.value)}
								placeholder="quic://play.example.com:7000 或 prism://play.example.com:7000?name=..."
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
								取消
							</Button>
							<Button onClick={handleImportLink}>导入并应用</Button>
						</CardFooter>
					</Card>
				</div>
			) : null}
		</>
	);
}
