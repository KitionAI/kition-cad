// Makes public/sample/Sample.dxf with the engine itself: a small floor plan on its own layers,
// with a label and a dimension.
import fs from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = fileURLToPath(new URL('..', import.meta.url))
const engine = await import(pathToFileURL(`${web}public/engine/kition_cad_kit.js`).href)
await engine.default({ module_or_path: await fs.readFile(`${web}public/engine/kition_cad_kit_bg.wasm`) })
const doc = engine.CadDocument.create('Sample.dxf', 420, 297)
const run = (command, params = {}) => doc.execute(command, JSON.stringify(params))
run('layer.add', { name: 'Walls', color: 'blue', current: true })
run('polyline.add', { vertices: [[40, 40], [300, 40], [300, 220], [40, 220]], closed: true, width: 2 })
run('line.add', { points: [[170, 40], [170, 150]] })
run('line.add', { points: [[170, 190], [170, 220]] })
run('layer.add', { name: 'Furniture', color: 'green', current: true })
run('rectangle.add', { p1: [60, 60], p2: [140, 110] })
run('circle.add', { center: [235, 130], radius: 30 })
run('layer.add', { name: 'Annotation', color: 'red', current: true })
run('text.add', { at: [60, 180], height: 8, text: 'Living room' })
run('dimension.addLinear', { p1: [40, 40], p2: [300, 40], at: [170, 20] })
run('layer.setCurrent', { name: 'Walls' })
await fs.writeFile(`${web}public/sample/Sample.dxf`, doc.save())
console.log('wrote public/sample/Sample.dxf')
