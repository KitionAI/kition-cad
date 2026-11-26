import { useEffect, useRef, useState } from 'react'
import { CheckIcon as Check, EyeIcon as Eye, EyeOffIcon as EyeOff, LockIcon as Lock, LockOpenIcon as LockOpen, PlusIcon as Plus, SnowflakeIcon as Snowflake, SunIcon as Sun, TrashIcon as Trash, XIcon as Close } from '@kition/tessera/icons/react'
import { IconButton, RenameField, useKitEditor } from '@kition/sdk/editor'
import type { CadInfo } from './cad'
import { ColorDialog, PICK_COLOR, PropertiesPanel } from './PropertiesPanel'
import { optionKey, useDescribed, type Described, type LayerDetails } from './properties'

/** Which part of the right panel was asked for last (the ribbon's Layer Properties, the menu's Properties). */
export type PanelPart = 'properties' | 'layers'

/** A layer's colour: its swatch opens the standard colours and "Select Color…". */
function LayerColor({ layer, described, disabled }: { layer: LayerDetails; described: Described | null; disabled: boolean }) {
  const { run, phrase } = useKitEditor()
  const [picking, setPicking] = useState(false)
  const colors = described?.colors ?? []
  const known = colors.some((color) => color.value === layer.color)
  return (
    <span className="cad-choice cad-layer-color">
      <span className="cad-swatch" style={{ background: layer.rgb }} />
      <select
        className="kui-field"
        aria-label={`${phrase('Color')} ${layer.name}`}
        title={layer.color}
        value={layer.color}
        disabled={disabled}
        onChange={(event) => {
          if (event.target.value === PICK_COLOR) setPicking(true)
          else void run('layer.set', { name: layer.name, color: event.target.value })
        }}
        data-testid={`cad-layer-${layer.name}-color`}
      >
        {!known && <option value={layer.color}>{layer.color}</option>}
        {colors.map((color) => <option key={color.value} value={color.value}>{phrase(color.value)}</option>)}
        <option value={PICK_COLOR}>{phrase('Select Color...')}</option>
      </select>
      {picking && <ColorDialog initial={layer.rgb} onPick={(color) => void run('layer.set', { name: layer.name, color })} onClose={() => setPicking(false)} />}
    </span>
  )
}

/** One layer: current, name (double-click renames), on, freeze, lock, delete; colour, linetype and lineweight under it. */
function LayerRow({ layer, details, described }: { layer: CadInfo['layers'][number]; details: LayerDetails | undefined; described: Described | null }) {
  const { run, enabled, t, phrase } = useKitEditor()
  const [renaming, setRenaming] = useState(false)
  const [name, setName] = useState(layer.name)
  const canSet = enabled('layer.set')
  const current = details?.current ?? false
  const makeCurrent = () => { if (!current && enabled('layer.setCurrent')) void run('layer.setCurrent', { name: layer.name }) }
  const [message, setMessage] = useState<string | null>(null)
  useEffect(() => { if (!message) return; const timer = window.setTimeout(() => setMessage(null), 6000); return () => window.clearTimeout(timer) }, [message])
  const remove = () => {
    const blocked = details?.deleteBlocked
    if (blocked) { setMessage(t(`layerKept.${blocked}`, { name: layer.name })); return }
    void run('layer.delete', { name: layer.name })
  }
  const lineweight = details ? optionKey(details.lineweight) : ''
  const linetypes = described?.linetypes.filter((item) => !['bylayer', 'byblock'].includes(item.toLowerCase())) ?? []
  return (
    <div className="cad-layer" data-current={current} data-testid={`cad-layer-${layer.name}`} onDoubleClick={(event) => { if (!(event.target instanceof Element) || !event.target.closest('.cad-layer-name, select, button, input, .cad-layer-message')) makeCurrent() }}>
      <div className="cad-layer-line">
        <button type="button" className="cad-layer-current" title={t('makeCurrent')} aria-label={`${t('makeCurrent')} ${layer.name}`} disabled={!enabled('layer.setCurrent')} onClick={makeCurrent} data-testid={`cad-layer-${layer.name}-current`}>
          {current ? <Check size={13} aria-hidden="true" /> : null}
        </button>
        {renaming ? (
          <span className="cad-layer-name">
            <RenameField
              value={name}
              label={t('renameLayer')}
              onChange={setName}
              onDone={(commit) => {
                setRenaming(false)
                const next = name.trim()
                if (commit && next && next !== layer.name) void run('layer.set', { name: layer.name, newName: next })
                else setName(layer.name)
              }}
              testId={`cad-layer-${layer.name}-rename`}
            />
          </span>
        ) : (
          <span className="cad-layer-name" title={t('renameHint')} onDoubleClick={() => { if (canSet && layer.name !== '0') { setName(layer.name); setRenaming(true) } }} data-testid={`cad-layer-${layer.name}-name`}>
            {layer.name}
          </span>
        )}
        <span className="cad-layer-fill" title={t('makeCurrent')} data-testid={`cad-layer-${layer.name}-row`} />
        <IconButton icon={layer.on ? Eye : EyeOff} label={t('on')} pressed={!layer.on} disabled={!canSet} onClick={() => void run('layer.set', { name: layer.name, on: !layer.on })} data-testid={`cad-layer-${layer.name}-on`} />
        <IconButton icon={layer.frozen ? Snowflake : Sun} label={t('freeze')} pressed={layer.frozen} disabled={!canSet || current} onClick={() => void run('layer.set', { name: layer.name, frozen: !layer.frozen })} />
        <IconButton icon={layer.locked ? Lock : LockOpen} label={t('lock')} pressed={layer.locked} disabled={!canSet} onClick={() => void run('layer.set', { name: layer.name, locked: !layer.locked })} data-testid={`cad-layer-${layer.name}-lock`} />
        <IconButton icon={Trash} label={t('deleteLayer')} disabled={!enabled('layer.delete')} onClick={remove} data-testid={`cad-layer-${layer.name}-delete`} />
      </div>
      {details ? (
        <div className="cad-layer-line cad-layer-details">
          <LayerColor layer={details} described={described} disabled={!canSet} />
          <select className="kui-field" aria-label={`${phrase('Linetype')} ${layer.name}`} value={details.linetype} disabled={!canSet} onChange={(event) => {
            const value = event.target.value
            const load = described?.library.includes(value) ?? false
            void (load ? run('property.linetype', { load: value }) : Promise.resolve(null)).then(() => run('layer.set', { name: layer.name, linetype: value }))
          }} data-testid={`cad-layer-${layer.name}-linetype`}>
            {!linetypes.includes(details.linetype) && <option value={details.linetype}>{details.linetype}</option>}
            {linetypes.map((item) => <option key={item} value={item}>{item}</option>)}
            {described?.library.length ? <optgroup label={phrase('Load')}>{described.library.map((item) => <option key={item} value={item}>{item}</option>)}</optgroup> : null}
          </select>
          <select className="kui-field" aria-label={`${phrase('Lineweight')} ${layer.name}`} value={lineweight} disabled={!canSet} onChange={(event) => {
            const option = described?.lineweights.find((item) => optionKey(item.value) === event.target.value)
            if (option) void run('layer.set', { name: layer.name, lineweight: option.value })
          }} data-testid={`cad-layer-${layer.name}-lineweight`}>
            {(described?.lineweights ?? []).filter((item) => !['ByLayer', 'ByBlock'].includes(String(item.value))).map((item) => <option key={optionKey(item.value)} value={optionKey(item.value)}>{phrase(item.label)}</option>)}
          </select>
        </div>
      ) : null}
      {message ? <div className="cad-layer-message" role="status" data-testid={`cad-layer-${layer.name}-message`}>{message}</div> : null}
    </div>
  )
}

/**
 * The right panel: Properties above, the drawing's layers below, each in its own part with its own
 * scrolling, so the layer list stays where it is whatever is selected (别扭 7). The ribbon's Layer
 * Properties shows the panel and its layers, the drawing menu's Properties its properties; the
 * panel's own button closes it (B8).
 */
export function LayersPanel({ info, focus, onClose }: { info: CadInfo; focus: { part: PanelPart; serial: number }; onClose: () => void }) {
  const { ask, enabled, t } = useKitEditor()
  const described = useDescribed(info)
  const properties = useRef<HTMLDivElement>(null)
  const layers = useRef<HTMLDivElement>(null)
  // Both parts always show: the one asked for gets the keyboard's attention (its first field).
  useEffect(() => {
    if (!focus.serial) return
    const part = focus.part === 'layers' ? layers.current : properties.current
    part?.setAttribute('data-flash', String(focus.serial))
  }, [focus])
  return (
    <aside className="kui-panel cad-side" data-side="right" data-testid="cad-layers" data-part={focus.part}>
      <div className="kui-panel-header">
        {t('properties')}
        <span style={{ flex: 1 }} />
        <IconButton icon={Close} label={t('closePanel')} onClick={onClose} data-testid="cad-panel-close" />
      </div>
      <div className="cad-side-part cad-side-properties" ref={properties} data-testid="cad-panel-properties">
        <PropertiesPanel info={info} described={described} />
      </div>
      <div className="cad-side-part cad-side-layers" ref={layers} data-testid="cad-panel-layers">
        <div className="kui-panel-header">
          {t('layers')}
          <span style={{ flex: 1 }} />
          <IconButton icon={Plus} label={t('newLayer')} disabled={!enabled('layer.add')} onClick={() => ask('layer.add', { current: true })} data-testid="cad-new-layer" />
        </div>
        <div className="kui-panel-body">
          {info.layers.map((layer) => <LayerRow key={layer.name} layer={layer} details={described?.layers.find((item) => item.name === layer.name)} described={described} />)}
        </div>
      </div>
    </aside>
  )
}
