import { CheckIcon as Check, EyeIcon as Eye, EyeOffIcon as EyeOff, LockIcon as Lock, LockOpenIcon as LockOpen, PlusIcon as Plus, SnowflakeIcon as Snowflake, SunIcon as Sun } from '@kition/tessera/icons/react'
import { IconButton, useKitEditor } from '@kition/sdk/editor'
import type { CadInfo } from './cad'
import { SelectionProperties } from './SelectionProperties'

/** The drawing's layers (Kition CAD's Layer Properties palette) and what is selected. */
export function LayersPanel({ info }: { info: CadInfo }) {
  const { run, ask, enabled, t } = useKitEditor()
  // Layer edits follow their commands, so a read-only drawing shows them disabled.
  const canSet = enabled('layer.set')
  return (
    <aside className="kui-panel cad-side" data-side="right" data-testid="cad-layers">
      <div className="kui-panel-header">
        {t('properties')}
      </div>
      <SelectionProperties info={info} />
      <div className="kui-panel-header">
        {t('layers')}
        <span style={{ flex: 1 }} />
        <IconButton icon={Plus} label={t('newLayer')} disabled={!enabled('layer.new')} onClick={() => ask('layer.new', { current: true })} data-testid="cad-new-layer" />
      </div>
      <div className="kui-panel-body">
        {info.layers.map((layer) => {
          const current = layer.name === info.currentLayer
          return (
            <div key={layer.name} className="cad-layer" data-current={current} data-testid={`cad-layer-${layer.name}`}>
              <button type="button" className="cad-layer-name" title={t('makeCurrent')} disabled={!enabled('layer.current')} onClick={() => void run('layer.current', { name: layer.name })}>
                <span className="cad-layer-current">{current ? <Check size={13} aria-hidden="true" /> : null}</span>
                {layer.name}
              </button>
              <IconButton icon={layer.on ? Eye : EyeOff} label={t('on')} pressed={!layer.on} disabled={!canSet} onClick={() => void run('layer.set', { name: layer.name, on: !layer.on })} data-testid={`cad-layer-${layer.name}-on`} />
              <IconButton icon={layer.frozen ? Snowflake : Sun} label={t('freeze')} pressed={layer.frozen} disabled={!canSet || current} onClick={() => void run('layer.set', { name: layer.name, frozen: !layer.frozen })} />
              <IconButton icon={layer.locked ? Lock : LockOpen} label={t('lock')} pressed={layer.locked} disabled={!canSet} onClick={() => void run('layer.set', { name: layer.name, locked: !layer.locked })} data-testid={`cad-layer-${layer.name}-lock`} />
            </div>
          )
        })}
      </div>
    </aside>
  )
}
