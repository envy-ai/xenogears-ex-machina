// Copied from rpg_creator src/editor/RecordSelect.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import type { ChangeEvent } from 'react';

/**
 * Minimal record shape an entity collection must expose to be selectable.
 * Domain records (actors, skills, items, …) satisfy this structurally.
 */
export interface RecordSelectRecord {
  id: string;
  name: string;
}

export interface RecordSelectProps<
  T extends RecordSelectRecord = RecordSelectRecord
> {
  records: readonly T[];
  value: string;
  onChange: (id: string) => void;
  /**
   * Visible label above the select. Omit for compact rows that show only
   * the control; `ariaLabel` is required in that case.
   */
  label?: string;
  /** Accessible name for the select when it differs from the visible label. */
  ariaLabel?: string;
  /** Label for an empty (`''`) choice rendered before the records. */
  noneOption?: string;
  /** Keep only matching records, preserving collection order. */
  filter?: (record: T) => boolean;
  disabled?: boolean;
  /** Dense dimensions via `editor-control--compact`. */
  compact?: boolean;
  /** Extra feature layout classes on the field label. */
  className?: string;
  /** Extra feature classes on the select itself. */
  selectClassName?: string;
}

export function RecordSelect<T extends RecordSelectRecord>({
  records,
  value,
  onChange,
  label,
  ariaLabel,
  noneOption,
  filter,
  disabled = false,
  compact = false,
  className,
  selectClassName
}: RecordSelectProps<T>) {
  const handleChange = (event: ChangeEvent<HTMLSelectElement>) => {
    onChange(event.target.value);
  };
  const visibleRecords = filter ? records.filter(filter) : records;
  const select = (
    <select
      className={[
        'editor-control',
        'editor-control--select',
        compact ? 'editor-control--compact' : '',
        selectClassName ?? ''
      ]
        .filter(Boolean)
        .join(' ')}
      aria-label={ariaLabel}
      value={value}
      disabled={disabled}
      onChange={handleChange}
    >
      {noneOption !== undefined && <option value="">{noneOption}</option>}
      {visibleRecords.map((record) => (
        <option key={record.id} value={record.id}>{record.name}</option>
      ))}
    </select>
  );
  if (label === undefined) {
    return select;
  }
  return (
    <label
      className={['editor-field', 'editor-field-label', className]
        .filter(Boolean)
        .join(' ')}
    >
      <span>{label}</span>
      {select}
    </label>
  );
}
