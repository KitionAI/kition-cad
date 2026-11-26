import type { EngineOverlay } from '@kition/sdk/editor'
import type { PromptParts } from './cad'

/**
 * Dynamic input (F12): the prompt and value boxes CAD draws beside the cursor while a command
 * waits (`DynamicTooltip.tsx`).
 *
 * The command line's text is the one source of truth and the boxes show it. What is typed on the
 * canvas goes to the command line (`App.tsx`) and belongs to the boxes, and so does what is typed
 * next while the command line keeps the keyboard: `40` is typed in the first box, `,` or `<` (or Tab) ends it and locks it, Tab after the second value locks that too and goes
 * back to the first box. Enter sends what the boxes show (the engine fills an empty box from the
 * cursor and measures from the last point: `@40<30`). Typed in the command line after a click in
 * it, or pasted there, the text goes as typed (absolute), as a CAD command window does.
 *
 * The engine holds the cursor to a locked value (`drafting.setDynamicEntry`) and sends the live
 * values the empty boxes show as a transparent label with the cursor (`dynamic_input.rs`).
 */

/** How the boxes read a point: X and Y, or distance and angle. */
export type Format = 'cartesian' | 'polar'

/** The command line's text read as the boxes show it. */
export type BoxEntry = { format: Format; first: string; split: boolean; second: string }

/** The live values the empty boxes show, in the drawing's units, as the engine sends them. */
export type Live = { cartesian: [string, string]; polar: [string, string]; relative: boolean }

/** What the tooltip shows for the prompt: two boxes, one box, or the prompt and what is typed. */
export type Layout = 'pair' | 'single' | 'message'

/** The live values' label: fully transparent, told apart from the coordinate readout by its colour. */
const DYNAMIC_TAG = [255, 255, 255, 0]

/** Where the tooltip sits from the cursor: right of it, below the tracking readout. */
const OFFSET_X = 18
const OFFSET_Y = 34
/** Room kept at the canvas's bottom for the command line's floating prompt and history. */
const BOTTOM_CLEAR = 80

/** A distance as typed: a number, feet and inches, a fraction. */
const DISTANCE = /^[-+]?(?=[\d.])[\d.]*(?:[eE][-+]?\d+)?(?:['"]?(?:[-\s]?[\d./]+"?)?)?$/
/** An angle as typed: a number with its units (`30d15'`, `0.5r`, `100g`), or a bearing (`N45dE`). */
const ANGLE = /^(?:[-+]?[\d.]+[dDrRgG]?(?:[\d.]+')?(?:[\d.]+")?|[nNsS][\d.dD'"]*[eEwW])$/

/** Reads `text` as a box entry, or `null` when it is something else (`@`, `#`, an option, a name). */
export function parseEntry(text: string, relative: boolean): BoxEntry | null {
  const t = text.trim()
  if (t.startsWith('@') || t.startsWith('#')) return null
  const comma = t.indexOf(',')
  const lt = t.indexOf('<')
  let entry: BoxEntry
  if (comma >= 0 && (lt < 0 || comma < lt)) entry = { format: 'cartesian', first: t.slice(0, comma).trim(), split: true, second: t.slice(comma + 1).trim() }
  else if (lt >= 0) entry = { format: 'polar', first: t.slice(0, lt).trim(), split: true, second: t.slice(lt + 1).trim() }
  else entry = { format: relative ? 'polar' : 'cartesian', first: t, split: false, second: '' }
  if (entry.first && !DISTANCE.test(entry.first)) return null
  if (/[,<]/.test(entry.second)) return null
  if (entry.second && !(entry.format === 'cartesian' ? DISTANCE : ANGLE).test(entry.second)) return null
  return entry
}

/** The separator of a format. */
const separator = (format: Format) => (format === 'polar' ? '<' : ',')

/** Typing `typed` in the first box of the split entry `text`: before the separator; a separator ends it again. */
function typeInFirst(text: string, typed: string): { text: string; done: boolean } | null {
  const at = text.search(/[,<]/)
  if (at < 0) return null
  let sep = text[at]
  let first = text.slice(0, at)
  let second = text.slice(at + 1)
  let done = false
  for (const c of typed) {
    if ((c === ',' || c === '<') && !done) { sep = c; done = true } else if (done) second += c
    else first += c
  }
  return { text: `${first}${sep}${second}`, done }
}

/** Backspace in the first box of the split entry `text`. */
function backspaceInFirst(text: string): string | null {
  const at = text.search(/[,<]/)
  if (at < 0) return null
  return `${text.slice(0, at).trimEnd().slice(0, -1)}${text.slice(at)}`
}

type Rect = { left: number; top: number; width: number; height: number }

/** The dynamic input's state, shared by the command line, the canvas and the tooltip. */
export class DynamicInput {
  /** DYNMODE, and the prompt the boxes work for (set as the editor renders). */
  on = false
  running = false
  prompt: PromptParts | null = null
  /** The command line's text, and whether it belongs to the boxes (typed on the canvas, or Tab). */
  text = ''
  dynamic = false
  /** The next change of the command line's text comes from typing on the canvas. */
  fromCanvas = false
  /**
   * The command line has the keyboard because typing on the canvas moved it there (not a click in
   * it): what is typed next still belongs to the boxes. `focusFromCanvas` marks the focus it gets.
   */
  canvasTyping = false
  focusFromCanvas = false
  /** Tab moved back to the first box (the text it did so at), and Tab locked the second value. */
  firstActive: string | null = null
  secondLocked: string | null = null
  /** The prompt's options listed under the tooltip (Down Arrow), and the highlighted one. */
  menu: { keywords: string[]; index: number } | null = null
  /** Whether the pointer is over the canvas (the tooltip shows only then). */
  inside = false
  /** The tooltip's element; it is placed and its live values written as the canvas draws. */
  box: HTMLElement | null = null
  live: Live | null = null
  /** Where the tooltip was put last (page pixels), kept while the options list is open. */
  private at: { x: number; y: number; canvas: Rect } | null = null
  /** The engine's calls, given by the editor: tell it the entry, and redraw the tool. */
  execute: ((command: string, params: Record<string, unknown>) => Promise<string>) | null = null
  refresh: (() => void) | null = null
  /** Answers the prompt with an option, as the command line does. */
  pick: ((keyword: string) => void) | null = null
  private sent = ''
  private version = 0
  private listeners = new Set<() => void>()

  readonly subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
  }
  readonly snapshot = () => this.version
  private notify() {
    this.version += 1
    for (const listener of this.listeners) listener()
  }

  /** Whether a command waits (not a hot grip) with dynamic input on. */
  active(): boolean {
    return this.on && this.running && this.prompt != null && this.prompt.kind !== 'command'
  }
  /** What the tooltip shows for the prompt, or `null` when it does not show. */
  layout(): Layout | null {
    if (!this.active() || !this.prompt) return null
    if (this.prompt.kind === 'point') return 'pair'
    if (this.prompt.kind === 'distance' || this.prompt.kind === 'angle') return 'single'
    return 'message'
  }
  /** Whether the tooltip shows: a layout for the prompt, and the text, if any, typed for the boxes. */
  shows(): boolean {
    return this.layout() != null && (this.dynamic || !this.text)
  }
  /** Measured from a base point (the last point): distance and angle, relative. */
  relative(): boolean {
    return this.prompt?.base != null
  }
  /** The text read as box entry, when the boxes show it. */
  entry(): BoxEntry | null {
    return this.layout() === 'pair' && this.dynamic ? parseEntry(this.text, this.relative()) : null
  }
  firstActiveNow(): boolean { return this.firstActive != null && this.firstActive === this.text }
  secondLockedNow(): boolean { return this.secondLocked != null && this.secondLocked === this.text }

  /**
   * The command line's text changed: typed (on the canvas, or in the command line), recalled,
   * cleared, or by a key of the boxes (`boxes`, which keeps the text theirs; `line`: recalled
   * from the command line's history, the command line's own).
   */
  setText(text: string, origin: 'typed' | 'boxes' | 'line' = 'typed') {
    if (origin === 'boxes') this.dynamic = true
    else if (origin === 'line' || !text) this.dynamic = false
    else if (!this.text) {
      // A new entry belongs to the boxes when it is typed on the canvas, or in the command line
      // the canvas's typing moved the keyboard to; not when it is pasted in whole.
      const pasted = text.length > 1 && !this.fromCanvas
      this.dynamic = !pasted && (this.fromCanvas || this.canvasTyping) && this.layout() != null
    }
    this.fromCanvas = false
    this.text = text
    if (this.menu) this.menu = null
    this.sync()
    this.notify()
  }
  /** The command line got the keyboard: by typing on the canvas, or otherwise (a click in it). */
  focused() {
    this.canvasTyping = this.focusFromCanvas
    this.focusFromCanvas = false
  }
  /** The entry is sent, used up by a click, or thrown away: the boxes are empty again. */
  reset() {
    this.text = ''
    this.dynamic = false
    this.fromCanvas = false
    this.firstActive = null
    this.secondLocked = null
    this.menu = null
    // The engine forgets its copy itself when the entry is sent, a click uses it or Escape.
    this.sent = ''
    this.notify()
  }

  /** Tells the engine what the boxes hold, when it changed: a locked value holds the cursor there. */
  private sync() {
    const entry = this.entry()
    const key = entry ? JSON.stringify([this.text, this.secondLockedNow()]) : ''
    if (key === this.sent || !this.execute) return
    this.sent = key
    void this.execute('drafting.setDynamicEntry', { text: entry ? this.text : '', secondLocked: entry ? this.secondLockedNow() : false })
      .catch(() => {})
      .finally(() => this.refresh?.())
  }

  /** Whether Enter should send what the boxes show rather than the text as typed. */
  completes(text: string): boolean {
    return this.dynamic && this.layout() === 'pair' && text === this.text && parseEntry(text, this.relative())?.split === true
  }
  /** The text Enter sends for `text`: the engine fills the empty box from the cursor (`@40<<30`). */
  async finish(text: string): Promise<string> {
    if (!this.execute) return text
    try {
      const answer = JSON.parse(await this.execute('drafting.setDynamicEntry', { text, secondLocked: this.secondLockedNow() })) as { complete?: string | null }
      return typeof answer.complete === 'string' ? answer.complete : text
    } catch {
      return text
    }
  }

  /**
   * A key at the command line or on the canvas, before anything else sees it: the options list's
   * keys while it is open, Down Arrow to open it, Tab between the boxes, typing and Backspace in
   * the first box after Tab went back to it. Returns the command line's new text when the key
   * changed it, `true` when it took the key otherwise, `false` when the key is not the boxes'.
   */
  key(key: string, browsingHistory: boolean): string | boolean {
    if (key === 'Shift' || key === 'Control' || key === 'Alt' || key === 'Meta') return false
    if (this.menu) {
      const menu = this.menu
      if (key === 'ArrowDown' || key === 'ArrowUp') {
        const n = menu.keywords.length
        this.menu = { ...menu, index: (menu.index + (key === 'ArrowDown' ? 1 : n - 1)) % n }
        this.notify()
        return true
      }
      if (key === 'Enter' || key === ' ') {
        const keyword = menu.keywords[menu.index]
        this.menu = null
        this.notify()
        if (keyword) this.pick?.(keyword)
        return true
      }
      if (key === 'Escape') {
        this.menu = null
        this.notify()
        return true
      }
      // Any other key closes the list and does what it does.
      this.menu = null
      this.notify()
    }
    const layout = this.layout()
    if (!layout) return false
    if (key === 'ArrowDown') {
      const keywords = this.prompt?.keywords ?? []
      if (browsingHistory || !keywords.length || !this.inside) return false
      this.menu = { keywords, index: 0 }
      this.notify()
      return true
    }
    if (layout !== 'pair') return false
    const relative = this.relative()
    if (key === 'Tab') {
      const entry = parseEntry(this.text, relative)
      if (!entry) return false
      this.dynamic = true
      if (!entry.split) return this.text.trim() + separator(entry.format)
      if (this.firstActiveNow()) {
        this.firstActive = null
      } else {
        // Leaving the second box: a typed value locks too, and Tab goes round to the first box.
        if (entry.first && entry.second) this.secondLocked = this.text
        this.firstActive = this.text
      }
      this.sync()
      this.notify()
      return true
    }
    if (!this.dynamic || !this.firstActiveNow()) return false
    const locked = this.secondLockedNow()
    if (key === 'Backspace') {
      const text = backspaceInFirst(this.text)
      if (text == null) return false
      this.firstActive = text
      this.secondLocked = locked ? text : null
      return text
    }
    if (key.length !== 1 || key === ' ') return false
    const edit = typeInFirst(this.text, key)
    if (!edit) return false
    if (!parseEntry(edit.text, relative)) return true
    this.firstActive = edit.done ? null : edit.text
    this.secondLocked = locked && !edit.done ? edit.text : null
    return edit.text
  }

  /** Picks an option of the list with the pointer. */
  choose(keyword: string) {
    this.menu = null
    this.notify()
    this.pick?.(keyword)
  }

  /**
   * The cursor's anchor overlay was drawn at canvas point (`x`, `y`): the tooltip goes right of it
   * and below (above near the canvas's bottom, where the command line floats; left near its right
   * edge), with the live values of this frame.
   */
  place(x: number, y: number, canvas: HTMLCanvasElement, overlays: readonly EngineOverlay[]) {
    const label = overlays.find((overlay) => overlay.kind === 'label' && overlay.color.every((value, at) => value === DYNAMIC_TAG[at]))
    let live: Live | null = null
    if (label?.kind === 'label') {
      try { live = JSON.parse(label.text) as Live } catch { live = null }
    }
    this.live = live
    const bounds = canvas.getBoundingClientRect()
    if (!this.menu || !this.at) this.at = { x: bounds.left + x, y: bounds.top + y, canvas: { left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height } }
    this.paint()
  }

  /** Puts the tooltip where the cursor is and writes the live values into its empty boxes. */
  paint() {
    const box = this.box
    if (!box) return
    const at = this.at
    if (!at || !this.inside || !this.shows()) {
      box.style.visibility = 'hidden'
      return
    }
    for (const element of box.querySelectorAll<HTMLElement>('[data-live]')) {
      const [kind, index] = (element.dataset.live ?? '').split('.')
      const values = kind === 'polar' ? this.live?.polar : kind === 'cartesian' ? this.live?.cartesian : null
      const text = values?.[Number(index)] ?? ''
      if (element.textContent !== text) element.textContent = text
    }
    const width = box.offsetWidth
    const height = box.offsetHeight
    const { canvas } = at
    let left = at.x + OFFSET_X
    let top = at.y + OFFSET_Y
    if (left + width > canvas.left + canvas.width - 4) left = Math.max(canvas.left + 4, at.x - OFFSET_X - width)
    const bottom = canvas.top + canvas.height - BOTTOM_CLEAR
    if (top + height > bottom) top = Math.max(canvas.top + 4, Math.min(at.y - 22 - height, bottom - height))
    box.style.transform = `translate(${Math.round(left)}px, ${Math.round(top)}px)`
    box.style.visibility = 'visible'
  }
}
