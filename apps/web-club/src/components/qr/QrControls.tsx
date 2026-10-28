/**
 * Studio control primitives.
 *
 * These are the only widgets the studio adds to the club panel kit, and each
 * exists because the kit has nothing that fits:
 *
 *   `ColorField`  the panel has no colour input, and a native `<input
 *                 type="color">` alone is unusable on a projector — the hex
 *                 value must stay visible and typable.
 *   `RangeField`  a slider needs a visible value and a stable width; the raw
 *                 input is 16px tall and fails the 44px touch rule on its own,
 *                 so the hit area is the row, not the track.
 *   `PresetTile`  a style choice is a picture, not a radio label.
 *
 * They all stamp `data-control` so `scripts/panel-audit.mjs` measures them like
 * every other control on the panel (10px radius, 36/44px height).
 */
import { useId, type ReactNode } from 'react';
import { parseHex } from '../../qr/color';

export interface ColorFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** Shown under the field; keep it to one line of consequence. */
  hint?: string;
  disabled?: boolean;
}

export function ColorField({ label, value, onChange, hint, disabled = false }: ColorFieldProps) {
  const id = useId();
  const valid = parseHex(value) !== null;
  return (
    <div className="qr-field">
      <label className="field-label" htmlFor={`${id}-hex`}>{label}</label>
      <div className="qr-color-row">
        <span
          className="qr-color-swatch"
          style={{ background: valid ? value : 'transparent' }}
          aria-hidden="true"
          data-invalid={valid ? undefined : 'true'}
        />
        <input
          className="qr-color-native"
          type="color"
          value={valid ? value : '#000000'}
          onChange={(event) => onChange(event.target.value)}
          aria-label={label}
          disabled={disabled}
          data-control="icon"
        />
        <input
          id={`${id}-hex`}
          className="field-input qr-color-hex is-mono"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={valid ? undefined : 'true'}
          aria-describedby={hint ? `${id}-hint` : undefined}
          disabled={disabled}
        />
      </div>
      {hint ? <p className="field-hint" id={`${id}-hint`}>{hint}</p> : null}
    </div>
  );
}

export interface RangeFieldProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  /** Formats the numeric readout, e.g. half-modules to a percentage. */
  format?: (value: number) => string;
  hint?: string;
  disabled?: boolean;
}

export function RangeField({
  label,
  value,
  min,
  max,
  step,
  onChange,
  format,
  hint,
  disabled = false,
}: RangeFieldProps) {
  const id = useId();
  return (
    <div className="qr-field">
      <div className="qr-field-head">
        <label className="field-label" htmlFor={`${id}-range`}>{label}</label>
        <span className="qr-field-value is-mono">{format ? format(value) : String(value)}</span>
      </div>
      <input
        id={`${id}-range`}
        className="qr-range"
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        aria-label={label}
        aria-valuetext={format ? format(value) : undefined}
        aria-describedby={hint ? `${id}-hint` : undefined}
        disabled={disabled}
        data-control="field"
      />
      {hint ? <p className="field-hint" id={`${id}-hint`}>{hint}</p> : null}
    </div>
  );
}

export interface ChoiceTileProps {
  label: string;
  selected: boolean;
  onSelect: () => void;
  /** Short note under the label. */
  hint?: string;
  /** A drawn preview; the reason this control is a tile and not a radio. */
  preview?: ReactNode;
  name: string;
}

export function ChoiceTile({ label, selected, onSelect, hint, preview, name }: ChoiceTileProps) {
  return (
    <button
      type="button"
      className={`qr-choice ${selected ? 'is-selected' : ''}`}
      onClick={onSelect}
      aria-pressed={selected}
      data-control="button"
      data-choice={name}
    >
      {preview ? <span className="qr-choice-preview" aria-hidden="true">{preview}</span> : null}
      <span className="qr-choice-label">{label}</span>
      {hint ? <span className="qr-choice-hint">{hint}</span> : null}
    </button>
  );
}

export interface SwitchFieldProps {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  hint?: string;
  disabled?: boolean;
}

export function SwitchField({ label, checked, onChange, hint, disabled = false }: SwitchFieldProps) {
  const id = useId();
  return (
    <div className="qr-switch-row">
      {/* The whole row is one target: a 16px checkbox is not a touch target, and
          a hint outside the label is a second, dead click area. The native input
          is stretched over the row at zero opacity so it keeps its own keyboard
          behaviour (Space, focus ring) while the painted box below it shows state. */}
      <input
        id={id}
        className="qr-switch"
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        aria-describedby={hint ? `${id}-hint` : undefined}
        disabled={disabled}
      />
      <span className="qr-switch-box" aria-hidden="true" data-checked={checked ? 'true' : 'false'} />
      <label htmlFor={id} className="qr-switch-copy">
        <span className="qr-switch-label">{label}</span>
        {hint ? <span className="field-hint" id={`${id}-hint`}>{hint}</span> : null}
      </label>
    </div>
  );
}
