import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react'
import { useKitEditor, useKitTools, type KitCanvasHandle } from '@kition/sdk/editor'
import type { CadInfo, EditRequest, TextBox } from './cad'

/**
 * What the in-place box edits: an object's text (a double-click, or Edit Text), or the text TEXT
 * and MTEXT ask for at their "Enter text" prompt.
 */
export type InPlace =
  | { source: 'object'; kind: 'text' | 'paragraphs' | 'dimension'; handle: string; text: string; box: TextBox }
  | { source: 'prompt'; box: TextBox; key: string }

/** The smallest and largest font size the box takes (CSS pixels), however far the view is zoomed. */
const MIN_FONT = 11
const MAX_FONT = 96

/** Where a box goes on the page: drawing units to the canvas's pixels, through the view the canvas shows. */
function place(box: TextBox, info: CadInfo, canvas: KitCanvasHandle | null, surface: Element | null) {
  const view = canvas?.view()
  const units = info.units
  if (!view || !units || !surface || !(units.perPixel > 0)) return null
  const bounds = surface.getBoundingClientRect()
  const toPage = (x: number, y: number) => ({
    x: bounds.left + (units.origin[0] + x / units.perPixel - view.x) * view.zoom,
    y: bounds.top + (units.origin[1] - y / units.perPixel - view.y) * view.zoom,
  })
  const scale = view.zoom / units.perPixel
  const height = box.height * scale
  const at = toPage(box.at[0], box.at[1])
  const font = Math.max(MIN_FONT, Math.min(MAX_FONT, height))
  return { at, font, height, width: box.width * scale, rotation: box.rotation }
}

/**
 * The text box CAD types in where the text is (walkthrough N12): a real input (a paragraph's is a
 * text area) placed and sized by the view, so an input method's candidates follow it and Space types
 * a space. An object's text: all of it selected; Enter (a paragraph: Ctrl or Cmd+Enter) or a click
 * elsewhere writes it with `text.edit`, one undo step; Escape leaves it. Emptied, a text keeps its
 * old value and a dimension gets its measurement back (`<>`). At TEXT's prompt Enter sends the line
 * (the box moves to the next one) and Enter on an empty box ends TEXT; at MTEXT's, the lines go as one
 * paragraph text (`\P` between them), then Enter. Escape there cancels the command.
 */
export function InPlaceText({ edit, info, canvas, region, onDone }: { edit: InPlace; info: CadInfo; canvas: RefObject<KitCanvasHandle | null>; region: RefObject<HTMLDivElement | null>; onDone: () => void }) {
  const editor = useKitEditor()
  const tools = useKitTools()
  const initial = edit.source === 'object' ? edit.text : ''
  const [text, setText] = useState(initial)
  const field = useRef<HTMLInputElement & HTMLTextAreaElement>(null)
  const settled = useRef(false)
  const [, redraw] = useState(0)
  const paragraphs = edit.box.paragraphs
  const key = edit.source === 'object' ? `${edit.handle}` : edit.key
  useEffect(() => { setText(initial); settled.current = false }, [key, initial])
  // Opened: the keyboard goes to the box, its text selected. Closed with the keyboard still in it
  // (Enter, Escape), the keyboard goes back where it was, the drawing, so Escape and Delete reach
  // it again (regression issue-in-place-focus); closed by a click elsewhere, it stays there.
  useLayoutEffect(() => {
    const element = field.current
    if (!element) return
    const before = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null
    element.focus({ preventScroll: true })
    element.select()
    return () => {
      const now = document.activeElement
      if (now && now !== document.body && now !== element) return
      const back = before?.isConnected ? before : document.querySelector<HTMLElement>('.kui-app')
      back?.focus({ preventScroll: true })
    }
  }, [key])
  // The view moved (zoom, pan, resize): the box follows on the next frame.
  useEffect(() => {
    let frame = 0
    let last = ''
    const tick = () => {
      const view = canvas.current?.view()
      const now = view ? `${view.x},${view.y},${view.zoom},${view.width},${view.height}` : ''
      if (now !== last) { last = now; redraw((n) => n + 1) }
      frame = window.requestAnimationFrame(tick)
    }
    frame = window.requestAnimationFrame(tick)
    return () => window.cancelAnimationFrame(frame)
  }, [canvas])
  const line = (entry: string) => editor.run('command.runLine', { text: entry }).then(() => tools?.refresh())
  const commit = async () => {
    if (settled.current) return
    settled.current = true
    if (edit.source === 'object') {
      onDone()
      let value = text
      if (!value.trim()) {
        if (edit.kind !== 'dimension') return
        value = '<>'
      }
      if (value !== edit.text) await editor.run('text.edit', { handle: edit.handle, text: value })
      return
    }
    const value = paragraphs ? text.split(/\r?\n/).join('\\P') : text
    setText('')
    if (paragraphs) {
      if (value.trim()) await line(value)
      await line('')
    } else {
      await line(value)
    }
    settled.current = false
  }
  const cancel = () => {
    settled.current = true
    onDone()
    if (edit.source === 'prompt') tools?.keyDown({ key: 'Escape', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false })
  }
  const where = place(edit.box, info, canvas.current, region.current?.querySelector('[data-testid="kit-canvas-surface"]') ?? region.current?.querySelector('canvas') ?? null)
  if (!where) return null
  const lineHeight = where.font * 1.25
  // A single line sits on its baseline; a paragraph and a dimension text from their top / middle.
  const top = edit.box.centered ? where.at.y - lineHeight / 2 : paragraphs ? where.at.y : where.at.y - where.font * 1.05
  const left = edit.box.centered ? where.at.x : where.at.x
  const style: React.CSSProperties = {
    position: 'fixed',
    left,
    top,
    fontSize: where.font,
    lineHeight: `${lineHeight}px`,
    minWidth: Math.max(80, paragraphs ? where.width : 0),
    width: paragraphs && where.width > 0 ? where.width : undefined,
    transform: `${edit.box.centered ? 'translateX(-50%) ' : ''}rotate(${-where.rotation}deg)`,
    transformOrigin: edit.box.centered ? 'center' : paragraphs ? 'top left' : 'bottom left',
  }
  const keys = (event: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => {
    // Every key is the box's: none reaches the canvas, the command line or the editor's shortcuts.
    event.stopPropagation()
    if (event.nativeEvent.isComposing) return
    if (event.key === 'Escape') { event.preventDefault(); cancel(); return }
    if (event.key === 'Enter' && (!paragraphs || event.metaKey || event.ctrlKey)) { event.preventDefault(); void commit() }
  }
  const props = {
    ref: field,
    className: 'cad-in-place',
    style,
    value: text,
    spellCheck: false,
    'aria-label': editor.phrase(edit.source === 'prompt' ? 'Enter text' : 'Edit Text'),
    onKeyDown: keys,
    onBlur: () => {
      // Leaving the box writes it (a prompt's only when something is typed).
      if (edit.source === 'prompt' && !text.trim()) return
      void commit()
    },
    'data-testid': 'cad-in-place',
    'data-kind': edit.source === 'object' ? edit.kind : paragraphs ? 'paragraphs' : 'text',
  }
  return paragraphs
    ? <textarea {...props} rows={Math.max(2, text.split('\n').length)} onChange={(event) => setText(event.target.value)} />
    : <input {...props} type="text" size={Math.max(4, text.length + 1)} onChange={(event) => setText(event.target.value)} />
}

/** The in-place edit a double-click asked for, from `/drafting/edit`: new ones only (by serial). */
export function useEditRequests(info: CadInfo | null, open: (request: EditRequest) => void) {
  const seen = useRef<number | null>(null)
  const serial = info?.drafting.edit?.serial ?? 0
  const ready = info != null
  useEffect(() => {
    if (!info) return
    // The request the drawing had when it opened is not new.
    if (seen.current == null) { seen.current = serial; return }
    if (serial === seen.current) return
    seen.current = serial
    const request = info.drafting.edit
    if (request) open(request)
  }, [serial, ready])
}
