import type { KitEngineInfo } from '@kition/sdk/editor'

export type Layer = { name: string; on: boolean; frozen: boolean; locked: boolean }

export type Drafting = {
  ortho: boolean
  polar: boolean
  osnap: boolean
  otrack: boolean
  grid: boolean
  snap: boolean
  lwdisplay: boolean
  /** The grid's spacing in drawing units (GRIDUNIT, or SNAPUNIT where it is 0) and its major lines (GRIDMAJOR). */
  gridUnit: [number, number]
  gridMajor: number
  /** CURSORSIZE: the crosshair's arms as a percentage of the canvas height. */
  cursorSize: number
  /** DYNMODE: the dynamic input tooltip beside the cursor (F12). */
  dynamicInput: boolean
  /** OSMODE: the running object snap modes' bits (without F3's off bit), as the Osnap menu checks them. */
  osmode?: number
  /** POLARANG: polar tracking's increment angle in degrees, as the Polar menu checks it. */
  polarAngle?: number
  /** The command line's prompt taken apart and its history (the engine's `/drafting/commandLine`). */
  commandLine: CommandLineState
  /** The last double-click's request to edit an object in place (`in_place.rs`), or null. */
  edit: EditRequest | null
}

/**
 * Where typed text goes in the drawing (drawing units): `at` is a line's left end on its baseline,
 * a paragraph's top-left corner, or a dimension text's middle when `centered`; the rotation in
 * degrees; a paragraph's width (0: none).
 */
export type TextBox = { at: [number, number]; height: number; rotation: number; width: number; paragraphs: boolean; centered?: boolean }

/** What a double-click (or Edit Text) edits in place: text where it is, or a hatch in its dialog. */
export type EditRequest =
  | { serial?: number; kind: 'text' | 'paragraphs' | 'dimension'; handle: string; text: string; box: TextBox }
  | { serial?: number; kind: 'hatch'; handle: string; pattern: string; scale: number; angle: number }

/** What a prompt asks for; `command` between commands. */
export type PromptKind = 'command' | 'point' | 'distance' | 'angle' | 'select' | 'text' | 'keyword'

/**
 * The prompt as the command line draws it: the command (as typed, in capitals), the message, the
 * options (each typed as written to choose it), the default (Enter takes it) and what it asks for.
 */
export type PromptParts = {
  command: string | null
  message: string
  keywords: string[]
  default: string | null
  kind: PromptKind
  /** The base point the rubber band and the dynamic input boxes measure from (drawing units). */
  base?: [number, number] | null
  /** A hot grip's edit: its mode (`stretch`, `move`, `rotate`, `scale`, `mirror`) and whether it places copies. */
  grip?: { mode: string; copy: boolean } | null
  /** Where TEXT's and MTEXT's text goes, at their "Enter text" prompts (typed there, in place). */
  textBox?: TextBox | null
}

/** The command line's history: its last lines, and how many lines it has ever had. */
export type CommandHistory = { serial: number; lines: string[] }

export type CommandLineState = { prompt: PromptParts; history: CommandHistory }

export const IDLE_PROMPT: PromptParts = { command: null, message: 'Command', keywords: [], default: null, kind: 'command' }

/** The status bar's on/off drafting modes. */
export type DraftingMode = 'ortho' | 'polar' | 'osnap' | 'otrack' | 'grid' | 'snap' | 'lwdisplay' | 'dynamicInput'

/** The canvas's drawing units per document pixel, and the document pixel of the drawing's origin. */
export type Units = { perPixel: number; origin: [number, number] }

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
  units: Units | null
  /** The drawing's spaces in tab order: Model, then its layouts (the status bar's layout tabs). */
  layouts: string[]
  /** The space the editor shows: `Model` or a layout's name. */
  space: string
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
    drafting: {
      ortho: false, polar: false, osnap: false, otrack: false, grid: false, snap: false, lwdisplay: false, dynamicInput: false, gridUnit: [10, 10], gridMajor: 5, cursorSize: 5,
      ...inspected.drafting,
      edit: inspected.drafting?.edit ?? null,
      commandLine: { prompt: inspected.drafting?.commandLine?.prompt ?? IDLE_PROMPT, history: inspected.drafting?.commandLine?.history ?? { serial: 0, lines: [] } },
    },
    units: inspected.units?.origin ? inspected.units : null,
    layouts: ((info.inspected as { layouts?: Array<{ name?: string }> }).layouts ?? []).map((layout) => layout.name ?? '').filter(Boolean),
    space: typeof (info.inspected as { space?: unknown }).space === 'string' ? (info.inspected as { space: string }).space : 'Model',
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
export const DRAFTING: Array<{ id: DraftingMode; command: string; label: string }> = [
  { id: 'grid', command: 'drafting.grid', label: 'Grid' },
  { id: 'snap', command: 'drafting.snap', label: 'Snap' },
  { id: 'ortho', command: 'drafting.ortho', label: 'Ortho' },
  { id: 'polar', command: 'drafting.polar', label: 'Polar' },
  { id: 'osnap', command: 'drafting.objectSnap', label: 'Osnap' },
  { id: 'otrack', command: 'drafting.objectSnapTracking', label: 'Otrack' },
  { id: 'lwdisplay', command: 'drafting.lineweightDisplay', label: 'Lineweight' },
  { id: 'dynamicInput', command: 'drafting.dynamicInput', label: 'DynInput' },
]

/** Whether a hot grip is being edited (its prompt is on the command line; no command runs). */
export function gripping(info: CadInfo | null): boolean {
  return info?.drafting.commandLine.prompt.grip != null
}
