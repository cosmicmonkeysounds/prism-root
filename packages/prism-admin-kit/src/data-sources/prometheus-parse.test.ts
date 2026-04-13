import { describe, expect, it } from "vitest";
import { parsePrometheus, findSample } from "./prometheus-parse.js";

describe("parsePrometheus", () => {
  it("returns empty for empty input", () => {
    expect(parsePrometheus("")).toEqual([]);
    expect(parsePrometheus("\n\n\n")).toEqual([]);
  });

  it("skips comment and HELP/TYPE lines", () => {
    const text = [
      "# HELP relay_uptime_seconds Uptime of the relay process",
      "# TYPE relay_uptime_seconds gauge",
      "relay_uptime_seconds 42",
    ].join("\n");
    const out = parsePrometheus(text);
    expect(out).toEqual([{ name: "relay_uptime_seconds", labels: {}, value: 42 }]);
  });

  it("parses unlabelled samples", () => {
    const text = "relay_modules_total 7\nrelay_peers_online 3\n";
    expect(parsePrometheus(text)).toEqual([
      { name: "relay_modules_total", labels: {}, value: 7 },
      { name: "relay_peers_online", labels: {}, value: 3 },
    ]);
  });

  it("parses labelled samples", () => {
    const text =
      'relay_requests_total{method="GET",status="200"} 142\nrelay_requests_total{method="POST",status="500"} 3\n';
    const out = parsePrometheus(text);
    expect(out).toHaveLength(2);
    expect(out[0]?.name).toBe("relay_requests_total");
    expect(out[0]?.labels).toEqual({ method: "GET", status: "200" });
    expect(out[0]?.value).toBe(142);
    expect(out[1]?.labels.status).toBe("500");
  });

  it("parses metric names containing a colon", () => {
    const text = "node:load1:avg 0.25\n";
    const [sample] = parsePrometheus(text);
    expect(sample?.name).toBe("node:load1:avg");
    expect(sample?.value).toBeCloseTo(0.25);
  });

  it("handles signed and fractional values", () => {
    const text = [
      "positive 1.5e2",
      "negative -3.14",
      "plus_sign +42",
      "zero 0",
    ].join("\n");
    const out = parsePrometheus(text);
    expect(out).toHaveLength(4);
    expect(out[0]?.value).toBeCloseTo(150);
    expect(out[1]?.value).toBeCloseTo(-3.14);
    expect(out[2]?.value).toBe(42);
    expect(out[3]?.value).toBe(0);
  });

  it("recognises Inf / -Inf / NaN", () => {
    const text = "a +Inf\nb -Inf\nc NaN\n";
    const out = parsePrometheus(text);
    expect(out[0]?.value).toBe(Number.POSITIVE_INFINITY);
    expect(out[1]?.value).toBe(Number.NEGATIVE_INFINITY);
    expect(Number.isNaN(out[2]?.value)).toBe(true);
  });

  it("recovers from a malformed line and continues", () => {
    const text = [
      "good_metric 1",
      "bad_metric {broken",
      "still_good 2",
    ].join("\n");
    const out = parsePrometheus(text);
    const names = out.map((s) => s.name).sort();
    expect(names).toContain("good_metric");
    expect(names).toContain("still_good");
  });

  it("ignores escaped quotes inside label values gracefully", () => {
    const text = 'labelled{name="hello"} 5\n';
    const out = parsePrometheus(text);
    expect(out[0]?.labels.name).toBe("hello");
    expect(out[0]?.value).toBe(5);
  });
});

describe("findSample", () => {
  const samples = parsePrometheus(
    [
      'http_requests_total{code="200"} 10',
      'http_requests_total{code="404"} 2',
      "relay_modules_total 5",
    ].join("\n"),
  );

  it("finds by name", () => {
    expect(findSample(samples, "relay_modules_total")?.value).toBe(5);
  });

  it("filters by labels", () => {
    expect(findSample(samples, "http_requests_total", { code: "404" })?.value).toBe(2);
  });

  it("returns undefined when no match", () => {
    expect(findSample(samples, "no_such_metric")).toBeUndefined();
    expect(findSample(samples, "http_requests_total", { code: "500" })).toBeUndefined();
  });
});
