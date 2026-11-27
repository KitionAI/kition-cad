import { useEffect, useState } from 'react'
import { Button, Checkbox, Dialog, Segmented, useKitEditor } from '@kition/sdk/editor'
import type { CadInfo } from './cad'
import { OBJECT_SNAPS, POLAR_ANGLES } from './StatusBar'

/** The dialog's pages, as the reference's tabs. */
type Page = 'snap' | 'polar' | 'osnap' | 'dynamic'

/** Every running object snap mode's bits together. */
const ALL_SNAPS = OBJECT_SNAPS.reduce((bits, mode) => bits | mode.bit, 0)

/** What the dialog edits: the modes on, the spacings and angles as typed. */
type Settings = {
  snap: boolean
  grid: boolean
  polar: boolean
  otrack: boolean
  osnap: boolean
  dynamicInput: boolean
  snapX: string
  snapY: string
  gridX: string
  gridY: string
  gridMajor: string
  polarAngle: string
  osmode: number
}

const positive = (text: string) => {
  const value = Number(text)
  return text.trim() !== '' && Number.isFinite(value) && value > 0 ? value : null
}

/**
 * Drafting Settings (the status bar's gear, Tools › Drafting Settings...): Snap and Grid, Polar
 * Tracking, Object Snap and Dynamic Input on four pages. It reads the drawing's settings when it
 * opens and OK writes what changed through the commands the status bar and the command line use
 * (drafting.snap, drafting.grid, drafting.polar, drafting.objectSnapTracking, drafting.objectSnap,
 * drafting.dynamicInput, variable.set SNAPUNIT, GRIDUNIT, GRIDMAJOR, POLARANG), all transparent: a
 * running command keeps running.
 */
export function DraftingSettings({ info, onClose }: { info: CadInfo; onClose: () => void }) {
  const { phrase, query, run, byId } = useKitEditor()
  const d = info.drafting
  const [page, setPage] = useState<Page>('snap')
  const initial: Settings = {
    snap: d.snap,
    grid: d.grid,
    polar: d.polar,
    otrack: d.otrack,
    osnap: d.osnap,
    dynamicInput: d.dynamicInput,
    snapX: '',
    snapY: '',
    gridX: String(d.gridUnit[0]),
    gridY: String(d.gridUnit[1]),
    gridMajor: String(d.gridMajor),
    polarAngle: String(d.polarAngle ?? 90),
    osmode: d.osmode ?? 0,
  }
  const [start, setStart] = useState<Settings>(initial)
  const [now, setNow] = useState<Settings>(initial)
  // The snap spacing is not in the editor state: asked once, when the dialog opens.
  useEffect(() => {
    let gone = false
    void query<{ value?: unknown }>('variable.get', { name: 'SNAPUNIT' }).then((answer) => {
      const value = Array.isArray(answer?.value) ? answer.value.map(Number) : [10, 10]
      if (gone) return
      const apply = (s: Settings): Settings => ({ ...s, snapX: String(value[0] ?? 10), snapY: String(value[1] ?? value[0] ?? 10) })
      setStart(apply)
      setNow(apply)
    })
    return () => { gone = true }
  }, [query])
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => setNow((s) => ({ ...s, [key]: value }))
  const numbers = {
    snapX: positive(now.snapX), snapY: positive(now.snapY), gridX: positive(now.gridX), gridY: positive(now.gridY),
    gridMajor: positive(now.gridMajor), polarAngle: positive(now.polarAngle),
  }
  const loaded = now.snapX !== ''
  const valid = loaded && Object.values(numbers).every((value) => value != null) && (numbers.gridMajor ?? 0) <= 100
  const named = (command: string, fallback: string) => {
    const known = byId.get(command)
    const name = phrase(fallback)
    return known?.shortcut ? `${name} (${known.shortcut})` : name
  }
  const ok = async () => {
    if (!valid) return
    onClose()
    const changed = <K extends keyof Settings>(key: K) => now[key] !== start[key]
    if (changed('snapX') || changed('snapY')) await run('variable.set', { name: 'SNAPUNIT', value: [numbers.snapX, numbers.snapY] })
    if (changed('gridX') || changed('gridY')) await run('variable.set', { name: 'GRIDUNIT', value: [numbers.gridX, numbers.gridY] })
    if (changed('gridMajor')) await run('variable.set', { name: 'GRIDMAJOR', value: Math.round(numbers.gridMajor ?? 5) })
    if (changed('polarAngle')) await run('variable.set', { name: 'POLARANG', value: numbers.polarAngle })
    // OSMODE first (setting it turns running snaps on), then whether they are on.
    if (changed('osmode')) await run('drafting.objectSnap', { osmode: now.osmode })
    if (changed('osmode') || changed('osnap')) await run('drafting.objectSnap', { on: now.osnap })
    const toggles: Array<[keyof Settings, string]> = [['snap', 'drafting.snap'], ['grid', 'drafting.grid'], ['polar', 'drafting.polar'], ['otrack', 'drafting.objectSnapTracking'], ['dynamicInput', 'drafting.dynamicInput']]
    for (const [key, command] of toggles) if (changed(key)) await run(command, { on: now[key] })
  }
  const field = (key: 'snapX' | 'snapY' | 'gridX' | 'gridY' | 'gridMajor', label: string) => (
    <label className="cad-property"><span>{phrase(label)}</span>
      <input className="kui-field" type="text" inputMode="decimal" value={now[key]} aria-invalid={numbers[key] == null} onChange={(event) => set(key, event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); void ok() } }} data-testid={`cad-settings-${key}`} />
    </label>
  )
  const angles = POLAR_ANGLES.map(String)
  return (
    <Dialog
      title={phrase('Drafting Settings')}
      onClose={onClose}
      testId="cad-drafting-settings"
      actions={<>
        <Button variant="secondary" onClick={onClose}>{phrase('Cancel')}</Button>
        <Button variant="primary" disabled={!valid} onClick={() => void ok()} data-testid="cad-settings-ok">{phrase('OK')}</Button>
      </>}
    >
      <div className="cad-dialog-fields cad-settings">
        <Segmented
          label={phrase('Drafting Settings')}
          value={page}
          onChange={setPage}
          segments={[
            { value: 'snap', label: phrase('Snap and Grid') },
            { value: 'polar', label: phrase('Polar Tracking') },
            { value: 'osnap', label: phrase('Object Snap') },
            { value: 'dynamic', label: phrase('Dynamic Input') },
          ]}
          testId="cad-settings-page"
        />
        {page === 'snap' ? (
          <>
            <Checkbox label={named('drafting.snap', 'Snap On')} checked={now.snap} onChange={(on) => set('snap', on)} testId="cad-settings-snap" />
            {field('snapX', 'Snap X spacing')}
            {field('snapY', 'Snap Y spacing')}
            <Checkbox label={named('drafting.grid', 'Grid On')} checked={now.grid} onChange={(on) => set('grid', on)} testId="cad-settings-grid" />
            {field('gridX', 'Grid X spacing')}
            {field('gridY', 'Grid Y spacing')}
            {field('gridMajor', 'Major line every')}
          </>
        ) : page === 'polar' ? (
          <>
            <Checkbox label={named('drafting.polar', 'Polar Tracking On')} checked={now.polar} onChange={(on) => set('polar', on)} testId="cad-settings-polar" />
            <label className="cad-property"><span>{phrase('Increment angle')}</span>
              <select className="kui-field" value={now.polarAngle} onChange={(event) => set('polarAngle', event.target.value)} data-testid="cad-settings-polarAngle">
                {!angles.includes(now.polarAngle) && <option value={now.polarAngle}>{`${now.polarAngle}°`}</option>}
                {angles.map((angle) => <option key={angle} value={angle}>{`${angle}°`}</option>)}
              </select>
            </label>
            <Checkbox label={named('drafting.objectSnapTracking', 'Object Snap Tracking On')} checked={now.otrack} onChange={(on) => set('otrack', on)} testId="cad-settings-otrack" />
          </>
        ) : page === 'osnap' ? (
          <>
            <Checkbox label={named('drafting.objectSnap', 'Object Snap On')} checked={now.osnap} onChange={(on) => set('osnap', on)} testId="cad-settings-osnap" />
            <Checkbox label={named('drafting.objectSnapTracking', 'Object Snap Tracking On')} checked={now.otrack} onChange={(on) => set('otrack', on)} testId="cad-settings-osnap-otrack" />
            <div className="cad-settings-snaps">
              {OBJECT_SNAPS.map((mode) => (
                <Checkbox key={mode.bit} label={phrase(mode.label)} checked={(now.osmode & mode.bit) !== 0} onChange={(on) => set('osmode', on ? now.osmode | mode.bit : now.osmode & ~mode.bit)} testId={`cad-settings-snap-${mode.bit}`} />
              ))}
            </div>
            <div className="cad-settings-buttons">
              <Button variant="secondary" onClick={() => set('osmode', ALL_SNAPS)} data-testid="cad-settings-snap-all">{phrase('Select All')}</Button>
              <Button variant="secondary" onClick={() => set('osmode', 0)} data-testid="cad-settings-snap-none">{phrase('Clear All')}</Button>
            </div>
          </>
        ) : (
          <>
            <Checkbox label={named('drafting.dynamicInput', 'Enable Dynamic Input')} checked={now.dynamicInput} onChange={(on) => set('dynamicInput', on)} testId="cad-settings-dynamicInput" />
            <div className="kui-muted cad-dialog-note">{phrase('The prompt and the value boxes beside the cursor: X and Y at a first point, distance and angle after it, and at a hot grip.')}</div>
          </>
        )}
      </div>
    </Dialog>
  )
}
