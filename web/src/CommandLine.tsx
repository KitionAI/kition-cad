import { useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from 'react'
import { ChevronDownIcon as ChevronDown, ChevronUpIcon as ChevronUp, TerminalIcon as Terminal } from '@kition/tessera/icons/react'
import { createPortal } from 'react-dom'
import { ContextMenu, MenuItem, useKitEditor, useKitTools } from '@kition/sdk/editor'
import type { CommandLineState, PromptKind } from './cad'
import { namesExactly, suggest, useCatalog, type Suggestion } from './commands'
import type { DynamicInput } from './dynamicInput'

/** The history lines shown above the command line by default, and the most it can be dragged to. */
export const HISTORY_ROWS = 3
export const HISTORY_MAX_ROWS = 12
/** The height of one history line (px), as `app.css` draws it. */
const ROW_PX = 17
/** How long the command line shakes after an entry was refused (ms). */
const SHAKE_MS = 400

/** What the input's placeholder says the command line waits for, by the prompt's kind. */
const WAITING: Record<PromptKind, string> = {
  command: 'commandLine',
  point: 'waitPoint',
  distance: 'waitDistance',
  angle: 'waitAngle',
  select: 'waitSelect',
  text: 'waitText',
  keyword: 'waitKeyword',
}

/**
 * Puts `text` on the clipboard: the asynchronous clipboard where the page may use it, else the
 * older copy command on a hidden text area (an embedded editor may not be allowed the former).
 */
async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
    return
  } catch {
    // Not allowed here: copy the old way.
  }
  const area = document.createElement('textarea')
  area.value = text
  area.setAttribute('readonly', '')
  area.style.position = 'fixed'
  area.style.opacity = '0'
  document.body.appendChild(area)
  area.select()
  document.execCommand('copy')
  area.remove()
}

/** The names INSERT is typed by: they open the insert dialog (the command line never starts INSERT, which may read a file). */
const INSERT_NAMES = new Set(['insert', 'i', '-insert', 'ddinsert', '_insert', '.insert'])

/** A line of the history: the engine's, or an error of an entry the engine refused outright. */
type Line = { text: string; error: boolean; key: string }

/**
 * Kition CAD's command line: the input, the prompt above it with its options and default as
 * buttons, the history of what commands wrote (the last lines, or the whole of it with F2), and the
 * commands a typed name may mean. What the person types runs as a command-line entry.
 */
export function CommandLine({ line, running, input, history, onCommand, expanded, onExpanded, dynamic, onInsert }: {
  prompt: string
  line: CommandLineState
  running: string | null
  input: RefObject<HTMLInputElement | null>
  history: string[]
  onCommand: (text: string) => void
  expanded: boolean
  onExpanded: (open: boolean) => void
  /** The dynamic input boxes beside the cursor, a view of what is typed here (`dynamicInput.ts`). */
  dynamic: DynamicInput
  /** Opens the insert dialog: typed INSERT between commands. */
  onInsert?: () => void
}) {
  const { session, run, enabled, t, phrase, commands } = useKitEditor()
  // What is typed runs as a command that may edit: a viewer cannot type here.
  const disabled = !enabled('command.runLine')
  const tools = useKitTools()
  const catalog = useCatalog()
  const [historyIndex, setHistoryIndex] = useState<number | null>(null)
  const draft = useRef('')
  const [typed, setTyped] = useState('')
  const [rows, setRows] = useState(HISTORY_ROWS)
  const [shake, setShake] = useState(0)
  // Errors of entries the engine refused outright (an unknown command), after the history line
  // they followed: the engine's history has only what commands wrote.
  const [errors, setErrors] = useState<Array<{ after: number; text: string }>>([])
  const serial = useRef(line.history.serial)
  serial.current = line.history.serial
  // Entries sent and not answered yet: until they are, what the prompt is is not known, so a
  // typed word is sent as it is rather than completed.
  const pending = useRef(0)
  const log = useRef<HTMLDivElement>(null)
  // The history window's menu (a right-click on it): Copy, Copy History.
  const [menu, setMenu] = useState<{ x: number; y: number; selected: string } | null>(null)
  const gripping = line.prompt.grip != null
  const idle = !running && !gripping
  const suggestions = useMemo(() => (idle && historyIndex === null ? suggest(catalog, typed) : []), [catalog, typed, idle, historyIndex])

  const lines = useMemo(() => {
    const out: Line[] = []
    const first = line.history.serial - line.history.lines.length
    const pendingErrors = errors.filter((error) => error.after <= first)
    for (const error of pendingErrors) out.push({ text: error.text, error: true, key: `e${error.after}-${out.length}` })
    line.history.lines.forEach((text, index) => {
      const at = first + index + 1
      out.push({ text, error: false, key: `l${at}` })
      for (const error of errors) if (error.after === at) out.push({ text: error.text, error: true, key: `e${at}-${out.length}` })
    })
    return out
  }, [line.history, errors])
  useEffect(() => { if (expanded && log.current) log.current.scrollTop = log.current.scrollHeight }, [expanded, lines])
  useEffect(() => setErrors([]), [session.doc])
  useEffect(() => {
    if (!shake) return
    const timer = window.setTimeout(() => setShake(0), SHAKE_MS)
    return () => window.clearTimeout(timer)
  }, [shake])

  const clear = () => {
    if (input.current) input.current.value = ''
    setTyped('')
    setHistoryIndex(null)
    dynamic.reset()
  }
  /** Puts `text` on the line, the caret at its end: a key of the dynamic input boxes or a recalled entry. */
  const replace = (element: HTMLInputElement, text: string, origin: 'boxes' | 'line') => {
    element.value = text
    setTyped(text)
    dynamic.setText(text, origin)
    element.setSelectionRange(text.length, text.length)
  }
  /** Sends an entry to the command line; an entry the engine refuses goes on the history. */
  const send = (text: string) => {
    if (!running && !gripping && text.trim()) onCommand(text.trim())
    pending.current += 1
    void session.run('command.runLine', { text })
      .then((answer) => {
        const result = typeof answer === 'string' ? JSON.parse(answer) as { rejected?: boolean } : null
        if (result?.rejected) setShake((value) => value + 1)
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : String(error)
        setErrors((items) => [...items, { after: serial.current, text: message }].slice(-50))
        setShake((value) => value + 1)
      })
      .finally(() => {
        pending.current -= 1
        tools?.refresh()
      })
  }
  /** Runs a suggested command. */
  const choose = (suggestion: Suggestion) => {
    clear()
    send(suggestion.entry.name)
    input.current?.focus()
  }
  /** Answers the prompt with an option (as typed) or Enter (its default). */
  const answer = (text: string) => {
    clear()
    send(text)
  }
  // An option picked from the dynamic input's list answers the prompt as a click on it here does.
  dynamic.pick = answer
  const dragRows = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    const start = event.clientY
    const from = rows
    const move = (next: PointerEvent) => setRows(Math.max(0, Math.min(HISTORY_MAX_ROWS, from + Math.round((start - next.clientY) / ROW_PX))))
    const up = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }

  const parts = line.prompt
  const shown = expanded ? lines : rows > 0 ? lines.slice(-rows) : []
  const placeholder = disabled ? t('commandLine') : t(WAITING[idle ? 'command' : parts.kind] ?? 'commandLine')
  return (
    <form
      className="kui-bar cad-command-line"
      data-shake={shake ? 'true' : undefined}
      onSubmit={(event) => {
        event.preventDefault()
        const text = input.current?.value ?? ''
        // What the dynamic input boxes show goes as they show it: an empty box from the cursor,
        // measured from the last point (`@40<30`).
        if (dynamic.completes(text)) {
          const done = dynamic.finish(text)
          clear()
          void done.then(send)
          return
        }
        // INSERT typed between commands opens its dialog, as the reference's INSERT does.
        if (idle && onInsert && INSERT_NAMES.has(text.trim().toLowerCase())) {
          onCommand(text.trim())
          clear()
          onInsert()
          return
        }
        // Enter on a part of a name runs the first command it may mean (an exact name runs as typed).
        const first = suggestions[0]
        const completed = first && pending.current === 0 && !namesExactly(catalog, text) ? first.entry.name : text
        clear()
        send(completed)
      }}
    >
      <div className="cad-command-float">
        {suggestions.length ? (
          <ul className="cad-completions" role="listbox" aria-label={t('completions')} data-testid="cad-completions">
            {suggestions.map((suggestion, index) => (
              <li key={suggestion.entry.id} role="option" aria-selected={index === 0}>
                <button
                  type="button"
                  className="cad-completion"
                  data-first={index === 0 ? 'true' : undefined}
                  onPointerDown={(event) => event.preventDefault()}
                  onClick={() => choose(suggestion)}
                  data-testid={`cad-completion-${index}`}
                >
                  <span className="cad-completion-name">
                    {suggestion.alias ? `${suggestion.alias.toUpperCase()} (${suggestion.entry.name.toUpperCase()})` : suggestion.entry.name.toUpperCase()}
                  </span>
                  <span className="cad-completion-title">{phrase(suggestion.entry.title.replace(/(\.\.\.|…)\s*$/, ''))}</span>
                </button>
              </li>
            ))}
          </ul>
        ) : null}
        {expanded ? (
          <div
            className="cad-history"
            data-open="true"
            ref={log}
            data-testid="cad-history"
            onContextMenu={(event) => {
              event.preventDefault()
              // What is selected in the window, if anything: Copy takes it, else the whole history.
              const selection = window.getSelection()
              const inside = selection && log.current && selection.anchorNode && log.current.contains(selection.anchorNode)
              setMenu({ x: event.clientX, y: event.clientY, selected: inside ? selection.toString() : '' })
            }}
          >
            {lines.map((item) => <div key={item.key} className="cad-history-line" data-error={item.error ? 'true' : undefined}>{item.text}</div>)}
          </div>
        ) : (
          <div className="cad-history" data-testid="cad-history" style={{ height: shown.length ? undefined : 0 }}>
            <div className="cad-history-grip" onPointerDown={dragRows} title={t('historyRows')} data-testid="cad-history-grip" />
            {shown.map((item) => <div key={item.key} className="cad-history-line" data-error={item.error ? 'true' : undefined}>{item.text}</div>)}
          </div>
        )}
        {/* On the page itself: the floating history is placed (and moved) by the frame, and a menu
            inside it would be placed against it, not the window. */}
        {menu ? createPortal(
          <ContextMenu at={menu} onClose={() => setMenu(null)} label={t('commandHistory')} testId="cad-history-menu">
            <MenuItem label={phrase('Copy')} onSelect={() => void copyText(menu.selected || lines.map((item) => item.text).join('\n'))} testId="cad-history-menu-copy" />
            <MenuItem label={phrase('Copy History')} onSelect={() => void copyText(lines.map((item) => item.text).join('\n'))} testId="cad-history-menu-copy-all" />
          </ContextMenu>,
          document.body,
        ) : null}
        <div className="cad-prompt" data-testid="cad-prompt">
          {idle ? (
            <span className="kui-muted">{phrase('Command:')}</span>
          ) : (
            <>
              {parts.command ? <span className="cad-prompt-command">{parts.command}</span> : null}
              {parts.message ? <span>{` ${phrase(parts.message)}`}</span> : null}
              {parts.keywords.length ? (
                <>
                  <span>{parts.message ? (parts.message.endsWith('?') ? ' [' : ' or [') : '['}</span>
                  {parts.keywords.map((keyword, index) => (
                    <span key={keyword}>
                      {index ? '/' : ''}
                      <button type="button" className="cad-prompt-option" disabled={disabled} onPointerDown={(event) => event.preventDefault()} onClick={() => answer(keyword)} data-testid={`cad-prompt-option-${keyword}`}>
                        {keyword}
                      </button>
                    </span>
                  ))}
                  <span>]</span>
                </>
              ) : null}
              {parts.default != null ? (
                <>
                  {' '}
                  <button type="button" className="cad-prompt-option" disabled={disabled} onPointerDown={(event) => event.preventDefault()} onClick={() => answer('')} data-testid="cad-prompt-default">
                    {`<${parts.default}>`}
                  </button>
                </>
              ) : null}
              <span>:</span>
            </>
          )}
        </div>
      </div>
      <button
        type="button"
        className="kui-icon-button cad-history-toggle"
        aria-label={t('commandHistory')}
        title={`${t('commandHistory')} (F2)`}
        aria-pressed={expanded}
        onClick={() => onExpanded(!expanded)}
        data-testid="cad-history-toggle"
      >
        {expanded ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronUp size={14} aria-hidden="true" />}
      </button>
      <Terminal size={14} className="kui-muted" aria-hidden="true" />
      <input
        className="kui-field cad-command-input"
        ref={input}
        placeholder={placeholder}
        aria-label={t('commandLine')}
        spellCheck={false}
        disabled={disabled}
        onFocus={() => dynamic.focused()}
        onChange={(event) => { setHistoryIndex(null); setTyped(event.currentTarget.value); dynamic.setText(event.currentTarget.value) }}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return
          // The dynamic input's keys first: its options list (Down Arrow), Tab between its boxes,
          // typing in its first box after Tab went back to it.
          if (!event.metaKey && !event.ctrlKey && !event.altKey && !(event.key === 'Tab' && event.shiftKey)) {
            const taken = dynamic.key(event.key, historyIndex !== null)
            if (taken !== false) {
              event.preventDefault()
              event.stopPropagation()
              if (typeof taken === 'string') replace(event.currentTarget, taken, 'boxes')
              return
            }
          }
          // Space enters the line, as Enter does, except at a prompt that takes free text (the
          // engine shows the text cursor there) and inside a command's JSON parameters.
          if (event.key === ' ' && !event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey
            && tools?.canvasTool.cursor !== 'text' && !event.currentTarget.value.includes('{')) {
            event.preventDefault()
            event.currentTarget.form?.requestSubmit()
            return
          }
          // Tab completes the name to the first command it may mean.
          if (event.key === 'Tab' && !event.shiftKey && suggestions.length) {
            event.preventDefault()
            const name = suggestions[0].entry.name
            event.currentTarget.value = name
            setTyped(name)
            return
          }
          const shortcut = commands.find((command) => /^F\d+$/.test(command.shortcut ?? '') && command.shortcut === event.key)
          if (shortcut && enabled(shortcut.id)) {
            event.preventDefault()
            event.stopPropagation()
            void run(shortcut.id, {})
            return
          }
          if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
            if (!history.length) return
            event.preventDefault()
            if (historyIndex === null) draft.current = event.currentTarget.value
            const next = Math.max(0, Math.min(history.length, (historyIndex ?? history.length) + (event.key === 'ArrowUp' ? -1 : 1)))
            setHistoryIndex(next)
            replace(event.currentTarget, history[next] ?? draft.current, 'line')
            return
          }
          // Escape clears the line and cancels the running command, as on the canvas. It is taken
          // (default prevented) only when it cleared or cancelled something (KitPointer 4.4).
          if (event.key !== 'Escape') return
          const cleared = event.currentTarget.value !== ''
          clear()
          const cancelled = tools?.keyDown({ key: 'Escape', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false }) ?? false
          if (cleared || cancelled) event.preventDefault()
        }}
        data-testid="cad-command-line"
      />
    </form>
  )
}
