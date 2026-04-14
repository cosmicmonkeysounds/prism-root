/**
 * Tests for the pure `buildRegistration` factory that backs
 * `useRegistration`. We exercise the pipeline directly so the logic is
 * covered without mounting React — mirroring the rest of Studio's
 * panel tests, which target extracted pure helpers.
 */

import { describe, expect, it, vi } from "vitest";
import {
  buildRegistration,
  type RegistrationKernel,
} from "./use-registration.js";

interface FakeNotification {
  title: string;
  kind: "warning" | "success";
}

function makeKernel(): { kernel: RegistrationKernel; added: FakeNotification[] } {
  const added: FakeNotification[] = [];
  const kernel: RegistrationKernel = {
    notifications: {
      add: (n) => {
        added.push(n);
      },
    },
  };
  return { kernel, added };
}

interface Entity {
  id: string;
  label: string;
}

describe("buildRegistration", () => {
  it("registers a def and emits a success notification", () => {
    const { kernel, added } = makeKernel();
    const store = new Map<string, Entity>();
    const register = vi.fn((def: Entity) => {
      store.set(def.id, def);
    });
    const onSuccess = vi.fn();

    const run = buildRegistration<Entity>(kernel, {
      noun: "entity",
      name: (def) => def.label,
      exists: (def) => store.has(def.id),
      register,
      onSuccess,
    });

    const ok = run({ id: "t1", label: "Task" });

    expect(ok).toBe(true);
    expect(register).toHaveBeenCalledWith({ id: "t1", label: "Task" });
    expect(store.get("t1")).toEqual({ id: "t1", label: "Task" });
    expect(added).toEqual([
      { title: 'Registered entity "Task"', kind: "success" },
    ]);
    expect(onSuccess).toHaveBeenCalledWith({ id: "t1", label: "Task" });
  });

  it("blocks when the existence check fails and emits a warning", () => {
    const { kernel, added } = makeKernel();
    const store = new Map<string, Entity>([["t1", { id: "t1", label: "Task" }]]);
    const register = vi.fn();

    const run = buildRegistration<Entity>(kernel, {
      noun: "entity",
      name: (def) => def.label,
      exists: (def) => store.has(def.id),
      register,
    });

    const ok = run({ id: "t1", label: "Task" });

    expect(ok).toBe(false);
    expect(register).not.toHaveBeenCalled();
    expect(added).toEqual([
      { title: 'Entity "Task" already exists', kind: "warning" },
    ]);
  });

  it("blocks when validate returns an error message", () => {
    const { kernel, added } = makeKernel();
    const register = vi.fn();

    const run = buildRegistration<Entity>(kernel, {
      noun: "entity",
      name: (def) => def.label,
      validate: (def) => (def.label ? null : "Label required"),
      exists: () => false,
      register,
    });

    const ok = run({ id: "t1", label: "" });

    expect(ok).toBe(false);
    expect(register).not.toHaveBeenCalled();
    expect(added).toEqual([{ title: "Label required", kind: "warning" }]);
  });

  it("defaults the noun to 'entry' when none is supplied", () => {
    const { kernel, added } = makeKernel();
    const register = vi.fn();

    const run = buildRegistration<Entity>(kernel, {
      name: (def) => def.label,
      exists: () => false,
      register,
    });

    run({ id: "x", label: "X" });

    expect(added).toEqual([
      { title: 'Registered entry "X"', kind: "success" },
    ]);
  });

  it("runs validate before exists", () => {
    const { kernel } = makeKernel();
    const existsSpy = vi.fn().mockReturnValue(true);
    const register = vi.fn();

    const run = buildRegistration<Entity>(kernel, {
      noun: "entity",
      name: (def) => def.label,
      validate: () => "nope",
      exists: existsSpy,
      register,
    });

    run({ id: "t1", label: "Task" });

    expect(existsSpy).not.toHaveBeenCalled();
    expect(register).not.toHaveBeenCalled();
  });

  it("capitalises the noun in the conflict message", () => {
    const { kernel, added } = makeKernel();

    const run = buildRegistration<Entity>(kernel, {
      noun: "relationship",
      name: (def) => def.label,
      exists: () => true,
      register: vi.fn(),
    });

    run({ id: "r1", label: "depends-on" });

    expect(added[0]?.title).toBe('Relationship "depends-on" already exists');
  });

  it("skips onSuccess when validation fails", () => {
    const { kernel } = makeKernel();
    const onSuccess = vi.fn();

    const run = buildRegistration<Entity>(kernel, {
      name: (def) => def.label,
      validate: () => "err",
      exists: () => false,
      register: vi.fn(),
      onSuccess,
    });

    run({ id: "x", label: "X" });

    expect(onSuccess).not.toHaveBeenCalled();
  });
});
