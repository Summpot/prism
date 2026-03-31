import { fileURLToPath, URL } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { devtools } from "@tanstack/devtools-vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import viteTsConfigPaths from "vite-tsconfig-paths";

const isVitest = !!process.env.VITEST;

const config = defineConfig(({ command }) => ({
	resolve: {
		alias: {
			"@": fileURLToPath(new URL("./src", import.meta.url)),
		},
	},
	plugins: [
		...(isVitest || command !== "serve" ? [] : [devtools()]),
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
