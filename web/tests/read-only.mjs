// A viewer's (read-only) session refuses CAD's history under its command ids (edit.undoMany,
// edit.undoStep, edit.redoStep; were undo, u and redo), as the editor's edit.undo and edit.redo,
// even where a kit flags them as not editing; it runs a read the kit flags as not editing
// (file.inspect). Checks the editor package's rule (@kition/sdk/editor `editsDocument`) against the
// ids this kit's engine declares (`bindings/wasm/agent-commands.json`). Runs in Node.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { editsDocument } from '@kition/sdk/editor'

const declared = JSON.parse(readFileSync(new URL('../../bindings/wasm/agent-commands.json', import.meta.url), 'utf8'))
for (const id of ['edit.undoMany', 'edit.undoStep', 'edit.redoStep']) {
  assert.ok(id in declared, `the engine declares ${id}`)
  assert.equal(editsDocument({ id, mutates: false }), true, `${id} is refused for a viewer`)
}
for (const id of ['edit.undo', 'edit.redo']) {
  assert.equal(editsDocument({ id, mutates: false }), true, `${id} is refused for a viewer`)
}
assert.ok('file.inspect' in declared, 'the engine declares file.inspect')
assert.equal(editsDocument({ id: 'file.inspect', mutates: false }), false, 'a viewer reads the drawing')
assert.equal(editsDocument({ id: 'object.move', mutates: true }), true, 'a viewer does not move objects')
console.log('read-only: CAD history refused for viewers, reads run')
