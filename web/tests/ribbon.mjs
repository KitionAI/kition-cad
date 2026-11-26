// The Classic ribbon keeps its labels when it runs out of room (walkthrough N18): at 1600, 1440, 1165
// and 1024 px no tab draws a button as an icon alone; at 1600 Home shows every group whole and
// labelled; where a group is short of room it shows its first commands labelled and a More button
// that opens the rest with their labels, and a command chosen there starts (Home's Annotation at
// 1280, the Draw tab's Draw group at 1165); a menu tab's later groups give way first, each as one
// labelled button that opens the group (the Draw tab's Arc at 1440), so Donut stays in view. Every
// command button draws an icon of its own: none draws kit-editor's neutral mark, and no two buttons
// of a group share one (Home and every menu tab, every group whole at 3000 px).
// node tests/ribbon.mjs [chromium|webkit]; prints PASS or FAIL per step, exits 1 on any FAIL.
import { mkdir } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { CommandGenericIcon } from '@kition/tessera/icons/react'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const output = new URL('../test-results/ribbon/', import.meta.url)
let failures = 0
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${which} ${name}${detail ? ` - ${detail}` : ''}`)
}
const TABS = ['home', 'File', 'Edit', 'Insert', 'Format', 'Tools', 'Draw', 'Dimension', 'Modify', 'View']

/** Opens the editor at `width`; answers the page, its editor frame and a reader of the drawing's state. */
const open = async (width) => {
  const page = await browser.newPage({ viewport: { width, height: 900 } })
  page.setDefaultTimeout(10_000)
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(`${origin}/dev.html?modes=classic&locale=en-US`)
  await page.waitForFunction(() => window.devHost?.connection(), null, { timeout: 60_000 })
  const kit = page.frameLocator('#kit')
  const details = async () => (await page.evaluate(async () => window.devHost.connection().ai.inspect())).details
  for (let i = 0; i < 100 && (await details().catch(() => null))?.entities !== 7; i += 1) await page.waitForTimeout(100)
  await kit.getByTestId('cad-tool-line.add').waitFor()
  await page.waitForTimeout(500)
  return { page, kit, details, errors }
}
/** The ribbon's buttons drawn as an icon alone: their labels. */
const iconsAlone = (kit) => kit.locator('.kui-ribbon-panel button.kui-ribbon-button').evaluateAll((buttons) => buttons.filter((button) => button.dataset.size === 'icon' || !button.textContent.trim()).map((button) => button.getAttribute('aria-label') ?? button.title ?? '?'))
const until = async (page, check, tries = 50) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}

await mkdir(output, { recursive: true })
try {
  // 1. No tab draws a button as an icon alone, at the widths people use.
  for (const width of [1600, 1440, 1165, 1024]) {
    const { page, kit, errors } = await open(width)
    const alone = []
    for (const tab of TABS) {
      await kit.getByTestId(`ribbon-tab-${tab}`).click()
      await page.waitForTimeout(400)
      for (const label of await iconsAlone(kit)) alone.push(`${tab}: ${label}`)
      if (width === 1440 && (tab === 'home' || tab === 'Draw')) await page.screenshot({ path: new URL(`${which}-${tab}-${width}.png`, output).pathname, clip: { x: 0, y: 0, width, height: 220 } })
    }
    step(`at ${width} px every ribbon button has its label`, alone.length === 0, alone.slice(0, 12).join(' | '))
    step(`at ${width} px the editor stays Classic`, (await kit.locator('.kui-app').getAttribute('data-mode')) === 'classic')
    step(`at ${width} px no page errors`, errors.length === 0, errors.slice(0, 2).join(' | '))
    await page.close()
  }

  // 2. At 1600 Home shows every group whole: nothing waits under More (at 1440 it may already give
  //    way, with labels, by the fonts the system has).
  {
    const { page, kit } = await open(1600)
    step('at 1600 px Home has no More', (await kit.locator('[data-testid^=cad-ribbon-more-]').count()) === 0)
    for (const id of ['cad-tool-dimension.addRadius', 'cad-tool-text.add', 'cad-layer', 'cad-layer-properties', 'cad-zoom-extents', 'cad-cmd-object.stretch']) {
      step(`at 1600 px Home shows ${id}`, (await kit.getByTestId(id).count()) === 1 && (await kit.getByTestId(id).isVisible()))
    }
    await page.close()
  }

  // 3. Short of room, Annotation keeps Multiline Text and Linear labelled and the rest under More.
  {
    const { page, kit, details } = await open(1280)
    const more = kit.getByTestId('cad-ribbon-more-annotation')
    step('at 1280 px Annotation has More', (await more.count()) === 1 && (await more.textContent()).includes('More'))
    step('and keeps Multiline Text and Linear labelled', (await kit.getByTestId('cad-tool-multilineText.add').textContent()).includes('Multiline Text') && (await kit.getByTestId('cad-tool-dimension.addLinear').textContent()).includes('Linear'))
    await more.click()
    const panel = kit.getByTestId('cad-ribbon-more-panel-annotation')
    await panel.waitFor()
    await page.screenshot({ path: new URL(`${which}-annotation-more.png`, output).pathname, clip: { x: 0, y: 0, width: 1280, height: 320 } })
    const labels = await panel.locator('button').evaluateAll((buttons) => buttons.map((button) => button.textContent.trim()))
    step('More opens the rest with their labels', ['Single Line Text', 'Aligned', 'Radius'].every((label) => labels.includes(label)), labels.join(', '))
    await panel.getByTestId('cad-tool-dimension.addAligned').click()
    const started = await until(page, async () => (await details()).running === 'dimension.addAligned')
    step('a command chosen under More starts', started, String((await details()).running))
    step('and More closes', await until(page, async () => (await kit.getByTestId('cad-ribbon-more-panel-annotation').count()) === 0))
    await page.close()
  }

  // 4. The Draw tab at 1440: the Draw group whole (Donut in view), the groups after it each one
  // labelled button that opens the group with its labels.
  {
    const { page, kit } = await open(1440)
    await kit.getByTestId('ribbon-tab-Draw').click()
    await page.waitForTimeout(400)
    step('at 1440 px the Draw tab shows Donut', await kit.getByTestId('cad-cmd-donut.add').isVisible())
    const arc = kit.getByTestId('kui-ribbon-collapsed-menu:Draw/Arc')
    step('the Arc group is one button labelled Arc', (await arc.count()) === 1 && (await arc.textContent()).includes('Arc'))
    await arc.click()
    const panel = kit.getByTestId('kui-ribbon-collapsed-panel-menu:Draw/Arc')
    await panel.waitFor()
    const labels = await panel.locator('button').evaluateAll((buttons) => buttons.map((button) => button.textContent.trim()))
    step('it opens the arcs with their labels', labels.includes('Start, Center, End') && labels.includes('Center, Start, Angle'), labels.join(', '))
    await page.close()
  }

  // 5. The Draw tab at 1165 (Kition's editor in a 1440 px window): the Draw group's first commands,
  // then More with the rest; Donut chosen there starts its prompts.
  {
    const { page, kit, details } = await open(1165)
    await kit.getByTestId('ribbon-tab-Draw').click()
    await page.waitForTimeout(400)
    const more = kit.getByTestId('cad-ribbon-more-Draw')
    step('at 1165 px the Draw tab\'s Draw group has More', (await more.count()) === 1)
    await more.click()
    const panel = kit.getByTestId('cad-ribbon-more-panel-Draw')
    await panel.waitFor()
    await page.screenshot({ path: new URL(`${which}-draw-more.png`, output).pathname, clip: { x: 0, y: 0, width: 1165, height: 360 } })
    step('its panel lists Donut with its label', (await panel.getByTestId('cad-cmd-donut.add').textContent()).includes('Donut'))
    await panel.getByTestId('cad-cmd-donut.add').click()
    step('Donut under More starts its prompts', await until(page, async () => (await details()).running === 'donut.add'), String((await details()).running))
    await page.close()
  }

  // 6. Every command its own icon: no neutral mark, no icon shared within a group.
  {
    const neutral = [...renderToStaticMarkup(createElement(CommandGenericIcon)).matchAll(/ d="([^"]+)"/g)].map((match) => match[1]).join('|')
    const { page, kit } = await open(3000)
    const shared = []
    const marks = []
    let buttons = 0
    for (const tab of TABS) {
      await kit.getByTestId(`ribbon-tab-${tab}`).click()
      await page.waitForTimeout(400)
      const groups = await kit.locator('.kui-ribbon-panel [data-ribbon-group]').evaluateAll((elements) => elements.map((group) => ({
        group: group.dataset.ribbonGroup,
        buttons: [...group.querySelectorAll('button.kui-ribbon-button[data-testid^="cad-"]')].map((button) => {
          const svg = button.querySelector('svg')
          return { label: button.getAttribute('aria-label'), icon: svg ? `${svg.getAttribute('style') ?? ''}${[...svg.querySelectorAll('path, circle, line, rect, polyline, polygon, ellipse')].map((shape) => shape.getAttribute('d') ?? shape.outerHTML).join('|')}` : '' }
        }),
      })))
      for (const { group, buttons: list } of groups) {
        const seen = new Map()
        for (const { label, icon } of list) {
          buttons += 1
          if (!icon || icon === neutral) marks.push(`${tab} ${group}: ${label}`)
          if (seen.has(icon)) shared.push(`${tab} ${group}: ${seen.get(icon)} = ${label}`)
          else seen.set(icon, label)
        }
      }
    }
    step('the ribbon\'s command buttons are read', buttons > 220, `${buttons} buttons`)
    step('no command draws the neutral mark', marks.length === 0, marks.slice(0, 12).join(' | '))
    step('no two commands of a group share an icon', shared.length === 0, shared.slice(0, 12).join(' | '))
    await kit.getByTestId('ribbon-tab-Draw').click()
    await page.waitForTimeout(400)
    await page.screenshot({ path: new URL(`${which}-draw-icons.png`, output).pathname, clip: { x: 0, y: 0, width: 3000, height: 200 } })
    await page.close()
  }
} catch (error) {
  step('threw', false, error instanceof Error ? error.message.split('\n')[0] : String(error))
} finally {
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
