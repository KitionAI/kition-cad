import { useState } from 'react'
import { Button, Dialog, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import { DESCRIBE, type Described } from './properties'

/**
 * What the hatch dialog does: set up a new hatch (Draw › Hatch…; OK starts HATCH with the pattern,
 * scale and angle, at "Pick internal point"), or change hatches (a double-click, Edit Hatch).
 */
export type HatchEdit = { mode: 'new' } | { mode: 'edit'; handles: string[]; pattern: string; scale: number; angle: number }

/**
 * The hatch's pattern, scale and angle. For a new hatch OK starts HATCH with them, answering its
 * options at the command line (seTtings, sCale, Angle), so the history shows them and the person
 * picks internal points next, the area under the cursor previewed; for hatches OK runs
 * `hatch.edit`, one undo step.
 */
export function HatchDialog({ edit, onClose, start }: { edit: HatchEdit; onClose: () => void; start: (entries: string[]) => void }) {
  const { phrase, run } = useKitEditor()
  const patterns = useEngineQuery<Described>(DESCRIBE, { handles: [] })?.patterns ?? []
  const [pattern, setPattern] = useState(edit.mode === 'edit' ? edit.pattern : 'ANSI31')
  const [scale, setScale] = useState(edit.mode === 'edit' ? String(edit.scale) : '1')
  const [angle, setAngle] = useState(edit.mode === 'edit' ? String(edit.angle) : '0')
  const scaleValue = Number(scale)
  const angleValue = Number(angle)
  const valid = pattern.trim() !== '' && Number.isFinite(scaleValue) && scaleValue > 0 && Number.isFinite(angleValue)
  const ok = () => {
    if (!valid) return
    onClose()
    if (edit.mode === 'edit') void run('hatch.edit', { handles: edit.handles, pattern, scale: scaleValue, angle: angleValue })
    // The angle typed with `<<`-free degrees: HATCH reads it in the drawing's angle units.
    else start(['hatch', 't', pattern, 'c', String(scaleValue), 'a', String(angleValue)])
  }
  return (
    <Dialog
      title={phrase(edit.mode === 'edit' ? 'Hatch Edit' : 'Hatch')}
      onClose={onClose}
      testId="cad-hatch-dialog"
      actions={<>
        <Button variant="secondary" onClick={onClose}>{phrase('Cancel')}</Button>
        <Button variant="primary" disabled={!valid} onClick={ok} data-testid="cad-hatch-dialog-ok">{phrase(edit.mode === 'edit' ? 'OK' : 'Pick Points')}</Button>
      </>}
    >
      <div className="cad-dialog-fields">
        <label className="cad-property"><span>{phrase('Pattern')}</span>
          <select className="kui-field" value={pattern} onChange={(event) => setPattern(event.target.value)} data-testid="cad-hatch-pattern">
            {!patterns.includes(pattern) && <option value={pattern}>{pattern}</option>}
            {patterns.map((name) => <option key={name} value={name}>{name}</option>)}
          </select>
        </label>
        <label className="cad-property"><span>{phrase('Scale')}</span>
          <input className="kui-field" type="text" inputMode="decimal" value={scale} onChange={(event) => setScale(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') ok() }} data-testid="cad-hatch-scale" />
        </label>
        <label className="cad-property"><span>{phrase('Angle')}</span>
          <input className="kui-field" type="text" inputMode="decimal" value={angle} onChange={(event) => setAngle(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') ok() }} data-testid="cad-hatch-angle" />
        </label>
      </div>
    </Dialog>
  )
}
