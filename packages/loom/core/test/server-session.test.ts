import { describe, expect, it } from "vitest";
import { SessionStore } from "../server/session.ts";

describe("SessionStore — composable capabilities", () => {
  it("a performer can scan but not moderate", () => {
    const s = new SessionStore();
    s.set("p", { character: "Recruiter", admin: false });
    expect(s.canScan("p")).toBe(true);
    expect(s.canModerate("p")).toBe(false);
    expect(s.characterOf("p")).toBe("Recruiter");
  });

  it("a headless admin can scan + moderate, with no character", () => {
    const s = new SessionStore();
    s.set("a", { character: null, admin: true });
    expect(s.canScan("a")).toBe(true);
    expect(s.canModerate("a")).toBe(true);
    expect(s.characterOf("a")).toBeNull();
  });

  it("a performer upgrades to performer+admin on the SAME token", () => {
    const s = new SessionStore();
    s.set("p", { character: "Recruiter", admin: false });
    expect(s.grant("p", { admin: true })).toBe(true);
    expect(s.canModerate("p")).toBe(true);
    expect(s.characterOf("p")).toBe("Recruiter"); // kept
  });

  it("an admin can pick up a character (symmetric compose)", () => {
    const s = new SessionStore();
    s.set("a", { character: null, admin: true });
    s.grant("a", { character: "Sentinel" });
    expect(s.characterOf("a")).toBe("Sentinel");
    expect(s.canModerate("a")).toBe(true);
  });

  it("grant on an unknown token fails; capabilities reject unknown tokens", () => {
    const s = new SessionStore();
    expect(s.grant("ghost", { admin: true })).toBe(false);
    expect(s.canScan("ghost")).toBe(false);
    expect(s.canScan(undefined)).toBe(false);
    expect(s.canModerate(undefined)).toBe(false);
  });

  it("round-trips through entries()/load() for persistence", () => {
    const s = new SessionStore();
    s.set("p", { character: "Recruiter", admin: true });
    s.set("a", { character: null, admin: true });
    const restored = new SessionStore();
    restored.load(s.entries());
    expect(restored.get("p")).toEqual({ character: "Recruiter", admin: true });
    expect(restored.get("a")).toEqual({ character: null, admin: true });
  });
});
