import { useEffect, useRef, useState, type RefObject } from 'react'
import { ContextMenu, KitDocumentCanvas, MenuItem, MenuSeparator, useKitEditor, useKitTools, type KitCanvasHandle, type KitCanvasView, type RasterView } from '@kition/sdk/editor'
import type { CadInfo } from './cad'

/** CAD keeps its model view and the raster viewport in agreement without changing other kits. */
export function CadCanvas({ canvas, viewMemory, info, history, onZoom }: { canvas: RefObject<KitCanvasHandle | null>; viewMemory: RefObject<RasterView | null>; info: CadInfo | null; history: string[]; onZoom: (zoom: number) => void }) {
  const editor = useKitEditor()
  const tools = useKitTools()
  const region = useRef<HTMLDivElement>(null)
  const lastView = useRef('')
  const applied = useRef(0)
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null)
  const { doc } = editor.session
  useEffect(() => { applied.current = 0; lastView.current = '' }, [doc])

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
    const wheel = (event: WheelEvent) => {
      if (!(event.target instanceof HTMLCanvasElement) || event.ctrlKey || event.metaKey) return
      const viewport = canvas.current
      const view = viewport?.view()
      if (!viewport || !view) return
      event.preventDefault()
      event.stopPropagation()
      const bounds = event.target.getBoundingClientRect()
      const x = event.clientX - bounds.left
      const y = event.clientY - bounds.top
      const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? bounds.height : 1)
      viewport.zoomTo(view.zoom * Math.exp(-Math.max(-500, Math.min(500, delta)) * 0.002))
      const next = viewport.view()
      if (next) viewport.panTo(view.x + x / view.zoom - x / next.zoom, view.y + y / view.zoom - y / next.zoom)
    }
    element.addEventListener('wheel', wheel, { capture: true, passive: false })
    return () => element.removeEventListener('wheel', wheel, true)
  }, [canvas])

  const command = (id: string, label: string) => <MenuItem key={id} label={editor.phrase(label)} disabled={!editor.enabled(id)} onSelect={() => { void editor.run(id, {}) }} testId={`cad-context-${id}`} />
  return (
    <div ref={region} className="kui-row" onContextMenu={(event) => {
      if (!(event.target instanceof HTMLCanvasElement)) return
      event.preventDefault()
      setMenu({ x: event.clientX, y: event.clientY })
    }}>
      <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} framed={false} onView={synchronize} />
      <ContextMenu at={menu} onClose={() => setMenu(null)} label={editor.phrase('Drawing')} testId="cad-context-menu">
        <MenuItem label={editor.phrase('Repeat last command')} disabled={!editor.enabled('command.runLine')} onSelect={() => { void editor.run('command.runLine', { text: '' }).then(() => tools?.refresh()) }} testId="cad-context-repeat" />
        {history.slice(-5).reverse().map((text) => <MenuItem key={text} label={text} disabled={!editor.enabled('command.runLine')} onSelect={() => { void editor.run('command.runLine', { text }).then(() => tools?.refresh()) }} />)}
        <MenuSeparator />
        {command('edit.undo', 'Undo')}{command('edit.redo', 'Redo')}
        {command('edit.delete', 'Erase')}{command('edit.selectAll', 'Select All')}
        <MenuSeparator />
        <MenuItem label={editor.phrase('Pan')} onSelect={() => tools?.select('hand')} />
        {command('view.zoomToFit', 'Zoom Extents')}{command('view.zoomIn', 'Zoom In')}{command('view.zoomOut', 'Zoom Out')}
      </ContextMenu>
    </div>
  )
}
