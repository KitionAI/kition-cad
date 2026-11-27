import { useEffect, useRef, useState } from 'react'
import { Button, ColorPicker, Dialog, parseHex, rgbToHex, useKitEditor, type RGB } from '@kition/sdk/editor'
import type { CadInfo } from './cad'
import { optionKey, useDescribed, useRowWriter, type Described, type PropertyRow, type RowOption } from './properties'

/** The Properties panel's groups, in order. */
const GROUPS = ['General', 'Geometry', 'Text', 'Pattern', 'Misc']

/** A row's label as a test id part: `Center X` is `Center-X`. */
const dashed = (text: string) => text.replace(/\s+/g, '-')

/**
 * A number row: the value rounded as the drawing's units show it; typed values may be arithmetic,
 * feet and inches or carry a unit (the engine reads them). Enter, Tab and leaving the field write
 * it, Escape gives the shown value back (别扭 8); a value that does not read, or not above zero
 * where it must be, is given back too. A text field with the spin button role, as CAD's fields
 * take expressions.
 */
function NumberRow({ row, disabled }: { row: PropertyRow; disabled: boolean }) {
  const { phrase } = useKitEditor()
  const { write, parse } = useRowWriter()
  const [draft, setDraft] = useState(row.display)
  const shown = useRef(row.display)
  useEffect(() => { setDraft(row.display); shown.current = row.display }, [row.display])
  const commit = async () => {
    const text = draft.trim()
    if (text === shown.current.trim()) return
    // Written once: leaving the field after Enter writes nothing more.
    const before = shown.current
    shown.current = text
    const value = text ? await parse(text, row.unit) : null
    if (value == null || (row.positive && value <= 0)) { shown.current = before; setDraft(before); return }
    if (typeof row.value === 'number' && Math.abs(value - row.value) <= 1e-12 * Math.max(1, Math.abs(value))) return
    await write(row, value)
  }
  const label = phrase(row.label)
  return (
    <input
      className="kui-field"
      type="text"
      role="spinbutton"
      inputMode="decimal"
      aria-label={label}
      placeholder={row.varies ? phrase('*Varies*') : undefined}
      disabled={disabled}
      value={draft}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) return
        if (event.key === 'Enter') { event.preventDefault(); void commit() }
        if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); setDraft(shown.current) }
      }}
      onBlur={() => void commit()}
      data-testid={`cad-property-${dashed(row.key)}`}
    />
  )
}

/** A text row (contents, a dimension's text override): Enter or leaving it writes it; several lines take Ctrl+Enter. */
function TextRow({ row, disabled }: { row: PropertyRow; disabled: boolean }) {
  const { phrase } = useKitEditor()
  const { write } = useRowWriter()
  const value = typeof row.value === 'string' ? row.value : ''
  const [draft, setDraft] = useState(value)
  const sent = useRef(value)
  useEffect(() => { setDraft(value); sent.current = value }, [value])
  const commit = () => {
    if (draft === sent.current || (row.varies && !draft)) return
    sent.current = draft
    void write(row, draft)
  }
  const lines = value.includes('\n')
  const props = {
    className: 'kui-field',
    'aria-label': phrase(row.label),
    placeholder: row.varies ? phrase('*Varies*') : undefined,
    disabled,
    value: draft,
    onBlur: commit,
    'data-testid': `cad-property-${dashed(row.key)}`,
  }
  const keys = (event: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing) return
    if (event.key === 'Enter' && (!lines || event.metaKey || event.ctrlKey)) { event.preventDefault(); commit() }
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); setDraft(sent.current) }
  }
  return lines
    ? <textarea {...props} rows={3} onChange={(event) => setDraft(event.target.value)} onKeyDown={keys} />
    : <input {...props} type="text" onChange={(event) => setDraft(event.target.value)} onKeyDown={keys} />
}

/** "Select Color…": the colour picker, giving a true colour ("r,g,b"). */
export function ColorDialog({ initial, onPick, onClose }: { initial: string | undefined; onPick: (color: string) => void; onClose: () => void }) {
  const { phrase } = useKitEditor()
  const [rgb, setRgb] = useState<RGB>(parseHex(initial ?? '') ?? [255, 255, 255])
  return (
    <Dialog
      title={phrase('Select Color')}
      onClose={onClose}
      testId="cad-color-dialog"
      actions={<>
        <Button variant="secondary" onClick={onClose}>{phrase('Cancel')}</Button>
        <Button variant="primary" onClick={() => { onPick(rgb.join(',')); onClose() }} data-testid="cad-color-dialog-ok">{phrase('OK')}</Button>
      </>}
    >
      <ColorPicker value={rgb} onChange={(next) => setRgb(next)} testId="cad-color-picker" />
      <div className="cad-color-value"><span className="cad-swatch" style={{ background: rgbToHex(rgb) }} />{rgb.join(', ')}</div>
    </Dialog>
  )
}

/** The value a "Select Color…" choice stands for in a colour `<select>`. */
export const PICK_COLOR = '\u0000pick'

/**
 * A choice row as a `<select>`: the choices the engine gives (a colour's swatch beside it); where
 * the objects disagree an empty choice shows "*Varies*". A library linetype not loaded yet loads
 * first; a colour row offers "Select Color…" for a true colour.
 */
function ChoiceRow({ row, disabled, testId, ariaLabel }: { row: PropertyRow; disabled: boolean; testId: string; ariaLabel: string }) {
  const { phrase, run } = useKitEditor()
  const { write } = useRowWriter()
  const [picking, setPicking] = useState(false)
  const options: RowOption[] = row.options ?? []
  const current = optionKey(row.value)
  const known = options.some((option) => optionKey(option.value) === current)
  const choose = async (key: string) => {
    if (key === PICK_COLOR) { setPicking(true); return }
    const option = options.find((item) => optionKey(item.value) === key)
    if (!option) return
    if (option.load) await run('property.linetype', { load: option.value })
    await write(row, option.value)
  }
  const loaded = options.filter((option) => !option.load)
  const library = options.filter((option) => option.load)
  return (
    <span className="cad-choice">
      {row.kind === 'color' && <span className="cad-swatch" style={{ background: row.rgb ?? 'transparent' }} data-testid={`${testId}-swatch`} />}
      <select className="kui-field" aria-label={ariaLabel} value={row.varies ? '' : current} disabled={disabled} onChange={(event) => void choose(event.target.value)} data-testid={testId}>
        {row.varies && <option value="" disabled>{row.key === 'layer' ? phrase('Multiple layers') : phrase('*Varies*')}</option>}
        {!row.varies && !known && <option value={current}>{row.display}</option>}
        {loaded.map((option) => <option key={optionKey(option.value)} value={optionKey(option.value)}>{phrase(option.label)}</option>)}
        {library.length ? <optgroup label={phrase('Load')}>{library.map((option) => <option key={optionKey(option.value)} value={optionKey(option.value)}>{option.label}</option>)}</optgroup> : null}
        {row.kind === 'color' && <option value={PICK_COLOR}>{phrase('Select Color...')}</option>}
      </select>
      {picking && <ColorDialog initial={row.rgb} onPick={(color) => void write(row, color)} onClose={() => setPicking(false)} />}
    </span>
  )
}

/**
 * A polyline's current vertex (counted from 1) with buttons to the one before and after, as CAD's
 * panel steps through them; the Vertex X and Y rows below show and move that vertex.
 */
function VertexRow({ row, count, onVertex }: { row: PropertyRow; count: number; onVertex: (index: number) => void }) {
  const { phrase } = useKitEditor()
  const current = typeof row.value === 'number' ? row.value : 1
  const go = (next: number) => onVertex(Math.max(0, Math.min(count - 1, next - 1)))
  return (
    <span className="cad-vertex" role="group" aria-label={phrase(row.label)}>
      <button type="button" className="kui-icon-button" aria-label={phrase('Previous vertex')} title={phrase('Previous vertex')} disabled={row.varies || current <= 1} onClick={() => go(current - 1)} data-testid="cad-property-vertex-previous">‹</button>
      <span className="cad-vertex-number" data-testid="cad-property-currentVertex">{row.varies ? phrase('*Varies*') : row.display}</span>
      <button type="button" className="kui-icon-button" aria-label={phrase('Next vertex')} title={phrase('Next vertex')} disabled={row.varies || current >= count} onClick={() => go(current + 1)} data-testid="cad-property-vertex-next">›</button>
    </span>
  )
}

function Row({ row, locked, vertices, onVertex }: { row: PropertyRow; locked: boolean; vertices: number; onVertex: (index: number) => void }) {
  const { phrase, enabled } = useKitEditor()
  const disabled = locked || !row.editable || !row.write || !enabled(row.write.command)
  const label = phrase(row.label)
  const field = (() => {
    if (row.key === 'currentVertex') return <VertexRow row={row} count={vertices} onVertex={onVertex} />
    switch (row.kind) {
      case 'number':
        return <NumberRow row={row} disabled={disabled} />
      case 'text':
        return <TextRow row={row} disabled={disabled} />
      default:
        // The layer keeps the test id and name it had before the panel described everything.
        return row.key === 'layer'
          ? <ChoiceRow row={row} disabled={disabled} testId="cad-selection-layer" ariaLabel={phrase('Object layer')} />
          : <ChoiceRow row={row} disabled={disabled} testId={`cad-property-${dashed(row.key)}`} ariaLabel={label} />
    }
  })()
  // The vertex buttons are not in a label (a click on its text would press the first of them).
  if (row.key === 'currentVertex') return <div className="cad-property" data-testid={`cad-property-row-${dashed(row.key)}`}><span title={label}>{label}</span>{field}</div>
  return <label className="cad-property" data-testid={`cad-property-row-${dashed(row.key)}`}><span title={label}>{label}</span>{field}</label>
}

/**
 * The Properties panel (walkthrough N13): every general property and each kind's own, rounded as the
 * drawing's units show them, "*Varies*" where several objects disagree; with nothing selected the
 * current properties new objects get, which may be changed here too.
 */
export function PropertiesPanel({ info, described, onVertex }: { info: CadInfo; described: Described | null; onVertex: (index: number) => void }) {
  const { t, phrase } = useKitEditor()
  const objects = info.selectedObjects
  const type = objects.length && objects.every((object) => object.type === objects[0].type) ? objects[0].type : 'Multiple types'
  // The description follows the selection; until it has, nothing stale shows.
  const fresh = described && (described.current ? objects.length === 0 : described.count === objects.length)
  const rows = fresh ? described.rows : []
  const count = rows.find((row) => row.key === 'vertices')?.value
  const vertices = typeof count === 'number' ? count : 0
  return (
    <div className="cad-properties" data-testid="cad-selection-properties">
      <div className="cad-properties-title">
        <span>{info.selection ? t('selected', { count: info.selection }) : phrase('Current properties')}</span>
        {objects.length ? <span className="kui-muted" data-testid="cad-selection-type">{phrase(type)}</span> : null}
      </div>
      {GROUPS.map((group) => {
        const own = rows.filter((row) => row.group === group)
        if (!own.length) return null
        return (
          <section key={group} className="cad-property-group" data-testid={`cad-property-group-${group}`}>
            <div className="cad-property-group-title">{phrase(group)}</div>
            {own.map((row) => <Row key={row.key} row={row} locked={described?.locked ?? false} vertices={vertices} onVertex={onVertex} />)}
          </section>
        )
      })}
    </div>
  )
}
