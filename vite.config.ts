import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { devtools } from "@tanstack/devtools-vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import viteTsConfigPaths from "vite-tsconfig-paths";

const isVitest = !!process.env.VITEST;

const config = defineConfig(({ command }) => ({
	// TanStack Start prerenders the SPA shell through Vite preview during build.
	// Bind preview to IPv4 so Docker/Node do not resolve localhost differently.
	preview: {
		host: "127.0.0.1",
	},
	server: {
		port: 3000,
		strictPort: true,
		proxy: {
			"/client": "http://127.0.0.1:8080",
			"/stats": "http://127.0.0.1:8080",
			"/auth": "http://127.0.0.1:8080",
			"/health": "http://127.0.0.1:8080",
			"/conns": "http://127.0.0.1:8080",
			"/tunnel": "http://127.0.0.1:8080",
			"/managed": "http://127.0.0.1:8080",
			"/middlewares": "http://127.0.0.1:8080",
			"/reload": "http://127.0.0.1:8080",
			"/config": "http://127.0.0.1:8080",
		},
	},
	resolve: {
		alias: {
			"@": fileURLToPath(new URL("./src", import.meta.url)),
		},
	},
	plugins: [
		...(isVitest || command !== "serve"
			? []
			: [
					devtools({
						eventBusConfig: {
							enabled: false,
						},
					}),
				]),
		viteTsConfigPaths({
			projects: ["./tsconfig.json"],
		}),
		tailwindcss(),
		...(isVitest
			? []
			: [
					tanstackStart({
						spa: {
							enabled: true,
						},
					}),
				]),
		viteReact(),
	],
}));

export default config;
