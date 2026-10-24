// Copied from rpg_creator src/editor/NumberField.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import type { AriaAttributes, ChangeEvent } from 'react';

/**
 * How a non-finite parsed value (for example the intermediate text `1e`
 * while typing `1e5`, or an empty field when `emptyAs` is `nan`) is
 * handled before `onChange` runs.
 *
 * - `passthrough` emits the value unchanged, including `NaN`.
 * - `minimum` substitutes the `min` bound (or `0` when no `min` is set).
 * - `ignore` skips `onChange` entirely.
 */
export type NumberFieldFallback = 'passthrough' | 'minimum' | 'ignore';

/**
 * Integer coercion applied before clamping. `true` is shorthand for the
 * dominant `trunc` behavior; `round` covers the sprite/frame helpers.
 */
export type NumberFieldInteger = boolean | 'trunc' | 'round';

export interface NumberFieldProps extends Pick<AriaAttributes, 'aria-invalid' | 'aria-describedby'> {
  label: string;
  value: number;
  onChange: (value: number) => void;
  /** Accessible name for the input when it differs from the visible label. */
  ariaLabel?: string;
  /** Native lower bound; also the clamp bound when `clamp` is set. */
  min?: number;
  /** Native upper bound; also the clamp bound when `clamp` is set. */
  max?: number;
  step?: number;
  /** Clamp the emitted value into the defined `min`/`max` bounds. */
  clamp?: boolean;
  integer?: NumberFieldInteger;
  /**
   * Value produced for empty text: `zero` matches `Number('')` (the
   * dominant editor behavior), `nan` matches `input.valueAsNumber`.
   */
  emptyAs?: 'zero' | 'nan';
  fallback?: NumberFieldFallback;
  disabled?: boolean;
  /** Dense dimensions via `editor-control--compact`. */
  compact?: boolean;
  /** Extra feature layout classes on the field label. */
  className?: string;
  /** Extra feature classes on the input itself. */
  inputClassName?: string;
}

export function NumberField({
  label,
  value,
  onChange,
  ariaLabel,
  'aria-invalid': ariaInvalid,
  'aria-describedby': ariaDescribedBy,
  min,
  max,
  step,
  clamp = false,
  integer = false,
  emptyAs = 'zero',
  fallback = 'passthrough',
  disabled = false,
  compact = false,
  className,
  inputClassName
}: NumberFieldProps) {
  const handleChange = (event: ChangeEvent<HTMLInputElement>) => {
    let next =
      emptyAs === 'nan'
        ? event.target.valueAsNumber
        : Number(event.target.value);
    if (!Number.isFinite(next)) {
      if (fallback === 'ignore') {
        return;
      }
      if (fallback === 'minimum') {
        next = min ?? 0;
      }
    }
    const integerMode = integer === true ? 'trunc' : integer;
    if (integerMode === 'trunc') {
      next = Math.trunc(next);
    } else if (integerMode === 'round') {
      next = Math.round(next);
    }
    if (clamp) {
      if (min !== undefined) {
        next = Math.max(min, next);
      }
      if (max !== undefined) {
        next = Math.min(max, next);
      }
    }
    onChange(next);
  };
  return (
    <label
      className={['editor-field', 'editor-field-label', className]
        .filter(Boolean)
        .join(' ')}
    >
      <span>{label}</span>
      <input
        className={[
          'editor-control',
          'editor-control--text',
          compact ? 'editor-control--compact' : '',
          ariaInvalid === true || ariaInvalid === 'true' ? 'editor-control--invalid' : '',
          inputClassName ?? ''
        ]
          .filter(Boolean)
          .join(' ')}
        type="number"
        aria-label={ariaLabel}
        aria-invalid={ariaInvalid}
        aria-describedby={ariaDescribedBy}
        min={min}
        max={max}
        step={step}
        value={Number.isFinite(value) ? value : ''}
        disabled={disabled}
        onChange={handleChange}
      />
    </label>
  );
}
