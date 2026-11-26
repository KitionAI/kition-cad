import { useEffect, useMemo, useRef, useState } from 'react'
import {
  HAND_TOOL,
  KitEditorProvider,
  EditorFrame,
  FrameChrome,
  ZOOM_TOOL,
  createKitStrings,
  kitLeases,
  toast,
  useEngineBackdrop,
  useEngineTools,
  useKitEditorState,
  useKitSession,
  type FrameTab,
  type HostConnection,
  type KitCanvasHandle,
  type Phrases,
  type RasterView,
} from '@kition/sdk/editor'
import { engineManifest } from './engine'
import { CATALOGS } from './strings'
import { CAD_PRIMARY, HOST_FILE_COMMANDS, MENU_TABS, describe, gripping } from './cad'
import { HomeTab } from './ribbon/HomeTab'
import { CadCommandTab } from './ribbon/CommandTab'
import { CatalogProvider } from './commands'
import { LayersPanel, type PanelPart } from './LayersPanel'
import { HatchDialog, type HatchEdit } from './HatchDialog'
import { CommandLine } from './CommandLine'
import { CadCanvas } from './CadCanvas'
import { ConstraintLease, constraintLeaseScope, withConstraintLease } from './constraintLease'
import { CadCursor } from './cursor'
import { DraftingStatus, LayoutTabs } from './StatusBar'

/** The view tools are the host's; the engine's other tools run in the engine. */
const HOST_TOOLS = { hand: HAND_TOOL, zoom: ZOOM_TOOL }

export function App({ host, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  const manifest = useMemo(engineManifest, [])
  // An agent's constraint command takes the drawing's constraint lease too (`constraintLease.ts`).
  // A drawing changed outside Kition opens with its stored replica (Kit UI protocol UI-R1): the
  // engine records what the file changed as one `import` change onto the replica
  // (`open_with_replica`), so the entities keep their object ids (tests/replica.mjs).
  const session = useKitSession(host, manifest, describe, {
    agentLeaseScope: (command, params) => constraintLeaseScope(command, params),
  })
  // The person's constraint commands take the drawing's constraint lease while it is co-edited.
  const leases = useMemo(() => kitLeases(host), [host])
  const running = useRef<string | null>(null)
  running.current = session.state?.running ?? null
  const constraintLease = useMemo(
    () => new ConstraintLease(leases, (holder) => toast('info', t('lease.constraints', { name: holder || t('lease.someone') })), () => running.current),
    [leases, t],
  )
  useEffect(() => () => constraintLease.release(), [constraintLease])
  useEffect(() => constraintLease.follow(), [constraintLease, session.state?.running])
  const leasedRun = useMemo(() => withConstraintLease(session.run, constraintLease), [session.run, constraintLease])
  const editor = useKitEditorState({ ...session, run: leasedRun }, host, t, 'cad', phrase)
  const { doc, change, state } = session
  // The crosshair, pick box and coordinate readout CAD draws from the engine's cursor overlays.
  const cursor = useMemo(() => new CadCursor(), [])
  const tools = useEngineTools(session.tool, doc != null, change, { initial: 'select', hostTools: HOST_TOOLS, drawOverlay: cursor.drawOverlay })
  const canvas = useRef<KitCanvasHandle>(null)
  // Model space is drawn on the host's canvas colour, not on paper (the engine's `setBackdrop`).
  useEngineBackdrop(doc, canvas)
  const viewMemory = useRef<RasterView | null>(null)
  const [zoom, setZoom] = useState(1)
  const [tab, setTab] = useState('home')
  const [panel, setPanel] = useState(true)
  // The part of the right panel asked for last: the ribbon's Layer Properties shows the layers, the
  // drawing menu's Properties the properties; neither hides the panel (B8), its own button does.
  const [panelFocus, setPanelFocus] = useState<{ part: PanelPart; serial: number }>({ part: 'properties', serial: 0 })
  const showPanel = (part: PanelPart) => {
    setPanel(true)
    setPanelFocus((last) => ({ part, serial: last.serial + 1 }))
  }
  // The hatch dialog: Draw › Hatch… (then HATCH picks points), a double-click or Edit Hatch.
  const [hatch, setHatch] = useState<HatchEdit | null>(null)
  const startHatch = (entries: string[]) => {
    const now = editor.session.doc ? describe(editor.session.doc.info) : null
    if (now && (now.running != null || gripping(now))) tools.keyDown({ key: 'Escape', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false })
    void entries.reduce<Promise<unknown>>((done, entry) => done.then(() => editor.run('command.runLine', { text: entry })), Promise.resolve()).then(async () => {
      // HATCH starts where it was left: picking objects; points are picked here.
      const after = editor.session.doc ? describe(editor.session.doc.info) : null
      if (after?.drafting.commandLine.prompt.kind === 'select' && after.running === 'hatch.add') await editor.run('command.runLine', { text: 'k' })
      tools.refresh()
    })
  }
  const commandInput = useRef<HTMLInputElement>(null)
  const [history, setHistory] = useState<string[]>([])
  useEffect(() => setHistory([]), [doc])
  // F2 opens the command line's whole history, scrollable, and closes it.
  const [historyOpen, setHistoryOpen] = useState(false)

  const tabs: FrameTab[] = [
    { id: 'home', label: phrase('Home'), ribbon: state ? <HomeTab info={state} onFit={() => { void editor.run('view.zoomToFit', {}) }} onLayers={() => showPanel('layers')} /> : null },
    ...MENU_TABS.map((menu) => ({ id: menu, label: phrase(menu), view: menu === 'View', ribbon: <CadCommandTab menu={menu} exclude={menu === 'File' ? HOST_FILE_COMMANDS : []} primary={CAD_PRIMARY[menu]} onHatch={() => setHatch({ mode: 'new' })} /> })),
  ]
  const activeTab = tabs.some((item) => item.id === tab) ? tab : 'home'

  return (
    <KitEditorProvider editor={editor} tools={tools}>
      <CatalogProvider>
      <div className="cad-editor-root" onKeyDownCapture={(event) => {
        if (event.key === 'F2' && !event.metaKey && !event.ctrlKey && !event.altKey && commandInput.current) {
          event.preventDefault()
          event.stopPropagation()
          setHistoryOpen((open) => !open)
          return
        }
        if (event.nativeEvent.isComposing || event.metaKey || event.ctrlKey || event.altKey || !editor.enabled('command.runLine')) return
        if (!(event.target instanceof Element) || !event.target.matches('.kui-app, canvas, .kui-text-sink')) return
        // The dynamic input's keys on the canvas: its options list (Down Arrow opens it; the arrows,
        // Enter, Space and Escape work it), and Tab into its boxes (F12, dynamicInput.ts).
        const dynamic = cursor.dynamic
        if ((dynamic.menu || event.key === 'Tab' || event.key === 'ArrowDown') && !(event.key === 'Tab' && event.shiftKey)) {
          const taken = dynamic.key(event.key, false)
          if (taken !== false) {
            event.preventDefault()
            event.stopPropagation()
            if (typeof taken === 'string' && commandInput.current) {
              dynamic.focusFromCanvas = true
              commandInput.current.focus()
              dynamic.focusFromCanvas = false
              commandInput.current.value = taken
              dynamic.fromCanvas = true
              commandInput.current.dispatchEvent(new Event('input', { bubbles: true }))
            }
            return
          }
        }
        // Space is Enter, as in CAD (it does not pan here: the middle button, the trackpad and the
        // Pan tool do): it ends the input of a running command, and between commands repeats the
        // last one. At a prompt that takes free text (the engine shows the text cursor) it is a
        // space typed on the command line, below.
        const freeText = tools.canvasTool.cursor === 'text' && commandInput.current != null
        if (event.key === ' ' && !event.shiftKey && !freeText) {
          event.preventDefault()
          event.stopPropagation()
          if (event.repeat) return
          const sent = tools.keyDown({ key: 'Enter', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false })
          if (!sent) void editor.run('command.runLine', { text: '' }).then(() => tools.refresh())
          return
        }
        // In Focus the command line is not drawn (it is in FrameChrome), so its input is unmounted
        // and this capture is off: a letter then picks its tool, as everywhere else.
        if (event.key.length !== 1 || (event.key === ' ' && !freeText) || !commandInput.current) return
        // Capture before the shared engine tool queues single-letter shortcuts during pointer work.
        // CAD collects a whole command in its input, even immediately after a canvas click.
        event.preventDefault()
        event.stopPropagation()
        cursor.dynamic.focusFromCanvas = true
        commandInput.current.focus()
        cursor.dynamic.focusFromCanvas = false
        commandInput.current.setRangeText(event.key, 0, commandInput.current.value.length, 'end')
        // Typed on the canvas, it is typed in the dynamic input boxes beside the cursor.
        cursor.dynamic.fromCanvas = true
        commandInput.current.dispatchEvent(new Event('input', { bubbles: true }))
      }}>
        <EditorFrame
          tabs={tabs}
          tab={activeTab}
          onTab={setTab}
          canvas={canvas}
          zoom={zoom}
          onKeyDown={(event) => {
            if (event.key !== 'Enter' || event.metaKey || event.ctrlKey || event.altKey || !editor.enabled('command.runLine')) return false
            if (!(event.target instanceof Element) || !event.target.matches('.kui-app, canvas, .kui-text-sink')) return false
            void editor.run('command.runLine', { text: '' }).then(() => tools.refresh())
            return true
          }}
          status={
            state ? (
              <>
                <LayoutTabs info={state} onSwitched={() => tools.refresh()} />
                {/* Written as the canvas draws the engine's answer (cursor.ts), not by React. */}
                <span className="cad-coordinates" data-testid="cad-coordinates" ref={(element) => { cursor.readout = element }} />
                <span data-testid="cad-status">{t('objects', { count: state.entities })}</span>
                <span>{t('layer', { name: state.currentLayer })}</span>
              </>
            ) : null
          }
          statusEnd={state ? <DraftingStatus info={state} /> : null}
          panels={panel && state ? <LayersPanel info={state} focus={panelFocus} onClose={() => setPanel(false)} /> : null}
        >
          <CadCanvas canvas={canvas} viewMemory={viewMemory} info={state} cursor={cursor} history={history} input={commandInput} onZoom={setZoom} onProperties={() => showPanel('properties')} onHatch={setHatch} />
          {hatch ? <HatchDialog edit={hatch} onClose={() => setHatch(null)} start={startHatch} /> : null}
          {state ? (
            <FrameChrome>
              <CommandLine
                prompt={state.prompt}
                line={state.drafting.commandLine}
                running={state.running}
                input={commandInput}
                history={history}
                onCommand={(text) => setHistory((items) => [...items.filter((item) => item !== text), text].slice(-100))}
                expanded={historyOpen}
                onExpanded={setHistoryOpen}
                dynamic={cursor.dynamic}
              />
            </FrameChrome>
          ) : null}
        </EditorFrame>
      </div>
      </CatalogProvider>
    </KitEditorProvider>
  )
}
