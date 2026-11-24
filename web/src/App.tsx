import { useEffect, useMemo, useRef, useState } from 'react'
import {
  CommandTab,
  HAND_TOOL,
  KitEditorProvider,
  EditorFrame,
  FrameChrome,
  ZOOM_TOOL,
  createKitStrings,
  kitLeases,
  toast,
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
import { LayersPanel } from './LayersPanel'
import { CommandLine } from './CommandLine'
import { CadCanvas } from './CadCanvas'
import { ConstraintLease, constraintLeaseScope, withConstraintLease } from './constraintLease'

/** The view tools are the host's; the engine's other tools run in the engine. */
const HOST_TOOLS = { hand: HAND_TOOL, zoom: ZOOM_TOOL }

export function App({ host, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  const manifest = useMemo(engineManifest, [])
  // An agent's constraint command takes the drawing's constraint lease too (`constraintLease.ts`).
  // A drawing changed outside Kition opens with its stored replica whatever the file's stamp
  // (`replicaMergesFile`, Kit UI protocol UI-R1): the engine records what the file changed as one
  // `import` change onto the replica (`open_with_replica`), so the entities keep their object ids
  // and the history goes on (tests/replica.mjs).
  const session = useKitSession(host, manifest, describe, {
    agentLeaseScope: (command, params) => constraintLeaseScope(command, params),
    replicaMergesFile: true,
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
  const tools = useEngineTools(session.tool, doc != null, change, { initial: 'select', hostTools: HOST_TOOLS })
  const canvas = useRef<KitCanvasHandle>(null)
  const viewMemory = useRef<RasterView | null>(null)
  const [zoom, setZoom] = useState(1)
  const [tab, setTab] = useState('home')
  const [panel, setPanel] = useState(true)
  const commandInput = useRef<HTMLInputElement>(null)
  const [history, setHistory] = useState<string[]>([])
  useEffect(() => setHistory([]), [doc])

  const tabs: FrameTab[] = [
    { id: 'home', label: phrase('Home'), ribbon: state ? <HomeTab info={state} onFit={() => { void editor.run('zoom.extents', {}) }} onLayers={() => setPanel((open) => !open)} /> : null },
    ...MENU_TABS.map((menu) => ({ id: menu, label: phrase(menu), view: menu === 'View', ribbon: <CommandTab menu={menu} exclude={menu === 'File' ? HOST_FILE_COMMANDS : []} primary={CAD_PRIMARY[menu]} /> })),
  ]
  const activeTab = tabs.some((item) => item.id === tab) ? tab : 'home'

  return (
    <KitEditorProvider editor={editor} tools={tools}>
      <div className="cad-editor-root" onKeyDownCapture={(event) => {
        if (event.nativeEvent.isComposing || event.metaKey || event.ctrlKey || event.altKey || !editor.enabled('commandLine')) return
        if (!(event.target instanceof Element) || !event.target.matches('.kui-app, canvas, .kui-text-sink')) return
        // In Focus the command line is not drawn (it is in FrameChrome), so its input is unmounted
        // and this capture is off: a letter then picks its tool, as everywhere else.
        if (event.key.length !== 1 || event.key === ' ' || !commandInput.current) return
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
            if (event.key !== 'Enter' || event.metaKey || event.ctrlKey || event.altKey || !editor.enabled('commandLine')) return false
            if (!(event.target instanceof Element) || !event.target.matches('.kui-app, canvas, .kui-text-sink')) return false
            void editor.run('commandLine', { text: '' }).then(() => tools.refresh())
            return true
          }}
          status={
            state ? (
              <>
                <span data-testid="cad-status">{t('objects', { count: state.entities })}</span>
                <span>{t('layer', { name: state.currentLayer })}</span>
              </>
            ) : null
          }
          statusEnd={
            state ? (
              <span className="cad-drafting">
                {DRAFTING.map((mode) => (
                  <button
                    key={mode.id}
                    type="button"
                    className="cad-mode"
                    aria-pressed={state.drafting[mode.id]}
                    disabled={!editor.enabled(mode.id)}
                    onClick={() => void editor.run(mode.id, { on: !state.drafting[mode.id] })}
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
          <CadCanvas canvas={canvas} viewMemory={viewMemory} info={state} history={history} onZoom={setZoom} />
          {state ? (
            <FrameChrome>
              <CommandLine prompt={state.prompt} running={state.running} input={commandInput} history={history} onCommand={(text) => setHistory((items) => [...items.filter((item) => item !== text), text].slice(-100))} />
            </FrameChrome>
          ) : null}
        </EditorFrame>
      </div>
    </KitEditorProvider>
  )
}
