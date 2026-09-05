import { describe, expect, it } from "vitest";

import { formatBytes, formatPercentage } from "@/lib/format";

describe("formatBytes", () => {
	it("formats zero and negative values", () => {
		expect(formatBytes(0)).toBe("0 B");
		expect(formatBytes(-10)).toBe("0 B");
		expect(formatBytes(undefined)).toBe("0 B");
		expect(formatBytes(null)).toBe("0 B");
	});

	it("formats bytes, kilobytes, megabytes, and gigabytes", () => {
		expect(formatBytes(512)).toBe("512 B");
		expect(formatBytes(1024)).toBe("1.0 KB");
		expect(formatBytes(20480)).toBe("20 KB");
		expect(formatBytes(1572864)).toBe("1.5 MB");
		expect(formatBytes(1073741824)).toBe("1.0 GB");
	});
});

describe("formatPercentage", () => {
	it("formats ratios to percentage strings", () => {
		expect(formatPercentage(0)).toBe("0%");
		expect(formatPercentage(undefined)).toBe("0%");
		expect(formatPercentage(0.825)).toBe("82.5%");
		expect(formatPercentage(1.0)).toBe("100.0%");
	});
});
