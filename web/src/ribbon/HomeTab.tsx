import { LayersIcon as Layers3, MaximizeIcon as Maximize, MoveIcon as Move, PencilIcon as Pencil, RulerIcon as Ruler, WrenchIcon as Wrench, ScanSearchIcon as ScanSearch } from '@kition/tessera/icons/react'
import { CommandButton, RibbonButton, RibbonItem, RibbonRow, RibbonStack, ToolButton, toolIcon, useKitEditor, useKitTools, type IconComponent } from '@kition/sdk/editor'
import type { CadInfo } from '../cad'
import { startsPrompt, useCatalog } from '../commands'
import { PromptButton } from './CommandTab'
import { CAD_ICONS } from '../commandIcons'
import { FitGroup } from './FitGroup'

/** The Draw group's four large tools. */
const DRAW_TOOLS = ['line.add', 'polyline.add', 'circle.add', 'arc.add']
/** The Draw group's small buttons after its four large ones, in stacks of three: tools and commands. */
const DRAW = ['rectangle.add', 'polygon.add', 'ellipse.add', 'spline.add', 'hatch.add', 'point.add']
/** The Modify group, in stacks of three: the common edits, so none needs the Modify tab. */
const MODIFY = [
  'object.move', 'object.copy', 'object.rotate',
  'object.scale', 'object.mirror', 'object.offset',
  'object.trim', 'object.extend', 'object.fillet',
  'object.chamfer', 'object.break', 'object.join',
  'object.delete', 'object.explode', 'object.stretch',
]
/** The Modify commands a compact Modify group still shows; the rest wait under More. */
const MODIFY_SHOWN = ['object.move', 'object.copy', 'object.rotate', 'object.trim', 'object.offset']
const ANNOTATION = ['multilineText.add', 'text.add', 'dimension.addLinear', 'dimension.addAligned', 'dimension.addRadius']

/** The tools Single line's row shows. */
const PRIMARY = new Set(['line.add', 'polyline.add', 'circle.add', 'arc.add', 'rectangle.add', 'object.move', 'object.copy', 'object.rotate', 'object.trim', 'object.offset', 'object.delete', 'multilineText.add', 'dimension.addLinear'])

/** One of the engine's tools by id, with its icon and its label translated; `primary` shows it in Single line's row. */
function Tool({ id, size = 'small', primary }: { id: string; size?: 'large' | 'small'; primary?: boolean }) {
  const tools = useKitTools()
  const { phrase: p } = useKitEditor()
  const tool = tools?.tools.find((item) => item.id === id)
  if (!tool) return null
  return <ToolButton tool={id} icon={CAD_ICONS[id] ?? toolIcon(tool.icon, id)} label={p(tool.label)} size={size} title={tool.shortcut ? `${p(tool.label)} (${tool.shortcut})` : p(tool.label)} primary={primary} />
}

/**
 * One of the engine's commands on Home: the engine's tool where it has one, else a button that
 * starts the command's prompts (or, for a command without prompts, runs it as the shared button
 * does, asking its parameters).
 */
function HomeCommand({ id, icon, primary }: { id: string; icon?: IconComponent; primary?: boolean }) {
  const tools = useKitTools()
  const catalog = useCatalog()
  const { phrase: p, byId } = useKitEditor()
  // Nothing until the engine's tools are known: a tool drawn first as a command button would be
  // another button (and widen Single line's row for a moment).
  if (!tools?.tools.length) return null
  if (tools.tools.some((tool) => tool.id === id)) return <Tool id={id} primary={primary} />
  const label = p(byId.get(id)?.title ?? id)
  if (startsPrompt(catalog.byId.get(id))) return <PromptButton command={id} icon={icon} label={label} primary={primary} />
  return <CommandButton command={id} icon={icon} label={label} primary={primary} ask />
}

export function HomeTab({ info, onFit, onLayers }: { info: CadInfo; onFit: () => void; onLayers: () => void }) {
  const { run, enabled, phrase: p, t } = useKitEditor()
  const layerProperties = (size: 'large' | 'small') => <RibbonButton key="cad-layer-properties" icon={Layers3} size={size} label={p('Layer Properties')} onClick={onLayers} data-testid="cad-layer-properties" primary />
  const layerField = (
    <RibbonRow key="cad-layer">
      <RibbonItem primary>
        <select
          className="kui-ribbon-field"
          style={{ width: 132 }}
          value={info.currentLayer}
          aria-label={t('layers')}
          disabled={!enabled('layer.setCurrent')}
          onChange={(event) => void run('layer.setCurrent', { name: event.target.value })}
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
  )
  const zoomExtents = (size: 'large' | 'small') => <RibbonButton key="cad-zoom-extents" icon={Maximize} size={size} label={t('zoomExtents')} onClick={onFit} data-testid="cad-zoom-extents" primary />
  return (
    <>
      {/* Where the ribbon runs out of room each group keeps its labels: its first commands, then More (FitGroup). */}
      <FitGroup
        label={p('Draw')}
        icon={Pencil}
        name="draw"
        shown={[...DRAW_TOOLS.map((id) => <Tool key={id} id={id} primary={PRIMARY.has(id)} />), <HomeCommand key={DRAW[0]} id={DRAW[0]} primary={PRIMARY.has(DRAW[0])} />]}
        more={DRAW.slice(1).map((id) => <HomeCommand key={id} id={id} primary={PRIMARY.has(id)} />)}
      >
        {DRAW_TOOLS.slice(0, 4).map((id) => <Tool key={id} id={id} primary={PRIMARY.has(id)} size="large" />)}
        {[0, 3].map((start) => (
          <RibbonStack key={start}>
            {DRAW.slice(start, start + 3).map((id) => <HomeCommand key={id} id={id} primary={PRIMARY.has(id)} />)}
          </RibbonStack>
        ))}
      </FitGroup>

      <FitGroup
        label={p('Modify')}
        icon={Move}
        name="modify"
        shown={MODIFY_SHOWN.map((id) => <HomeCommand key={id} id={id} primary={PRIMARY.has(id)} />)}
        more={MODIFY.filter((id) => !MODIFY_SHOWN.includes(id)).map((id) => <HomeCommand key={id} id={id} primary={PRIMARY.has(id)} />)}
      >
        {[0, 3, 6, 9, 12].map((start) => (
          <RibbonStack key={start}>
            {MODIFY.slice(start, start + 3).map((id) => <HomeCommand key={id} id={id} primary={PRIMARY.has(id)} />)}
          </RibbonStack>
        ))}
      </FitGroup>

      <FitGroup
        label={p('Annotation')}
        icon={Ruler}
        name="annotation"
        shown={['multilineText.add', 'dimension.addLinear'].map((id) => <Tool key={id} id={id} primary={PRIMARY.has(id)} />)}
        more={['text.add', 'dimension.addAligned', 'dimension.addRadius'].map((id) => <Tool key={id} id={id} primary={PRIMARY.has(id)} />)}
      >
        <Tool id="multilineText.add" primary={PRIMARY.has('multilineText.add')} size="large" />
        <RibbonStack>
          {ANNOTATION.slice(1).map((id) => (
            <Tool key={id} id={id} primary={PRIMARY.has(id)} />
          ))}
        </RibbonStack>
      </FitGroup>

      {/* Compact: the layer field over Layer Properties, both labelled. */}
      <FitGroup label={p('Layers')} icon={Layers3} name="layers" shown={[layerField, layerProperties('small')]}>
        {layerProperties('large')}
        <RibbonStack>{layerField}</RibbonStack>
      </FitGroup>

      <FitGroup label={p('Utilities')} icon={Wrench} name="utilities" shown={[zoomExtents('small'), <HomeCommand key="view.zoomWindow" id="view.zoomWindow" icon={ScanSearch} />]}>
        {zoomExtents('large')}
        <RibbonStack>
          <HomeCommand id="view.zoomWindow" icon={ScanSearch} />
        </RibbonStack>
      </FitGroup>
    </>
  )
}
