// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { RouterProvider } from "@tanstack/react-router";
import { getRouter } from "./router";

describe("Full Router Render", () => {
	it("renders route / without throwing context error", async () => {
		const router = getRouter();
		await router.load();
		expect(() => {
			render(<RouterProvider router={router} />);
		}).not.toThrow();
	});
});
