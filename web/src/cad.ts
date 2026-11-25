import type { KitEngineInfo } from '@kition/sdk/editor'

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
export const HOST_FILE_COMMANDS = ['file.new', 'file.open', 'file.close', 'file.closeAll', 'file.save', 'file.saveAs']

/** The ribbon's tabs after Home, each the engine's menu of that name. */
export const MENU_TABS = ['File', 'Edit', 'Insert', 'Format', 'Tools', 'Draw', 'Dimension', 'Modify', 'View']

/** The engine commands each menu tab shows in Single line's row (AutoCAD's most-used), by menu. */
export const CAD_PRIMARY: Record<string, readonly string[]> = {
  File: ['file.exportPdf', 'file.print', 'plot.pageSetup'],
  Edit: ['edit.cut', 'edit.copy', 'edit.paste', 'edit.delete', 'edit.selectAll', 'edit.find'],
  Insert: ['block.insert', 'layout.add'],
  Format: ['layer.open', 'property.color', 'property.linetype', 'property.lineweight', 'textStyle.open', 'dimensionStyle.open', 'file.setUnits'],
  Tools: ['measure.distance', 'measure.area', 'object.list', 'arrange.group', 'arrange.ungroup', 'arrange.bringToFront', 'arrange.sendToBack'],
  Draw: ['line.add', 'polyline.add', 'circle.add', 'arc.add', 'rectangle.add', 'polygon.add', 'ellipse.add', 'spline.add', 'hatch.add', 'multilineText.add'],
  Dimension: ['dimension.addLinear', 'dimension.addAligned', 'dimension.addAngular', 'dimension.addRadius', 'dimension.addDiameter', 'dimension.addContinue', 'dimension.addBaseline', 'multileader.add', 'dimension.addQuick'],
  Modify: ['object.move', 'object.copy', 'object.rotate', 'object.scale', 'object.mirror', 'object.offset', 'object.trim', 'object.extend', 'object.fillet', 'object.delete', 'object.explode', 'object.arrayRectangular'],
  View: ['view.zoomToFit', 'view.zoomWindow', 'view.zoomIn', 'view.zoomOut', 'view.zoomPrevious', 'view.pan', 'view.regen'],
}

/** The status bar's drafting modes, in AutoCAD's order: the key of each in `Drafting` and the command that sets it. */
export const DRAFTING: Array<{ id: keyof Drafting; command: string; label: string }> = [
  { id: 'grid', command: 'drafting.grid', label: 'Grid' },
  { id: 'snap', command: 'drafting.snap', label: 'Snap' },
  { id: 'ortho', command: 'drafting.ortho', label: 'Ortho' },
  { id: 'polar', command: 'drafting.polar', label: 'Polar' },
  { id: 'osnap', command: 'drafting.objectSnap', label: 'Osnap' },
  { id: 'otrack', command: 'drafting.objectSnapTracking', label: 'Otrack' },
  { id: 'lwdisplay', command: 'drafting.lineweightDisplay', label: 'Lineweight' },
]
