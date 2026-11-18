// Takes the README's pictures of the CAD editor (assets/readme/*.png at the repository root): the
// editor in the development host, on two showcase samples, "House Lighting Circuit Diagram" (the
// hero, blocks, layers) and "Open Office Desk Layout" (dimensions, copy), at 1440 x 900, device
// scale 2, Classic display mode, light theme, English. Not a test; re-run it by hand when the
// editor changes:
//
//   cd web && pnpm install && pnpm run engine      # the engine in public/
//   node scripts/readme-shots.mjs [name ...]       # every shot, or only the named ones
//
// It needs kition-sdk (the shared runner, tools/readme-shots/) and kition-showcase (the samples)
// checked out beside this repository, or KITION_SDK and KITION_SHOWCASE naming them. Run it with
// no HTTP proxy set; CHROME may name a Chrome or Chromium executable. Every step is what a person
// does in the editor: clicks and drags on the drawing, the wheel, the layers panel and commands
// typed at the command line. When pngquant is installed, each picture is then shrunk with
// `pngquant --quality 70-90` (a README picture is at most 400 KB).
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(web, '..', '..', 'kition-sdk'))
const showcase = path.resolve(process.env.KITION_SHOWCASE ?? path.join(web, '..', '..', 'kition-showcase'))
const { openFile, takeShots } = await import(pathToFileURL(path.join(sdk, 'tools/readme-shots/shots.mjs')).href)

const sample = (name) => path.join(showcase, 'cad', name, `${name}.dxf`)
const SCHEMATIC = sample('house-lighting-circuit-schematic')
const OFFICE = sample('open-office-desk-layout')
/** The shots that open the office layout; the others open the circuit diagram. */
const ON_OFFICE = new Set(['see-3', 'see-4'])

/**
 * Where a point of the drawing is on the page once it is fitted to the window (zoom extents),
 * measured on the pictures: the corners of each sample's frame, in the kit's frame, in CSS pixels.
 */
const FITTED = {
  // The schematic's sheet border, (10, 10) to (287, 205).
  schematic: { world: [10, 205, 287, 10], frame: [157, 151, 1037, 770] },
  // The office's outer wall, (-250, 9250) to (14250, -250).
  office: { world: [-250, 9250, 14250, -250], frame: [205, 217, 963, 713] },
}
async function toPage(page, which, x, y) {
  const { world: [x0, y0, x1, y1], frame: [left, top, right, bottom] } = FITTED[which]
  const box = await page.locator('#kit').boundingBox()
  return { x: box.x + left + ((x - x0) / (x1 - x0)) * (right - left), y: box.y + top + ((y - y0) / (y1 - y0)) * (bottom - top) }
}

async function fit(page, kit) {
  await kit.getByTestId('cad-zoom-extents').click()
  await page.waitForTimeout(500)
}

/** Types a line at the command line and presses Enter. */
async function type(page, kit, text) {
  const line = kit.getByTestId('cad-command-line')
  await line.fill(text)
  await line.press('Enter')
  await page.waitForTimeout(300)
}

/** A window selection: a drag from the top left to the bottom right corner (page pixels). */
async function window(page, from, to) {
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  await page.mouse.move((from.x + to.x) / 2, (from.y + to.y) / 2, { steps: 5 })
  await page.mouse.move(to.x, to.y, { steps: 5 })
  await page.mouse.up()
  await page.waitForTimeout(500)
}

/** Moves the pointer onto `to` in a few steps, so the drawing previews the command there. */
async function hover(page, to) {
  await page.mouse.move(to.x - 30, to.y)
  await page.mouse.move(to.x, to.y, { steps: 6 })
}

const report = await takeShots({
  web,
  sample: async (page, shot) => {
    const file = ON_OFFICE.has(shot.name) ? OFFICE : SCHEMATIC
    await openFile(page, { name: path.basename(file), bytes: fs.readFileSync(file) })
  },
  query: { modes: 'classic' },
  ready: '[data-testid="cad-status"]',
  settle: { open: 3000, shot: 1000 },
  shots: [
    {
      // The diagram as it opens, fitted to the window: the ribbon, the layers and the command line.
      name: 'hero',
      steps: async (page, { kit }) => fit(page, kit),
    },
    {
      // Closer in with the wheel; the three luminaires of circuit C1 window-selected: each is the
      // block LAMP with its ID attribute, and the Properties panel names them.
      name: 'see-1',
      steps: async (page, { kit }) => {
        await fit(page, kit)
        const centre = await toPage(page, 'schematic', 150, 150)
        await page.mouse.move(centre.x, centre.y)
        for (let notch = 0; notch < 2; notch += 1) {
          await page.mouse.wheel(0, -120)
          await page.waitForTimeout(300)
        }
        // Two notches scale the view by exp(0.24) each about the pointer.
        const scale = Math.exp(0.48)
        const zoomed = async (x, y) => {
          const at = await toPage(page, 'schematic', x, y)
          return { x: centre.x + (at.x - centre.x) * scale, y: centre.y + (at.y - centre.y) * scale }
        }
        await window(page, await zoomed(140, 165), await zoomed(203, 145))
      },
    },
    {
      // Wires made current, Text frozen (the labels leave the view) and Frame locked.
      name: 'see-2',
      steps: async (page, { kit }) => {
        await fit(page, kit)
        await kit.getByTestId('cad-layer-Wires').locator('.cad-layer-name').click()
        await page.waitForTimeout(300)
        await kit.getByTestId('cad-layer-Text').getByRole('button', { name: /freeze/i }).click()
        await kit.getByTestId('cad-layer-Frame-lock').click()
        // The pointer leaves the panel, so no button shows its hover; the layers are as clicked.
        await page.mouse.move(700, 600)
        await page.waitForFunction(async () => {
          const { layers } = (await window.devHost.connection().ai.inspect()).details
          return layers.some((layer) => layer.name === 'Text' && layer.frozen) && layers.some((layer) => layer.name === 'Frame' && layer.locked)
        }, null, { timeout: 10_000 })
      },
    },
    {
      // A linear dimension typed at the command line: dimlinear and the meeting room's two corners;
      // the pointer places the dimension line, and the prompt offers the command's options.
      name: 'see-3',
      steps: async (page, { kit }) => {
        await fit(page, kit)
        await type(page, kit, 'dimlinear')
        await type(page, kit, '9500,9000')
        await type(page, kit, '9500,4800')
        await hover(page, await toPage(page, 'office', 8900, 6900))
      },
    },
    {
      // One workstation window-selected and copied: the base point typed, the copy following the pointer.
      name: 'see-4',
      steps: async (page, { kit }) => {
        await fit(page, kit)
        await window(page, await toPage(page, 'office', 1100, 2300), await toPage(page, 'office', 2900, 800))
        await type(page, kit, 'copy')
        await type(page, kit, '1200,1400')
        await hover(page, await toPage(page, 'office', 8200, 1400))
      },
    },
  ],
})

// Shrinks the pictures when pngquant is there (it keeps a file it cannot make smaller).
const repo = path.resolve(web, '..')
for (const { file } of report) {
  try {
    execFileSync('pngquant', ['--quality', '70-90', '--force', '--skip-if-larger', '--output', file, file], { cwd: repo, stdio: 'ignore' })
  } catch (error) {
    if (error.code === 'ENOENT') break
  }
  console.log(`${file}  ${Math.ceil(fs.statSync(path.join(repo, file)).size / 1024)} KB`)
}
