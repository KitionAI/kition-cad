import { useLayoutEffect, useSyncExternalStore, type ReactNode } from 'react'
import { ChevronDownIcon as ChevronDown, LockIcon as Lock } from '@kition/tessera/icons/react'
import { useKitEditor } from '@kition/sdk/editor'
import type { DynamicInput } from './dynamicInput'

/** One value box: what is typed (full colour), or the cursor's live value (dimmed). */
function Box({ typed, live, active, locked, testId }: { typed: string; live: string; active: boolean; locked: boolean; testId: string }) {
  return (
    <span className="cad-dynamic-box" data-active={active ? 'true' : undefined} data-locked={locked ? 'true' : undefined} data-typed={typed ? 'true' : undefined} data-testid={testId}>
      {typed ? <span className="cad-dynamic-typed">{typed}</span> : <span className="cad-dynamic-live" data-live={live} />}
      {active ? <span className="cad-dynamic-caret" aria-hidden="true" /> : null}
      {locked ? <Lock size={10} className="cad-dynamic-lock" aria-hidden="true" data-testid={`${testId}-lock`} /> : null}
    </span>
  )
}

/**
 * The dynamic input tooltip beside the cursor (F12): the prompt and, at a point prompt, two boxes
 * (X and Y at a first point, `@` distance and `<` angle after it), one box for a value prompt, or
 * what is typed; a ▾ when the prompt has options, which Down Arrow lists. It shows what the command
 * line holds (`dynamicInput.ts`); the canvas places it and writes the live values as it draws.
 */
export function DynamicTooltip({ dynamic }: { dynamic: DynamicInput }) {
  const { phrase } = useKitEditor()
  useSyncExternalStore(dynamic.subscribe, dynamic.snapshot)
  // Its size changes with what it shows: put it where it fits again.
  useLayoutEffect(() => dynamic.paint())
  const layout = dynamic.layout()
  const prompt = dynamic.prompt
  // Typed in the command line itself, the text is the command line's alone.
  if (!layout || !prompt || !dynamic.shows()) return <div className="cad-dynamic" ref={(element) => { dynamic.box = element }} style={{ visibility: 'hidden' }} data-testid="cad-dynamic-input" hidden />
  const options = dynamic.options().length > 0
  const message = prompt.message ? phrase(prompt.message) : ''
  let body: ReactNode
  if (layout === 'pair') {
    const relative = dynamic.relative()
    const entry = dynamic.entry() ?? { format: relative ? 'polar' : 'cartesian', first: '', split: false, second: '' }
    const polar = entry.format === 'polar'
    const firstActive = !entry.split || dynamic.firstActiveNow()
    const kind = polar ? 'polar' : 'cartesian'
    body = (
      <>
        {relative ? <span className="cad-dynamic-label">@</span> : null}
        {polar ? null : <span className="cad-dynamic-label">X</span>}
        <Box typed={entry.first} live={`${kind}.0`} active={firstActive} locked={entry.split && entry.first !== ''} testId="cad-dynamic-first" />
        <span className="cad-dynamic-label">{polar ? '<' : 'Y'}</span>
        <Box typed={entry.second} live={`${kind}.1`} active={!firstActive} locked={dynamic.secondLockedNow()} testId="cad-dynamic-second" />
      </>
    )
  } else if (layout === 'single') {
    const typed = dynamic.dynamic ? dynamic.text : ''
    const live = prompt.base ? (prompt.kind === 'angle' ? 'polar.1' : 'polar.0') : 'none.0'
    body = <Box typed={typed} live={live} active locked={false} testId="cad-dynamic-value" />
  } else {
    body = dynamic.dynamic && dynamic.text ? <span className="cad-dynamic-typed" data-testid="cad-dynamic-text">{dynamic.text}</span> : null
  }
  const menu = dynamic.menu
  return (
    <div className="cad-dynamic" ref={(element) => { dynamic.box = element }} style={{ visibility: 'hidden' }} data-layout={layout} data-testid="cad-dynamic-input">
      <div className="cad-dynamic-row">
        {message ? <span className="cad-dynamic-message" data-testid="cad-dynamic-message">{message}</span> : null}
        {body}
        {options ? <ChevronDown size={12} className="cad-dynamic-more" aria-hidden="true" data-testid="cad-dynamic-more" /> : null}
      </div>
      {menu ? (
        <ul className="cad-dynamic-options" role="listbox" aria-label={phrase('Options')} data-testid="cad-dynamic-options">
          {menu.keywords.map((keyword, index) => (
            <li key={keyword} role="option" aria-selected={index === menu.index}>
              <button
                type="button"
                className="cad-dynamic-option"
                data-current={index === menu.index ? 'true' : undefined}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => dynamic.choose(keyword)}
                data-testid={`cad-dynamic-option-${keyword}`}
              >
                {keyword}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  )
}
