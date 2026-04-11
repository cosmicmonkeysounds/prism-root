export interface FormState {
  values: Record<string, unknown>;
  errors: Record<string, string[]>;
  touched: Record<string, boolean>;
  dirty: Record<string, boolean>;
  isSubmitting: boolean;
  isValid: boolean;
}

export function createFormState(defaults: Record<string, unknown> = {}): FormState {
  return {
    values: { ...defaults },
    errors: {},
    touched: {},
    dirty: {},
    isSubmitting: false,
    isValid: true,
  };
}

export function setFieldValue(
  state: FormState,
  fieldId: string,
  value: unknown,
  originalValue: unknown,
): FormState {
  return {
    ...state,
    values: { ...state.values, [fieldId]: value },
    touched: { ...state.touched, [fieldId]: true },
    dirty: { ...state.dirty, [fieldId]: value !== originalValue },
  };
}

export function setFieldErrors(state: FormState, fieldId: string, errors: string[]): FormState {
  const newErrors = { ...state.errors, [fieldId]: errors };
  return {
    ...state,
    errors: newErrors,
    isValid: Object.values(newErrors).every((e) => e.length === 0),
  };
}

export function touchField(state: FormState, fieldId: string): FormState {
  return { ...state, touched: { ...state.touched, [fieldId]: true } };
}

export function setAllErrors(state: FormState, errors: Record<string, string[]>): FormState {
  return {
    ...state,
    errors,
    isValid: Object.values(errors).every((e) => e.length === 0),
  };
}

export function setSubmitting(state: FormState, isSubmitting: boolean): FormState {
  return { ...state, isSubmitting };
}

export function resetFormState(defaults: Record<string, unknown> = {}): FormState {
  return createFormState(defaults);
}

export function isDirty(state: FormState): boolean {
  return Object.values(state.dirty).some(Boolean);
}

export function isTouchedValid(state: FormState): boolean {
  for (const [fieldId, isTouched] of Object.entries(state.touched)) {
    if (isTouched && (state.errors[fieldId]?.length ?? 0) > 0) return false;
  }
  return true;
}

export function fieldErrors(state: FormState, fieldId: string): string[] {
  return state.errors[fieldId] ?? [];
}

export function fieldHasVisibleError(state: FormState, fieldId: string): boolean {
  return (state.touched[fieldId] ?? false) && (state.errors[fieldId]?.length ?? 0) > 0;
}
