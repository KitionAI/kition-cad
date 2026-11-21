import { useRef, useState, type RefObject } from 'react'
import { TerminalIcon as Terminal } from '@kition/tessera/icons/react'
import { useKitEditor, useKitTools } from '@kition/sdk/editor'

/** Kition CAD's command line: the engine's prompt, and what the person types run as a command. */
export function CommandLine({ prompt, running, input, history, onCommand }: { prompt: string; running: string | null; input: RefObject<HTMLInputElement | null>; history: string[]; onCommand: (text: string) => void }) {
  const { run, enabled, t, phrase, commands } = useKitEditor()
  // What is typed runs as a command that may edit: a viewer cannot type here.
  const disabled = !enabled('commandLine')
  const tools = useKitTools()
  const [historyIndex, setHistoryIndex] = useState<number | null>(null)
  const draft = useRef('')
  return (
    <form
      className="kui-bar cad-command-line"
      onSubmit={(event) => {
        event.preventDefault()
        const text = input.current?.value ?? ''
        if (input.current) input.current.value = ''
        setHistoryIndex(null)
        if (!running && !prompt.startsWith('STRETCH ') && text.trim()) onCommand(text.trim())
        void run('commandLine', { text }).then(() => tools?.refresh())
      }}
    >
      <Terminal size={14} className="kui-muted" aria-hidden="true" />
      <span className="cad-prompt" data-testid="cad-prompt">{phrase(tools?.status ?? prompt)}</span>
      <input
        className="kui-field cad-command-input"
        ref={input}
        placeholder={t('commandLine')}
        aria-label={t('commandLine')}
        spellCheck={false}
        disabled={disabled}
        onChange={() => setHistoryIndex(null)}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return
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
            event.currentTarget.value = history[next] ?? draft.current
            event.currentTarget.setSelectionRange(event.currentTarget.value.length, event.currentTarget.value.length)
            return
          }
          // Escape clears the line and cancels the running command, as on the canvas. It is taken
          // (default prevented) only when it cleared or cancelled something (KitPointer 4.4).
          if (event.key !== 'Escape') return
          const typed = event.currentTarget.value !== ''
          event.currentTarget.value = ''
          setHistoryIndex(null)
          const cancelled = tools?.keyDown({ key: 'Escape', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false }) ?? false
          if (typed || cancelled) event.preventDefault()
        }}
        data-testid="cad-command-line"
      />
    </form>
  )
}
