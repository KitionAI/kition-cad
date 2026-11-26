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
import { CAD_PRIMARY, DRAFTING, HOST_FILE_COMMANDS, MENU_TABS, describe } from './cad'
import { HomeTab } from './ribbon/HomeTab'
import { CadCommandTab } from './ribbon/CommandTab'
import { CatalogProvider } from './commands'
import { LayersPanel } from './LayersPanel'
import { CommandLine } from './CommandLine'
import { CadCanvas } from './CadCanvas'
import { ConstraintLease, constraintLeaseScope, withConstraintLease } from './constraintLease'
import { CadCursor } from './cursor'

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
  const commandInput = useRef<HTMLInputElement>(null)
  const [history, setHistory] = useState<string[]>([])
  useEffect(() => setHistory([]), [doc])
  // F2 opens the command line's whole history, scrollable, and closes it.
  const [historyOpen, setHistoryOpen] = useState(false)

  const tabs: FrameTab[] = [
    { id: 'home', label: phrase('Home'), ribbon: state ? <HomeTab info={state} onFit={() => { void editor.run('view.zoomToFit', {}) }} onLayers={() => setPanel((open) => !open)} /> : null },
    ...MENU_TABS.map((menu) => ({ id: menu, label: phrase(menu), view: menu === 'View', ribbon: <CadCommandTab menu={menu} exclude={menu === 'File' ? HOST_FILE_COMMANDS : []} primary={CAD_PRIMARY[menu]} /> })),
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
        commandInput.current.focus()
        commandInput.current.setRangeText(event.key, 0, commandInput.current.value.length, 'end')
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
                {/* Written as the canvas draws the engine's answer (cursor.ts), not by React. */}
                <span className="cad-coordinates" data-testid="cad-coordinates" ref={(element) => { cursor.readout = element }} />
                <span data-testid="cad-status">{t('objects', { count: state.entities })}</span>
                <span>{t('layer', { name: state.currentLayer })}</span>
              </>
            ) : null
          }
          statusEnd={
            state ? (
              <span className="cad-drafting">
                <button type="button" className="cad-mode" disabled={!editor.enabled('view.zoomToFit')} onClick={() => void editor.run('view.zoomToFit', {})} data-testid="cad-status-zoom-extents">
                  {t('zoomExtents')}
                </button>
                {DRAFTING.map((mode) => (
                  <button
                    key={mode.id}
                    type="button"
                    className="cad-mode"
                    aria-pressed={state.drafting[mode.id]}
                    disabled={!editor.enabled(mode.command)}
                    onClick={() => void editor.run(mode.command, { on: !state.drafting[mode.id] })}
                    data-testid={`cad-mode-${mode.id}`}
                  >
                    {phrase(mode.label)}
                  </button>
                ))}
              </span>
            ) : null
          }
          panels={panel && state ? <LayersPanel info={state} /> : null}
        >
          <CadCanvas canvas={canvas} viewMemory={viewMemory} info={state} cursor={cursor} history={history} input={commandInput} onZoom={setZoom} />
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
              />
            </FrameChrome>
          ) : null}
        </EditorFrame>
      </div>
      </CatalogProvider>
    </KitEditorProvider>
  )
}
