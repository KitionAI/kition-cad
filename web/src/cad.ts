import type { KitEngineInfo } from '@kition/kit-editor'

export type Layer = { name: string; on: boolean; frozen: boolean; locked: boolean }

export type Drafting = { ortho: boolean; polar: boolean; osnap: boolean; otrack: boolean; grid: boolean; snap: boolean; lwdisplay: boolean }

export type SelectedObject = { handle: string; type: string; layer: string; color: string; locked: boolean; radius?: number; center?: [number, number] }
export type ViewRequest = { serial: number; x: number; y: number; height: number }

/** What the editor reads from the engine's `inspect()`. */
export type CadInfo = {
  entities: number
  layers: Layer[]
  currentLayer: string
  selection: number
  selectedObjects: SelectedObject[]
  viewRequest: ViewRequest | null
  /** The command line's prompt (`Command:`, `Specify first point:`). */
  prompt: string
  running: string | null
  drafting: Drafting
}

export function describe(info: KitEngineInfo): CadInfo {
  const inspected = info.inspected as Partial<CadInfo>
  return {
    entities: inspected.entities ?? 0,
    layers: inspected.layers ?? [],
    currentLayer: inspected.currentLayer ?? '0',
    selection: inspected.selection ?? 0,
    selectedObjects: inspected.selectedObjects ?? [],
    viewRequest: inspected.viewRequest ?? null,
    prompt: inspected.prompt ?? '',
    running: inspected.running ?? null,
    drafting: { ortho: false, polar: false, osnap: false, otrack: false, grid: false, snap: false, lwdisplay: false, ...inspected.drafting },
  }
}

/** File commands the host's own Save, Open, and Close already cover. */
export const HOST_FILE_COMMANDS = ['new', 'open', 'close', 'closeall', 'qsave', 'saveas']

/** The ribbon's tabs after Home, each the engine's menu of that name. */
export const MENU_TABS = ['File', 'Edit', 'Insert', 'Format', 'Tools', 'Draw', 'Dimension', 'Modify', 'View']

/** The engine commands each menu tab shows in Single line's row (AutoCAD's most-used), by menu. */
export const CAD_PRIMARY: Record<string, readonly string[]> = {
  File: ['exportpdf', 'plot', 'pagesetup'],
  Edit: ['cutclip', 'copyclip', 'pasteclip', 'erase.selection', 'selectall', 'find'],
  Insert: ['insert', 'layout.new'],
  Format: ['layer', 'color', 'linetype', 'lweight', 'style', 'dimstyle', 'units'],
  Tools: ['dist', 'area', 'list', 'group', 'ungroup', 'draworder.front', 'draworder.back'],
  Draw: ['line', 'pline', 'circle', 'arc', 'rectang', 'polygon', 'ellipse', 'spline', 'hatch', 'mtext'],
  Dimension: ['dimlinear', 'dimaligned', 'dimangular', 'dimradius', 'dimdiameter', 'dimcontinue', 'dimbaseline', 'mleader', 'qdim'],
  Modify: ['move', 'copy', 'rotate', 'scale', 'mirror', 'offset', 'trim', 'extend', 'fillet', 'erase', 'explode', 'arrayrect'],
  View: ['zoom.extents', 'zoom.window', 'zoom.in', 'zoom.out', 'zoom.previous', 'pan', 'regen'],
}

/** The status bar's drafting modes, in AutoCAD's order, and the commands that set them. */
export const DRAFTING: Array<{ id: keyof Drafting; label: string }> = [
  { id: 'grid', label: 'Grid' },
  { id: 'snap', label: 'Snap' },
  { id: 'ortho', label: 'Ortho' },
  { id: 'polar', label: 'Polar' },
  { id: 'osnap', label: 'Osnap' },
  { id: 'otrack', label: 'Otrack' },
  { id: 'lwdisplay', label: 'Lineweight' },
]
