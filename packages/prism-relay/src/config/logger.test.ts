import { describe, it, expect, vi, beforeEach } from "vitest";
import { createLogger } from "./logger.js";

describe("logger", () => {
  let stdoutSpy: ReturnType<typeof vi.spyOn>;
  let stderrSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    stdoutSpy = vi.spyOn(process.stdout, "write").mockImplementation(() => true);
    stderrSpy = vi.spyOn(process.stderr, "write").mockImplementation(() => true);
  });

  describe("text format", () => {
    it("includes timestamp, level, and message", () => {
      const logger = createLogger({ level: "debug", format: "text" });
      logger.info("hello world");

      expect(stdoutSpy).toHaveBeenCalledOnce();
      const output = (stdoutSpy.mock.calls[0]?.[0] ?? "") as string;
      // ISO timestamp pattern
      expect(output).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/);
      expect(output).toContain("[INFO ]");
      expect(output).toContain("hello world");
    });

    it("includes key=value pairs when data provided", () => {
      const logger = createLogger({ level: "debug", format: "text" });
      logger.info("request", { method: "GET", path: "/api" });

      const output = (stdoutSpy.mock.calls[0]?.[0] ?? "") as string;
      expect(output).toContain('method="GET"');
      expect(output).toContain('path="/api"');
    });

    it("omits data section when data is empty object", () => {
      const logger = createLogger({ level: "debug", format: "text" });
      logger.info("simple message", {});

      const output = (stdoutSpy.mock.calls[0]?.[0] ?? "") as string;
      // Should end with the message, no trailing key=value
      expect(output).toMatch(/simple message\n$/);
    });
  });

  describe("json format", () => {
    it("produces valid JSON with ts, level, msg, and data fields", () => {
      const logger = createLogger({ level: "debug", format: "json" });
      logger.info("structured", { count: 42 });

      const output = (stdoutSpy.mock.calls[0]?.[0] ?? "") as string;
      const parsed = JSON.parse(output.trim()) as Record<string, unknown>;
      expect(parsed.ts).toMatch(/^\d{4}-\d{2}-\d{2}T/);
      expect(parsed.level).toBe("info");
      expect(parsed.msg).toBe("structured");
      expect(parsed.count).toBe(42);
    });
  });

  describe("level filtering", () => {
    it("info logger skips debug messages", () => {
      const logger = createLogger({ level: "info", format: "text" });
      logger.debug("should not appear");

      expect(stdoutSpy).not.toHaveBeenCalled();
      expect(stderrSpy).not.toHaveBeenCalled();
    });

    it("warn logger skips info and debug messages", () => {
      const logger = createLogger({ level: "warn", format: "text" });
      logger.debug("skip");
      logger.info("skip too");

      expect(stdoutSpy).not.toHaveBeenCalled();
      expect(stderrSpy).not.toHaveBeenCalled();
    });

    it("error logger only logs error messages", () => {
      const logger = createLogger({ level: "error", format: "text" });
      logger.debug("skip");
      logger.info("skip");
      logger.warn("skip");
      logger.error("important");

      expect(stdoutSpy).not.toHaveBeenCalled();
      expect(stderrSpy).toHaveBeenCalledOnce();
      expect((stderrSpy.mock.calls[0]?.[0] ?? "") as string).toContain("important");
    });

    it("debug level logs everything", () => {
      const logger = createLogger({ level: "debug", format: "text" });
      logger.debug("d");
      logger.info("i");
      logger.warn("w");
      logger.error("e");

      // debug + info go to stdout
      expect(stdoutSpy).toHaveBeenCalledTimes(2);
      // warn + error go to stderr
      expect(stderrSpy).toHaveBeenCalledTimes(2);
    });
  });

  describe("output routing", () => {
    it("routes warn and error to stderr, info and debug to stdout", () => {
      const logger = createLogger({ level: "debug", format: "text" });

      logger.debug("dbg");
      expect(stdoutSpy).toHaveBeenCalledTimes(1);

      logger.info("inf");
      expect(stdoutSpy).toHaveBeenCalledTimes(2);

      logger.warn("wrn");
      expect(stderrSpy).toHaveBeenCalledTimes(1);
      expect((stderrSpy.mock.calls[0]?.[0] ?? "") as string).toContain("wrn");

      logger.error("err");
      expect(stderrSpy).toHaveBeenCalledTimes(2);
      expect((stderrSpy.mock.calls[1]?.[0] ?? "") as string).toContain("err");
    });
  });
});
