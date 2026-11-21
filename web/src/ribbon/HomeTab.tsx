import { LayersIcon as Layers3, MaximizeIcon as Maximize, MoveIcon as Move, PencilIcon as Pencil, RulerIcon as Ruler, WrenchIcon as Wrench, ScanSearchIcon as ScanSearch, ShrinkIcon as Shrink, SquareSplitHorizontalIcon as SquareSplitHorizontal, UngroupIcon as Ungroup } from '@kition/tessera/icons/react'
import { CommandButton, RibbonButton, RibbonGroup, RibbonItem, RibbonRow, RibbonStack, ToolButton, toolIcon, useKitEditor, useKitTools } from '@kition/sdk/editor'
import type { CadInfo } from '../cad'

const DRAW = ['line', 'pline', 'circle', 'arc', 'rectang', 'polygon', 'ellipse', 'spline', 'hatch']
const MODIFY = ['move', 'copy', 'rotate', 'scale', 'mirror', 'offset', 'trim', 'extend', 'fillet']
const ANNOTATION = ['mtext', 'text', 'dimlinear', 'dimaligned', 'dimradius']

/** The tools Single line's row shows. */
const PRIMARY = new Set(['line', 'pline', 'circle', 'arc', 'rectang', 'move', 'copy', 'rotate', 'trim', 'offset', 'erase', 'mtext', 'dimlinear'])

/** One of the engine's tools by id, with its icon and its label translated; `primary` shows it in Single line's row. */
function Tool({ id, size = 'small', primary }: { id: string; size?: 'large' | 'small'; primary?: boolean }) {
  const tools = useKitTools()
  const { phrase: p } = useKitEditor()
  const tool = tools?.tools.find((item) => item.id === id)
  if (!tool) return null
  return <ToolButton tool={id} icon={toolIcon(tool.icon, id)} label={p(tool.label)} size={size} title={tool.shortcut ? `${p(tool.label)} (${tool.shortcut})` : p(tool.label)} primary={primary} />
}

export function HomeTab({ info, onFit, onLayers }: { info: CadInfo; onFit: () => void; onLayers: () => void }) {
  const { run, enabled, phrase: p, t } = useKitEditor()
  return (
    <>
      <RibbonGroup label={p('Draw')} icon={Pencil}>
        <Tool id="line" primary={PRIMARY.has('line')} size="large" />
        <Tool id="pline" primary={PRIMARY.has('pline')} size="large" />
        <Tool id="circle" primary={PRIMARY.has('circle')} size="large" />
        <Tool id="arc" primary={PRIMARY.has('arc')} size="large" />
        <RibbonStack>
          {DRAW.slice(4, 7).map((id) => (
            <Tool key={id} id={id} primary={PRIMARY.has(id)} />
          ))}
        </RibbonStack>
        <RibbonStack>
          {DRAW.slice(7).map((id) => (
            <Tool key={id} id={id} primary={PRIMARY.has(id)} />
          ))}
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Modify')} icon={Move}>
        {[0, 3, 6].map((start) => (
          <RibbonStack key={start}>
            {MODIFY.slice(start, start + 3).map((id) => (
              <Tool key={id} id={id} primary={PRIMARY.has(id)} />
            ))}
          </RibbonStack>
        ))}
        <RibbonStack>
          <Tool id="erase" primary={PRIMARY.has('erase')} />
          <CommandButton command="explode" icon={Ungroup} label={p('Explode')} ask />
          <CommandButton command="stretch" icon={SquareSplitHorizontal} label={p('Stretch')} ask />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Annotation')} icon={Ruler}>
        <Tool id="mtext" primary={PRIMARY.has('mtext')} size="large" />
        <RibbonStack>
          {ANNOTATION.slice(1).map((id) => (
            <Tool key={id} id={id} primary={PRIMARY.has(id)} />
          ))}
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Layers')} icon={Layers3}>
        <RibbonButton icon={Layers3} size="large" label={p('Layer Properties')} onClick={onLayers} data-testid="cad-layer-properties" primary />
        <RibbonStack>
          <RibbonRow>
            <RibbonItem primary>
              <select
                className="kui-ribbon-field"
                style={{ width: 150 }}
                value={info.currentLayer}
                aria-label={t('layers')}
                disabled={!enabled('layer.current')}
                onChange={(event) => void run('layer.current', { name: event.target.value })}
                data-testid="cad-layer"
              >
                {info.layers.map((layer) => (
                  <option key={layer.name} value={layer.name}>
                    {layer.name}
                  </option>
                ))}
              </select>
            </RibbonItem>
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Utilities')} icon={Wrench}>
        <RibbonButton icon={Maximize} size="large" label={t('zoomExtents')} onClick={onFit} data-testid="cad-zoom-extents" primary />
        <RibbonStack>
          <CommandButton command="zoom.window" icon={ScanSearch} label={p('Zoom Window')} ask />
          <CommandButton command="overkill" icon={Shrink} label={p('Delete Duplicate Objects')} />
        </RibbonStack>
      </RibbonGroup>
    </>
  )
}
