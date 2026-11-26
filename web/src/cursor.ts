import { canvasColor, type OverlayDrawer, type RasterDecoration } from '@kition/sdk/editor'
import type { CadInfo } from './cad'
import { DynamicInput } from './dynamicInput'

/**
 * The cursor and the canvas chrome CAD draws itself, never into the drawing's pixels: the
 * crosshair and pick box, the coordinate readout, the grid over the whole canvas and the UCS icon.
 *
 * The engine tells where the cursor is through two fully transparent overlays (the binding's
 * `cursor_overlays`): an anchor where the crosshair goes, as large as the pick box, and a label
 * with the coordinate readout. Its CSS cursor says what the cursor is (`cursor_state`).
 */

/** What the drawn cursor shows: the crosshair with the pick box, the crosshair alone, the box alone. */
export type CursorState = 'idle' | 'point' | 'select'

/**
 * The engine tool's CSS cursor as the cursor CAD draws; `null` where the system's shows instead
 * (`text` at a free-text prompt, a host tool's hand).
 */
export function cursorState(css: string | undefined): CursorState | null {
  return css === 'default' ? 'idle' : css === 'crosshair' ? 'point' : css === 'pointer' ? 'select' : null
}

type Rgb = [number, number, number]

const parse = (hex: string | null): Rgb | null => (hex && /^#[0-9a-f]{6}$/i.test(hex) ? [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16)) as Rgb : null)
const luminance = ([r, g, b]: Rgb) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255
const mix = (a: Rgb, b: Rgb, t: number): string => `rgb(${a.map((v, i) => Math.round(v + (b[i] - v) * t)).join(',')})`
/** A crisp one-pixel line at canvas coordinate `v`. */
const crisp = (v: number) => Math.round(v) + 0.5

/** The canvas colour the drawing is on (the host theme's `--kit-canvas`) and the ink on it. */
class Palette {
  private key = ''
  private value: { background: Rgb; ink: Rgb } = { background: [246, 245, 244], ink: [26, 26, 26] }

  get(): { background: Rgb; ink: Rgb } {
    const style = getComputedStyle(document.documentElement)
    const key = `${style.getPropertyValue('--kit-canvas')}|${style.getPropertyValue('--kit-text')}`
    if (key !== this.key) {
      this.key = key
      const background = parse(canvasColor()) ?? this.value.background
      // The theme's text colour, or black or white, whichever stands out on the canvas.
      const text = parse(canvasColor(document.documentElement, '--kit-text'))
      const contrast: Rgb = luminance(background) > 0.5 ? [32, 32, 32] : [230, 230, 230]
      const ink = text && Math.abs(luminance(text) - luminance(background)) > 0.4 ? text : contrast
      this.value = { background, ink }
    }
    return this.value
  }
}

/** The fully transparent red label the engine puts at the grip under the pointer (`GRIP_INFO`). */
const GRIP_TAG = [255, 0, 0, 0]

/**
 * An option of a grip's own (a line's end: Lengthen): its menu label, the prompt's keyword, and
 * whether answering it changes the drawing at once (Remove vertex).
 */
export type GripOption = { label: string; keyword: string; immediate: boolean }

/** The grip under the pointer, for its menu: its object and number, its options, where it is on the page. */
export type GripUnder = { handle: string; index: number; actions: GripOption[]; x: number; y: number }

/** The cursor's state shared by the overlay hook (drawn every frame) and the canvas element. */
export class CadCursor {
  /** Whether the pointer is over the canvas: off it (on a menu, a panel) no crosshair is drawn. */
  inside = false
  state: CursorState | null = null
  /** CURSORSIZE: the crosshair's arms as a percentage of the canvas height. */
  size = 5
  /** The status bar's coordinate readout, written as the canvas draws (no React render per move). */
  readout: HTMLElement | null = null
  readonly palette = new Palette()
  /** The dynamic input tooltip, placed where the crosshair is (F12). */
  readonly dynamic = new DynamicInput()
  /** The grip under the pointer between edits (a right-click there opens its menu), or null. */
  grip: GripUnder | null = null

  /**
   * The engine overlays only CAD draws (all transparent): the crosshair's anchor, the readout's
   * label (black), the dynamic input's live values (a white label) and the grip under the pointer
   * (a red label), the last two read where the anchor is.
   */
  readonly drawOverlay: OverlayDrawer = (overlay, { context, toScreen, overlays }) => {
    if (!('color' in overlay) || overlay.color[3] !== 0) return false
    if (overlay.kind === 'label') {
      if (overlay.color[0] !== 0) return true
      if (this.readout && this.readout.textContent !== overlay.text) this.readout.textContent = overlay.text
      return true
    }
    if (overlay.kind !== 'anchor') return false
    const at = toScreen(overlay.x, overlay.y)
    // The grip under the pointer comes in the same frame as the crosshair's anchor.
    const grip = overlays.find((item) => item.kind === 'label' && item.color.every((value, index) => value === GRIP_TAG[index]))
    this.grip = null
    if (grip?.kind === 'label') {
      try {
        const bounds = context.canvas.getBoundingClientRect()
        const place = toScreen(grip.x, grip.y)
        this.grip = { ...(JSON.parse(grip.text) as Omit<GripUnder, 'x' | 'y'>), x: bounds.left + place.x, y: bounds.top + place.y }
      } catch {
        this.grip = null
      }
    }
    this.dynamic.inside = this.inside
    this.dynamic.place(at.x, at.y, context.canvas, overlays)
    const state = this.state
    if (!this.inside || !state) return true
    const x = crisp(at.x)
    const y = crisp(at.y)
    const length = Math.max(12, (context.canvas.clientHeight * this.size) / 100)
    const half = state === 'point' ? 0 : Math.max(1, Math.round(overlay.size / 2))
    const [r, g, b] = this.palette.get().ink
    context.strokeStyle = `rgb(${r},${g},${b})`
    context.lineWidth = 1
    context.setLineDash([])
    context.beginPath()
    if (state !== 'select') {
      context.moveTo(x - length, y)
      context.lineTo(x - half, y)
      context.moveTo(x + half, y)
      context.lineTo(x + length, y)
      context.moveTo(x, y - length)
      context.lineTo(x, y - half)
      context.moveTo(x, y + half)
      context.lineTo(x, y + length)
    }
    if (half) context.rect(x - half, y - half, 2 * half, 2 * half)
    context.stroke()
    return true
  }
}

/** Screen pixels between grid lines: the unit times 5 until they are 8 pixels apart, as the engine's. */
function gridStep(pixels: number): number | null {
  let step = pixels
  if (!Number.isFinite(step) || step <= 0) return null
  for (let i = 0; i < 40 && step < 8; i += 1) step *= 5
  return Number.isFinite(step) && step >= 8 ? step : null
}

/**
 * The grid (F7) over the whole canvas, inside the drawing's area and around it: GRIDUNIT apart,
 * times 5 while lines would be closer than 8 pixels, every GRIDMAJOR line stronger. It is drawn over
 * the drawing but blends as if under it (only darkening a light canvas, only lightening a dark one),
 * so a line of the drawing keeps its colour where the grid crosses it.
 */
export function gridDecoration(info: CadInfo | null, palette: CadCursor['palette']): RasterDecoration | null {
  const units = info?.units
  if (!info?.drafting.grid || !units || !(units.perPixel > 0)) return null
  const [unitX, unitY] = info.drafting.gridUnit
  const major = Math.max(1, Math.round(info.drafting.gridMajor))
  return (context, toScreen, view) => {
    const width = context.canvas.clientWidth
    const height = context.canvas.clientHeight
    const scale = view.zoom / units.perPixel
    const stepX = gridStep(Math.abs(unitX) * scale)
    const stepY = gridStep(Math.abs(unitY) * scale)
    if (!stepX || !stepY) return
    const origin = toScreen(units.origin[0], units.origin[1])
    const { background, ink } = palette.get()
    const light = luminance(background) > 0.5
    context.globalCompositeOperation = light ? 'darken' : 'lighten'
    context.lineWidth = 1
    const lines = (strong: boolean) => {
      context.beginPath()
      for (let i = Math.ceil(-origin.x / stepX); i <= Math.floor((width - origin.x) / stepX); i += 1) {
        if ((i % major === 0) !== strong) continue
        const x = crisp(origin.x + i * stepX)
        context.moveTo(x, 0)
        context.lineTo(x, height)
      }
      for (let j = Math.ceil(-origin.y / stepY); j <= Math.floor((height - origin.y) / stepY); j += 1) {
        if ((j % major === 0) !== strong) continue
        const y = crisp(origin.y + j * stepY)
        context.moveTo(0, y)
        context.lineTo(width, y)
      }
      context.strokeStyle = mix(background, ink, light ? (strong ? 0.12 : 0.05) : strong ? 0.16 : 0.07)
      context.stroke()
    }
    lines(false)
    lines(true)
  }
}

/** The UCS icon in the canvas's lower-left corner: X to the right, Y up, as the drawing's axes run. */
export function ucsDecoration(palette: CadCursor['palette']): RasterDecoration {
  return (context) => {
    const x = 18.5
    const y = Math.round(context.canvas.clientHeight - 18) + 0.5
    const arm = 34
    const [r, g, b] = palette.get().ink
    context.strokeStyle = `rgba(${r},${g},${b},0.85)`
    context.fillStyle = context.strokeStyle
    context.lineWidth = 1.5
    context.setLineDash([])
    context.beginPath()
    context.moveTo(x + arm, y)
    context.lineTo(x, y)
    context.lineTo(x, y - arm)
    // Arrow heads.
    context.moveTo(x + arm - 5, y - 3)
    context.lineTo(x + arm, y)
    context.lineTo(x + arm - 5, y + 3)
    context.moveTo(x - 3, y - arm + 5)
    context.lineTo(x, y - arm)
    context.lineTo(x + 3, y - arm + 5)
    context.stroke()
    context.font = '600 11px ui-sans-serif, system-ui, sans-serif'
    context.textBaseline = 'middle'
    context.fillText('X', x + arm + 4, y)
    context.textAlign = 'center'
    context.fillText('Y', x, y - arm - 8)
  }
}
