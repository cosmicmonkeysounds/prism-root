import { describe, it, expect } from "vitest";
import {
  createFormState,
  setFieldValue,
  setFieldErrors,
  touchField,
  setAllErrors,
  setSubmitting,
  resetFormState,
  isDirty,
  isTouchedValid,
  fieldErrors,
  fieldHasVisibleError,
} from "./form-state.js";

describe("createFormState", () => {
  it("creates empty state", () => {
    const state = createFormState();
    expect(state.values).toEqual({});
    expect(state.errors).toEqual({});
    expect(state.touched).toEqual({});
    expect(state.dirty).toEqual({});
    expect(state.isSubmitting).toBe(false);
    expect(state.isValid).toBe(true);
  });

  it("creates with defaults", () => {
    const state = createFormState({ name: "Alice", age: 30 });
    expect(state.values).toEqual({ name: "Alice", age: 30 });
  });
});

describe("setFieldValue", () => {
  it("sets value and marks touched and dirty", () => {
    const state = setFieldValue(createFormState(), "name", "Bob", "");
    expect(state.values["name"]).toBe("Bob");
    expect(state.touched["name"]).toBe(true);
    expect(state.dirty["name"]).toBe(true);
  });

  it("marks not dirty when value matches original", () => {
    const state = setFieldValue(createFormState(), "name", "Alice", "Alice");
    expect(state.dirty["name"]).toBe(false);
  });
});

describe("setFieldErrors", () => {
  it("sets errors for a field", () => {
    const state = setFieldErrors(createFormState(), "name", ["Required"]);
    expect(state.errors["name"]).toEqual(["Required"]);
    expect(state.isValid).toBe(false);
  });

  it("clears errors with empty array", () => {
    let state = setFieldErrors(createFormState(), "name", ["Required"]);
    state = setFieldErrors(state, "name", []);
    expect(state.errors["name"]).toEqual([]);
    expect(state.isValid).toBe(true);
  });

  it("isValid reflects all fields", () => {
    let state = setFieldErrors(createFormState(), "name", ["Required"]);
    state = setFieldErrors(state, "email", []);
    expect(state.isValid).toBe(false);
  });
});

describe("touchField", () => {
  it("marks field as touched", () => {
    const state = touchField(createFormState(), "name");
    expect(state.touched["name"]).toBe(true);
  });
});

describe("setAllErrors", () => {
  it("replaces all errors", () => {
    const state = setAllErrors(createFormState(), {
      name: ["Required"],
      email: ["Invalid"],
    });
    expect(state.errors["name"]).toEqual(["Required"]);
    expect(state.errors["email"]).toEqual(["Invalid"]);
    expect(state.isValid).toBe(false);
  });

  it("is valid when all errors empty", () => {
    const state = setAllErrors(createFormState(), { name: [], email: [] });
    expect(state.isValid).toBe(true);
  });
});

describe("setSubmitting", () => {
  it("toggles submitting flag", () => {
    const state = setSubmitting(createFormState(), true);
    expect(state.isSubmitting).toBe(true);
    expect(setSubmitting(state, false).isSubmitting).toBe(false);
  });
});

describe("resetFormState", () => {
  it("returns fresh state", () => {
    const state = resetFormState({ name: "default" });
    expect(state.values).toEqual({ name: "default" });
    expect(state.touched).toEqual({});
    expect(state.dirty).toEqual({});
  });
});

describe("isDirty", () => {
  it("returns true when any field is dirty", () => {
    const state = setFieldValue(createFormState(), "name", "Bob", "");
    expect(isDirty(state)).toBe(true);
  });

  it("returns false when clean", () => {
    expect(isDirty(createFormState())).toBe(false);
  });
});

describe("isTouchedValid", () => {
  it("returns false when touched field has errors", () => {
    let state = touchField(createFormState(), "name");
    state = setFieldErrors(state, "name", ["Required"]);
    expect(isTouchedValid(state)).toBe(false);
  });

  it("returns true when touched fields have no errors", () => {
    const state = touchField(createFormState(), "name");
    expect(isTouchedValid(state)).toBe(true);
  });
});

describe("fieldErrors", () => {
  it("returns errors for field", () => {
    const state = setFieldErrors(createFormState(), "name", ["Required"]);
    expect(fieldErrors(state, "name")).toEqual(["Required"]);
  });

  it("returns empty array when no errors", () => {
    expect(fieldErrors(createFormState(), "name")).toEqual([]);
  });
});

describe("fieldHasVisibleError", () => {
  it("returns true when touched and has errors", () => {
    let state = touchField(createFormState(), "name");
    state = setFieldErrors(state, "name", ["Required"]);
    expect(fieldHasVisibleError(state, "name")).toBe(true);
  });

  it("returns false when untouched even with errors", () => {
    const state = setFieldErrors(createFormState(), "name", ["Required"]);
    expect(fieldHasVisibleError(state, "name")).toBe(false);
  });
});
