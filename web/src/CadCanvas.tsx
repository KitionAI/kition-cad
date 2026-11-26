import { useEffect, useMemo, useRef, useState, type RefObject } from 'react'
import { ContextMenu, KitDocumentCanvas, MenuItem, MenuSeparator, useKitEditor, useKitTools, type KitCanvasHandle, type KitCanvasView, type RasterView } from '@kition/sdk/editor'
import { describe, gripping as isGripping, type CadInfo } from './cad'
import { cursorState, gridDecoration, ucsDecoration, type CadCursor, type GripUnder } from './cursor'
import { startsPrompt, useCatalog, useStartCommand } from './commands'
import { DynamicTooltip } from './DynamicTooltip'

/** A right press shorter than this is Enter; a longer one opens the command menu (as CAD does). */
const QUICK_RIGHT_MS = 250
/** Two middle presses closer than this (and within a few pixels) are a double click: Zoom Extents. */
const DOUBLE_MIDDLE_MS = 400
/** How near (page pixels) a right press is to a grip to open the grip's menu (the engine's pick range). */
const GRIP_PX = 8
/** The pointer resting this long on the canvas acquires the object snap under it for tracking. */
const DWELL_MS = 400
/** One wheel notch (100 pixels of delta) zooms by e^0.2, about 1.22 times. */
const WHEEL_ZOOM = 0.002
/** A wheel event within this long of a trackpad's keeps panning (a gesture's events vary). */
const TRACKPAD_HOLD_MS = 200

/** The object snap overrides of the snap menu: the modifier typed, and its label. */
const SNAPS: Array<[string, string]> = [
  ['_end', 'Endpoint'],
  ['_mid', 'Midpoint'],
  ['_int', 'Intersection'],
  ['_cen', 'Center'],
  ['_qua', 'Quadrant'],
  ['_tan', 'Tangent'],
  ['_per', 'Perpendicular'],
  ['_nea', 'Nearest'],
  ['_non', 'None'],
]
/** Pan and Zoom in the command menu: typed transparently, so the command keeps running. */
const PANS: Array<[string, string]> = [['pan.left', 'Left'], ['pan.right', 'Right'], ['pan.up', 'Up'], ['pan.down', 'Down']]
const ZOOMS: Array<[string, string]> = [['zoom.extents', 'Extents'], ['zoom.in', 'In'], ['zoom.out', 'Out'], ['zoom.previous', 'Previous']]

/** Zoom in the drawing menu between commands: the view commands themselves. */
const VIEW_ZOOMS: Array<[string, string]> = [['view.zoomToFit', 'Extents'], ['view.zoomIn', 'In'], ['view.zoomOut', 'Out'], ['view.zoomPrevious', 'Previous']]
/** The grip modes, in the order Space cycles them: the word typed for each, and its label. */
const GRIP_MODES: Array<[string, string]> = [['stretch', 'Stretch'], ['move', 'Move'], ['rotate', 'Rotate'], ['scale', 'Scale'], ['mirror', 'Mirror']]
/** The edit each kind of object offers in the drawing menu when everything selected is of it. */
const TYPE_EDITS: Array<[string[], string, string]> = [
  [['Text', 'MText'], 'text.edit', 'Edit Text'],
  [['Hatch'], 'hatch.edit', 'Edit Hatch'],
  [['Polyline'], 'polyline.edit', 'Edit Polyline'],
  [['Dimension'], 'dimension.editText', 'Edit Dimension Text'],
]
/** The most recent commands the drawing menu lists. */
const RECENT = 8

type MenuKind = 'drawing' | 'command' | 'snap' | 'pan' | 'zoom' | 'grip' | 'recent' | 'viewZoom' | 'drawOrder'
type Menu = { kind: MenuKind; x: number; y: number; grip?: GripUnder | null }
type Press = { time: number; x: number; y: number; shift: boolean; running: boolean; grip: GripUnder | null }

/** A test id part for an option: `Base point` is `Base-point`. */
const dashed = (text: string) => text.replace(/\s+/g, '-')

/** CAD keeps its model view and the raster viewport in agreement without changing other kits. */
export function CadCanvas({ canvas, viewMemory, info, cursor, history, input, onZoom, onProperties }: { canvas: RefObject<KitCanvasHandle | null>; viewMemory: RefObject<RasterView | null>; info: CadInfo | null; cursor: CadCursor; history: string[]; input: RefObject<HTMLInputElement | null>; onZoom: (zoom: number) => void; onProperties: () => void }) {
  const editor = useKitEditor()
  const tools = useKitTools()
  const catalog = useCatalog()
  const start = useStartCommand()
  const region = useRef<HTMLDivElement>(null)
  const lastView = useRef('')
  const applied = useRef(0)
  const [menu, setMenu] = useState<Menu | null>(null)
  const rightPress = useRef<Press | null>(null)
  const menuPress = useRef<Press | null>(null)
  const middle = useRef<{ time: number; x: number; y: number } | null>(null)
  const dwell = useRef<number | null>(null)
  const { doc } = editor.session
  // The drawn cursor's state, read by the overlay hook when the canvas draws (set while rendering:
  // the canvas, a child, draws in its effects, which run before this component's).
  const drawn = cursorState(tools?.canvasTool.cursor)
  cursor.state = drawn
  cursor.size = info?.drafting.cursorSize ?? 5
  // The dynamic input tooltip follows the prompt (F12, `dynamicInput.ts`).
  const dynamic = cursor.dynamic
  dynamic.on = info?.drafting.dynamicInput ?? false
  dynamic.running = info?.running != null
  dynamic.prompt = info?.drafting.commandLine.prompt ?? null
  dynamic.execute = doc ? (command, params) => doc.execute(command, params) : null
  dynamic.refresh = () => tools?.refresh()
  if (dynamic.menu && dynamic.menu.keywords.join('/') !== (dynamic.prompt?.keywords ?? []).join('/')) dynamic.menu = null
  const gridKey = info ? JSON.stringify([info.drafting.grid, info.drafting.gridUnit, info.drafting.gridMajor, info.units]) : ''
  const grid = useMemo(() => gridDecoration(info, cursor.palette), [gridKey, cursor])
  const ucs = useMemo(() => ucsDecoration(cursor.palette), [cursor])
  /** The pointer came onto the canvas or left it (for a menu, a panel): the crosshair follows. */
  const over = (inside: boolean) => {
    if (cursor.inside === inside) return
    cursor.inside = inside
    tools?.refresh()
  }
  useEffect(() => { applied.current = 0; lastView.current = '' }, [doc])

  /** The engine's state now (the tool calls bring it up to date before React renders it). */
  const fresh = (): CadInfo | null => (doc ? describe(doc.info) : info)
  /** A command or a hot grip is waiting for input. */
  const prompting = () => {
    const now = fresh()
    return now != null && (now.running != null || isGripping(now))
  }
  const refresh = () => tools?.refresh()
  const typeLine = (text: string) => { void editor.run('command.runLine', { text }).then(refresh) }
  /** Enter, as the Enter key gives it: what was typed on the command line, or Enter. */
  const enter = () => {
    const typed = input.current?.value ?? ''
    if (typed.trim()) {
      if (input.current) input.current.value = ''
      // What the dynamic input boxes show is sent, as Enter sends it.
      if (dynamic.completes(typed)) {
        const done = dynamic.finish(typed)
        dynamic.reset()
        void done.then(typeLine)
        return
      }
      dynamic.reset()
      typeLine(typed)
      return
    }
    const sent = tools?.keyDown({ key: 'Enter', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false }) ?? false
    if (!sent) typeLine('')
  }
  const cancel = () => {
    if (input.current) input.current.value = ''
    dynamic.reset()
    const sent = tools?.keyDown({ key: 'Escape', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false }) ?? false
    if (!sent) typeLine('')
  }
  /** Opens a submenu where the menu was (after the menu that chose it has closed). */
  const open = (kind: MenuKind) => {
    const at = menu
    if (at) window.setTimeout(() => setMenu({ kind, x: at.x, y: at.y }), 0)
  }
  /**
   * The grip menu's choice on a grip that is not hot: the grip goes hot in `mode`, or answers
   * `option`; an option that changes the drawing at once (Remove vertex) is answered at the prompt.
   */
  const editGrip = (grip: GripUnder, choice: { mode?: string; option?: string }, answer?: string) => {
    void editor.run('grip.edit', { handle: grip.handle, index: grip.index, ...choice }).then(() => (answer ? typeLine(answer) : refresh()))
  }

  const synchronize = (view: KitCanvasView) => {
    onZoom(view.zoom)
    const value = { x: view.x, y: view.y, width: view.width, height: view.height, zoom: view.zoom }
    const key = JSON.stringify(value)
    if (!doc || view.width <= 0 || view.height <= 0 || key === lastView.current) return
    lastView.current = key
    void doc.execute('viewport.sync', value).catch((error) => editor.report('Could not synchronize drawing view', error))
  }

  useEffect(() => {
    const request = info?.viewRequest
    const viewport = canvas.current
    const view = viewport?.view()
    if (!request || !view || !viewport || request.serial === applied.current) return
    applied.current = request.serial
    viewport.zoomTo(view.height / request.height)
    const placed = viewport.view()
    if (placed) viewport.panTo(request.x - placed.width / (2 * placed.zoom), request.y - placed.height / (2 * placed.zoom))
  }, [canvas, info?.viewRequest])

  useEffect(() => {
    const element = region.current
    if (!element) return
    // A trackpad's two-finger scroll pans (kit-editor's canvas does that); a mouse wheel zooms
    // about the pointer. A trackpad sends pixel deltas with a sideways part or in fractions; when
    // unsure it is a wheel. Pinch (Ctrl or Cmd) zooms through kit-editor.
    let trackpadUntil = 0
    const wheel = (event: WheelEvent) => {
      if (!(event.target instanceof HTMLCanvasElement) || event.ctrlKey || event.metaKey) return
      const trackpad = event.deltaMode === 0 && (event.deltaX !== 0 || !Number.isInteger(event.deltaY))
      if (trackpad || (event.deltaMode === 0 && event.timeStamp < trackpadUntil)) {
        trackpadUntil = event.timeStamp + TRACKPAD_HOLD_MS
        return
      }
      const viewport = canvas.current
      const view = viewport?.view()
      if (!viewport || !view) return
      event.preventDefault()
      event.stopPropagation()
      const bounds = event.target.getBoundingClientRect()
      const x = event.clientX - bounds.left
      const y = event.clientY - bounds.top
      const delta = event.deltaY * (event.deltaMode === 1 ? 100 / 3 : event.deltaMode === 2 ? bounds.height : 1)
      viewport.zoomTo(view.zoom * Math.exp(-Math.max(-500, Math.min(500, delta)) * WHEEL_ZOOM))
      const next = viewport.view()
      if (next) viewport.panTo(view.x + x / view.zoom - x / next.zoom, view.y + y / view.zoom - y / next.zoom)
    }
    element.addEventListener('wheel', wheel, { capture: true, passive: false })
    return () => element.removeEventListener('wheel', wheel, true)
  }, [canvas])

  useEffect(() => () => { if (dwell.current != null) window.clearTimeout(dwell.current) }, [])

  // A command with prompts starts them, as a CAD menu does; one without runs.
  const run = (id: string) => { if (startsPrompt(catalog.byId.get(id))) start(id); else void editor.run(id, {}).then(refresh) }
  const command = (id: string, label: string) => (
    <MenuItem key={id} label={editor.phrase(label)} disabled={!editor.enabled(id)} onSelect={() => run(id)} testId={`cad-context-${id}`} />
  )
  // An object's own edit starts its prompts on the selection (its dialog-like title notwithstanding).
  const edit = (id: string, label: string) => {
    const entry = catalog.byId.get(id)
    return (
      <MenuItem key={id} label={editor.phrase(label)} disabled={!editor.enabled(id)} onSelect={() => { if (entry?.interactive && entry.commandLine) start(id); else void editor.run(id, {}).then(refresh) }} testId={`cad-context-${id}`} />
    )
  }
  const unanswerable = () => !prompting() || !editor.enabled('command.runLine')
  const now = fresh()
  // The prompt's options, as the engine gives them (the command line draws the same).
  const options = now?.drafting.commandLine.prompt.keywords ?? []
  const gripping = isGripping(now)
  const selected = now?.selectedObjects ?? []
  const types = new Set(selected.map((object) => object.type))
  // The commands run from the command line lately, newest first ("Command: LINE" on the history),
  // then those typed here; Repeat names the newest.
  const recent: string[] = []
  for (const line of [...(now?.drafting.commandLine.history.lines ?? [])].reverse()) {
    const name = /^Command:\s+'?([A-Za-z][\w.-]*)\s*$/.exec(line)?.[1]
    if (name && !recent.includes(name.toUpperCase())) recent.push(name.toUpperCase())
  }
  for (const text of [...history].reverse()) if (/^[A-Za-z][\w.-]*$/.test(text) && !recent.includes(text.toUpperCase())) recent.push(text.toUpperCase())
  const grip = menu?.kind === 'grip' ? menu.grip ?? null : null
  return (
    <div
      ref={region}
      className="kui-row cad-canvas"
      data-cursor={drawn ? 'drawn' : undefined}
      onPointerOver={(event) => over(event.target instanceof HTMLCanvasElement)}
      onPointerOut={(event) => { if (event.target instanceof HTMLCanvasElement && !(event.relatedTarget instanceof HTMLCanvasElement)) over(false) }}
      onPointerDownCapture={(event) => {
        if (!(event.target instanceof HTMLCanvasElement)) return
        if (dwell.current != null) window.clearTimeout(dwell.current)
        if (event.button === 2) {
          const running = prompting()
          // The grip under the pointer, when the last frame's is still where the press is.
          const under = cursor.grip
          const onGrip = !running && under != null && Math.hypot(under.x - event.clientX, under.y - event.clientY) <= GRIP_PX
          const press = { time: event.timeStamp, x: event.clientX, y: event.clientY, shift: event.shiftKey || event.ctrlKey, running, grip: onGrip ? under : null }
          rightPress.current = press
          menuPress.current = press
        } else if (event.button === 1) {
          const last = middle.current
          if (last && event.timeStamp - last.time < DOUBLE_MIDDLE_MS && Math.hypot(event.clientX - last.x, event.clientY - last.y) < 6) {
            middle.current = null
            void editor.run('view.zoomToFit', {}).then(refresh)
          } else {
            middle.current = { time: event.timeStamp, x: event.clientX, y: event.clientY }
          }
        }
      }}
      onPointerUpCapture={(event) => {
        // A click places the point: what was typed in the dynamic input boxes is used up (the
        // engine forgets its copy as it takes the click).
        if (event.button === 0 && event.target instanceof HTMLCanvasElement && dynamic.dynamic && dynamic.text && prompting()) {
          if (input.current) input.current.value = ''
          dynamic.reset()
        }
        const press = rightPress.current
        if (event.button !== 2 || !press) return
        rightPress.current = null
        if (press.shift) setMenu({ kind: 'snap', x: event.clientX, y: event.clientY })
        // On a grip between edits: the grip's menu (its modes and options), however long the press.
        else if (press.grip) setMenu({ kind: 'grip', x: event.clientX, y: event.clientY, grip: press.grip })
        else if (press.running && event.timeStamp - press.time < QUICK_RIGHT_MS) enter()
        // Held with a hot grip: the grip's menu; in a command: the command menu.
        else if (press.running) setMenu({ kind: isGripping(fresh()) ? 'grip' : 'command', x: event.clientX, y: event.clientY })
      }}
      onPointerMove={(event) => {
        // The page tells the engine when the pointer rests: one repeated move after the dwell,
        // which acquires the object snap under it for tracking (a repeat is not a new place).
        if (!event.isTrusted || !(event.target instanceof HTMLCanvasElement) || event.buttons !== 0) return
        if (dwell.current != null) window.clearTimeout(dwell.current)
        if (!prompting()) return
        const target = event.target
        const { clientX, clientY, pointerId, pointerType } = event
        dwell.current = window.setTimeout(() => {
          dwell.current = null
          target.dispatchEvent(new PointerEvent('pointermove', { clientX, clientY, pointerId, pointerType, bubbles: true, cancelable: true }))
        }, DWELL_MS)
      }}
      onPointerLeave={() => { if (dwell.current != null) window.clearTimeout(dwell.current) }}
      onContextMenu={(event) => {
        if (!(event.target instanceof HTMLCanvasElement)) return
        event.preventDefault()
        // In a command a right-click is Enter or the command menu, and Shift+right-click is the
        // snap menu, when the button comes up; between commands it is the drawing menu.
        const press = menuPress.current
        menuPress.current = null
        if (press && (press.shift || press.running || press.grip)) return
        // Ctrl+click on a Mac is the system's right-click: during an edit it is a click (a copy).
        if (!press && event.ctrlKey && prompting()) return
        setMenu({ kind: 'drawing', x: event.clientX, y: event.clientY })
      }}
    >
      <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} framed={false} onView={synchronize} decorateUnder={grid} decorate={ucs} />
      <DynamicTooltip dynamic={dynamic} />
      <ContextMenu at={menu?.kind === 'drawing' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Drawing')} testId="cad-context-menu">
        <MenuItem label={recent[0] ? `${editor.phrase('Repeat')} ${recent[0]}` : editor.phrase('Repeat last command')} disabled={!editor.enabled('command.runLine')} onSelect={() => typeLine('')} testId="cad-context-repeat" />
        <MenuItem label={`${editor.phrase('Recent Commands')} ▸`} disabled={!recent.length || !editor.enabled('command.runLine')} onSelect={() => open('recent')} testId="cad-context-recent" />
        <MenuSeparator />
        {selected.length ? (
          <>
            {command('edit.delete', 'Erase')}{command('object.move', 'Move')}{command('object.copy', 'Copy Selection')}
            {command('object.scale', 'Scale')}{command('object.rotate', 'Rotate')}{command('object.mirror', 'Mirror')}
            <MenuItem label={`${editor.phrase('Draw Order')} ▸`} onSelect={() => open('drawOrder')} testId="cad-context-draworder" />
            {command('arrange.group', 'Group')}{command('arrange.ungroup', 'Ungroup')}
            {TYPE_EDITS.filter(([kinds]) => [...types].every((type) => kinds.includes(type))).map(([, id, label]) => edit(id, label))}
            <MenuSeparator />
            {command('edit.deselectAll', 'Deselect All')}
            <MenuItem label={editor.phrase('Properties')} onSelect={onProperties} testId="cad-context-properties" />
            <MenuSeparator />
            {command('edit.undo', 'Undo')}{command('edit.redo', 'Redo')}
          </>
        ) : (
          <>
            {command('edit.undo', 'Undo')}{command('edit.redo', 'Redo')}
            {command('edit.paste', 'Paste')}{command('edit.selectAll', 'Select All')}
          </>
        )}
        <MenuSeparator />
        <MenuItem label={editor.phrase('Pan')} onSelect={() => tools?.select('hand')} testId="cad-context-pan" />
        <MenuItem label={`${editor.phrase('Zoom')} ▸`} onSelect={() => open('viewZoom')} testId="cad-context-zoom" />
      </ContextMenu>
      <ContextMenu at={menu?.kind === 'recent' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Recent Commands')} testId="cad-recent-menu">
        {recent.slice(0, RECENT).map((name) => <MenuItem key={name} label={name} disabled={!editor.enabled('command.runLine')} onSelect={() => typeLine(name)} testId={`cad-recent-menu-${name}`} />)}
      </ContextMenu>
      <ContextMenu at={menu?.kind === 'viewZoom' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Zoom')} testId="cad-view-zoom-menu">
        {VIEW_ZOOMS.map(([id, label]) => <MenuItem key={id} label={editor.phrase(label)} disabled={!editor.enabled(id)} onSelect={() => void editor.run(id, {}).then(refresh)} testId={`cad-view-zoom-menu-${id}`} />)}
      </ContextMenu>
      <ContextMenu at={menu?.kind === 'drawOrder' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Draw Order')} testId="cad-draworder-menu">
        {command('arrange.bringToFront', 'Bring to Front')}{command('arrange.sendToBack', 'Send to Back')}
      </ContextMenu>
      <ContextMenu at={menu?.kind === 'grip' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Grip')} testId="cad-grip-menu">
        {grip ? (
          <>
            <MenuItem label={editor.phrase(grip.actions.some((option) => option.keyword === 'Remove vertex') ? 'Stretch Vertex' : 'Stretch')} disabled={!editor.enabled('grip.edit')} onSelect={() => editGrip(grip, {})} testId="cad-grip-menu-stretch" />
            {grip.actions.map((option) => <MenuItem key={option.keyword} label={editor.phrase(option.label)} disabled={!editor.enabled('grip.edit')} onSelect={() => (option.immediate ? editGrip(grip, {}, option.keyword) : editGrip(grip, { option: option.keyword }))} testId={`cad-grip-menu-${dashed(option.keyword)}`} />)}
            <MenuSeparator />
            {GRIP_MODES.slice(1).map(([mode, label]) => <MenuItem key={mode} label={editor.phrase(label)} disabled={!editor.enabled('grip.edit')} onSelect={() => editGrip(grip, { mode })} testId={`cad-grip-menu-${mode}`} />)}
          </>
        ) : (
          <>
            <MenuItem label={editor.phrase('Enter')} onSelect={enter} testId="cad-grip-menu-enter" />
            <MenuSeparator />
            {GRIP_MODES.map(([mode, label]) => <MenuItem key={mode} label={editor.phrase(label)} disabled={unanswerable()} onSelect={() => typeLine(mode)} testId={`cad-grip-menu-${mode}`} />)}
            <MenuSeparator />
            {options.map((option) => <MenuItem key={option} label={editor.phrase(option)} disabled={unanswerable()} onSelect={() => typeLine(option)} testId={`cad-grip-menu-${dashed(option)}`} />)}
            <MenuSeparator />
            <MenuItem label={editor.phrase('Cancel')} onSelect={cancel} testId="cad-grip-menu-cancel" />
          </>
        )}
      </ContextMenu>
      <ContextMenu at={menu?.kind === 'command' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Command')} testId="cad-command-menu">
        <MenuItem label={editor.phrase('Enter')} onSelect={enter} testId="cad-command-menu-enter" />
        <MenuItem label={editor.phrase('Cancel')} onSelect={cancel} testId="cad-command-menu-cancel" />
        {options.length && !gripping ? <MenuSeparator /> : null}
        {gripping ? null : options.map((option) => <MenuItem key={option} label={option} disabled={unanswerable()} onSelect={() => typeLine(option)} testId={`cad-command-menu-${option}`} />)}
        <MenuSeparator />
        <MenuItem label={`${editor.phrase('Snap Overrides')} ▸`} disabled={gripping} onSelect={() => open('snap')} testId="cad-command-menu-snaps" />
        <MenuItem label={`${editor.phrase('Pan')} ▸`} onSelect={() => open('pan')} testId="cad-command-menu-pan" />
        <MenuItem label={`${editor.phrase('Zoom')} ▸`} onSelect={() => open('zoom')} testId="cad-command-menu-zoom" />
      </ContextMenu>
      <ContextMenu at={menu?.kind === 'snap' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Object Snap')} testId="cad-snap-menu">
        {SNAPS.map(([modifier, label]) => (
          <MenuItem key={modifier} label={editor.phrase(label)} disabled={gripping || unanswerable()} onSelect={() => typeLine(modifier)} testId={`cad-snap-menu-${modifier}`} />
        ))}
      </ContextMenu>
      <ContextMenu at={menu?.kind === 'pan' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Pan')} testId="cad-pan-menu">
        {PANS.map(([name, label]) => <MenuItem key={name} label={editor.phrase(label)} onSelect={() => typeLine(`'${name}`)} testId={`cad-pan-menu-${name}`} />)}
      </ContextMenu>
      <ContextMenu at={menu?.kind === 'zoom' ? menu : null} onClose={() => setMenu(null)} label={editor.phrase('Zoom')} testId="cad-zoom-menu">
        {ZOOMS.map(([name, label]) => <MenuItem key={name} label={editor.phrase(label)} onSelect={() => typeLine(`'${name}`)} testId={`cad-zoom-menu-${name}`} />)}
      </ContextMenu>
    </div>
  )
}
