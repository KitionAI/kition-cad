import { useEffect, useState } from 'react'
import { Button, Dialog, useKitEditor, useKitTools } from '@kition/sdk/editor'
import { describe, gripping } from './cad'

/** A block the drawing defines, as `block.list` answers it. */
type BlockEntry = { name: string; entities: number; description?: string }

/** What the insert dialog was opened with: the block chosen last, the scale and rotation used last. */
export type InsertChoice = { name: string; scale: string; rotation: string }

/**
 * The Insert dialog (stage 9): the drawing's blocks, a uniform scale, a rotation, and where the
 * block goes. With "Specify on-screen" (the default) OK starts INSERT with the block, the scale
 * and the rotation answered at its prompts (the history shows them), so the person clicks the
 * insertion point next with the block's ghost following the cursor (INSERT's own preview). With
 * an X and a Y typed instead, OK runs `block.insert` there, one undo step. Typed INSERT and
 * Home › Insert open it; the command line itself never starts INSERT (it may read a file).
 */
export function InsertDialog({ last, onClose, onChosen }: { last: InsertChoice | null; onClose: () => void; onChosen: (choice: InsertChoice) => void }) {
  const { phrase, query, run, session } = useKitEditor()
  const tools = useKitTools()
  const [blocks, setBlocks] = useState<BlockEntry[] | null>(null)
  const [name, setName] = useState(last?.name ?? '')
  const [onScreen, setOnScreen] = useState(true)
  const [x, setX] = useState('0')
  const [y, setY] = useState('0')
  const [scale, setScale] = useState(last?.scale ?? '1')
  const [rotation, setRotation] = useState(last?.rotation ?? '0')
  useEffect(() => {
    let gone = false
    void query<{ blocks?: BlockEntry[] }>('block.list', {}).then((answer) => {
      if (gone) return
      const list = (answer?.blocks ?? []).filter((block) => block.name && !block.name.startsWith('*')).sort((a, b) => a.name.localeCompare(b.name))
      setBlocks(list)
      setName((chosen) => (list.some((block) => block.name === chosen) ? chosen : list[0]?.name ?? ''))
    })
    return () => { gone = true }
  }, [query])
  const number = (text: string) => (text.trim() === '' ? NaN : Number(text))
  const scaleValue = number(scale)
  const rotationValue = number(rotation)
  const at = [number(x), number(y)]
  const valid = name !== '' && Number.isFinite(scaleValue) && scaleValue !== 0 && Number.isFinite(rotationValue) && (onScreen || at.every(Number.isFinite))
  const ok = () => {
    if (!valid) return
    onClose()
    onChosen({ name, scale: String(scaleValue), rotation: String(rotationValue) })
    if (!onScreen) {
      void run('block.insert', { name, at, scale: scaleValue, rotation: rotationValue }).then(() => tools?.refresh())
      return
    }
    // A command running (or a hot grip) is cancelled first, as a menu does; then INSERT starts as
    // the engine's tool and its prompts are answered: the block, then Scale and Rotate before the
    // insertion point, which the person clicks.
    const now = session.doc ? describe(session.doc.info) : null
    if (now && (now.running != null || gripping(now))) tools?.keyDown({ key: 'Escape', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false })
    tools?.select('block.insert')
    const entries = [name, 's', String(scaleValue), 'r', String(rotationValue)]
    void entries.reduce<Promise<unknown>>((done, text) => done.then(() => run('command.runLine', { text })), Promise.resolve()).then(() => tools?.refresh())
  }
  const enter = (event: React.KeyboardEvent) => { if (event.key === 'Enter') { event.preventDefault(); ok() } }
  return (
    <Dialog
      title={phrase('Insert')}
      onClose={onClose}
      testId="cad-insert-dialog"
      actions={<>
        <Button variant="secondary" onClick={onClose}>{phrase('Cancel')}</Button>
        <Button variant="primary" disabled={!valid} onClick={ok} data-testid="cad-insert-ok">{phrase('OK')}</Button>
      </>}
    >
      <div className="cad-dialog-fields">
        <label className="cad-property"><span>{phrase('Name')}</span>
          <select className="kui-field" value={name} disabled={!blocks?.length} onChange={(event) => setName(event.target.value)} data-testid="cad-insert-name">
            {blocks?.map((block) => <option key={block.name} value={block.name}>{block.name}</option>)}
          </select>
        </label>
        {blocks && !blocks.length ? <div className="kui-muted cad-dialog-note" data-testid="cad-insert-none">{phrase('This drawing has no blocks yet: make one with Block first.')}</div> : null}
        <div className="cad-property-group-title">{phrase('Insertion point')}</div>
        <label className="cad-check">
          <input type="checkbox" checked={onScreen} onChange={(event) => setOnScreen(event.target.checked)} data-testid="cad-insert-on-screen" />
          <span>{phrase('Specify On-screen')}</span>
        </label>
        <label className="cad-property"><span>X</span>
          <input className="kui-field" type="text" inputMode="decimal" value={x} disabled={onScreen} onChange={(event) => setX(event.target.value)} onKeyDown={enter} data-testid="cad-insert-x" />
        </label>
        <label className="cad-property"><span>Y</span>
          <input className="kui-field" type="text" inputMode="decimal" value={y} disabled={onScreen} onChange={(event) => setY(event.target.value)} onKeyDown={enter} data-testid="cad-insert-y" />
        </label>
        <label className="cad-property"><span>{phrase('Scale')}</span>
          <input className="kui-field" type="text" inputMode="decimal" value={scale} onChange={(event) => setScale(event.target.value)} onKeyDown={enter} data-testid="cad-insert-scale" />
        </label>
        <label className="cad-property"><span>{phrase('Rotation')}</span>
          <input className="kui-field" type="text" inputMode="decimal" value={rotation} onChange={(event) => setRotation(event.target.value)} onKeyDown={enter} data-testid="cad-insert-rotation" />
        </label>
      </div>
    </Dialog>
  )
}
