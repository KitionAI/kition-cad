import { useEffect, useState } from 'react'
import { useKitEditor } from '@kition/sdk/editor'
import type { CadInfo } from './cad'

function NumberField({ label, value, disabled, positive, onCommit }: { label: string; value: number; disabled: boolean; positive?: boolean; onCommit: (value: number) => void }) {
  const [draft, setDraft] = useState(String(value))
  useEffect(() => setDraft(String(value)), [value])
  return <label className="cad-property"><span>{label}</span><input className="kui-field" type="number" step="any" aria-label={label} disabled={disabled} value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => {
    if (event.key === 'Enter') event.currentTarget.blur()
    if (event.key === 'Escape') { setDraft(String(value)); event.stopPropagation() }
  }} onBlur={() => {
    const next = Number(draft)
    if (!draft.trim() || !Number.isFinite(next) || (positive && next <= 0)) { setDraft(String(value)); return }
    if (next !== value) onCommit(next)
  }} /></label>
}

export function SelectionProperties({ info }: { info: CadInfo }) {
  const { run, enabled, t, phrase } = useKitEditor()
  const objects = info.selectedObjects
  const handles = objects.map((object) => object.handle)
  const layer = objects.length && objects.every((object) => object.layer === objects[0].layer) ? objects[0].layer : ''
  const type = objects.length && objects.every((object) => object.type === objects[0].type) ? objects[0].type : 'Multiple types'
  const editable = enabled('object.setProperties') && !objects.some((object) => object.locked)
  const one = objects.length === 1 ? objects[0] : null
  const apply = (params: Record<string, unknown>) => { void run('object.setProperties', { handles, ...params }) }
  return <div className="cad-properties" data-testid="cad-selection-properties">
    <span>{info.selection ? t('selected', { count: info.selection }) : t('nothingSelected')}</span>
    {!objects.length ? <span className="kui-muted">{t('layer', { name: info.currentLayer })}</span> : <>
      <span data-testid="cad-selection-type">{phrase(type)}</span>
      <label className="cad-property"><span>{phrase('Layer')}</span><select className="kui-field" aria-label={phrase('Object layer')} value={layer} disabled={!editable} onChange={(event) => apply({ layer: event.target.value })} data-testid="cad-selection-layer">
        {!layer && <option value="" disabled>{phrase('Multiple layers')}</option>}
        {info.layers.map((item) => <option key={item.name} value={item.name}>{item.name}</option>)}
      </select></label>
      {one?.center && <>
        <NumberField label={phrase('Center X')} value={one.center[0]} disabled={!editable} onCommit={(x) => apply({ center: [x, one.center![1]] })} />
        <NumberField label={phrase('Center Y')} value={one.center[1]} disabled={!editable} onCommit={(y) => apply({ center: [one.center![0], y] })} />
      </>}
      {one?.radius !== undefined && <NumberField label={phrase('Radius')} value={one.radius} positive disabled={!editable} onCommit={(radius) => apply({ radius })} />}
    </>}
  </div>
}
