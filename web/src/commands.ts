import { createContext, createElement, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import { useKitEditor, useKitTools } from '@kition/sdk/editor'
import { describe, gripping } from './cad'

/**
 * One of the commands the command line knows (the engine's `command.list`): the name typed for it
 * (`line`), its aliases (`l`), its title and menu path, whether it asks for its input in prompts
 * (`interactive`), and whether the command line may start it (not one that reads or writes files).
 */
export type CatalogEntry = { id: string; name: string; aliases: string[]; title: string; menu: string[]; interactive: boolean; commandLine: boolean }

export type Catalog = { entries: CatalogEntry[]; byId: Map<string, CatalogEntry> }

const EMPTY: Catalog = { entries: [], byId: new Map() }

export const CatalogContext = createContext<Catalog>(EMPTY)

/** The command catalogue, asked of the engine once per drawing (it changes only with the engine). */
export function useCatalogLoader(): Catalog {
  const { query, session } = useKitEditor()
  const [catalog, setCatalog] = useState<Catalog>(EMPTY)
  const { doc } = session
  useEffect(() => {
    if (!doc) return
    let gone = false
    void query<{ commands?: CatalogEntry[] }>('command.list', {}).then((answer) => {
      const entries = answer?.commands ?? []
      if (!gone) setCatalog({ entries, byId: new Map(entries.map((entry) => [entry.id, entry])) })
    })
    return () => { gone = true }
  }, [doc, query])
  return catalog
}

export function useCatalog(): Catalog {
  return useContext(CatalogContext)
}

/**
 * Whether a button, menu item or context-menu item for the command starts its prompts, as CAD's
 * do: it asks for its input in prompts and the command line may start it. A title that ends in an
 * ellipsis ("Hatch...", "Table...") asks for its parameters first, so it keeps its dialog.
 */
export function startsPrompt(entry: CatalogEntry | undefined): entry is CatalogEntry {
  return entry != null && entry.interactive && entry.commandLine && !/(\.\.\.|…)\s*$/.test(entry.title)
}

/**
 * Starts a command's prompts as a menu does in CAD: the command running (or a hot grip) is
 * cancelled first, then the command's name goes to the command line, so the history shows
 * "Command: DONUT" and the command line its first prompt.
 */
export function useStartCommand(): (id: string) => void {
  const editor = useKitEditor()
  const tools = useKitTools()
  const catalog = useCatalog()
  return useCallback((id: string) => {
    const name = catalog.byId.get(id)?.name ?? id
    const now = editor.session.doc ? describe(editor.session.doc.info) : null
    if (now && (now.running != null || gripping(now))) {
      tools?.keyDown({ key: 'Escape', shiftKey: false, altKey: false, metaKey: false, ctrlKey: false })
    }
    void editor.run('command.runLine', { text: name }).then(() => tools?.refresh())
  }, [catalog, editor, tools])
}

/** The most suggestions the command line lists. */
export const SUGGESTIONS = 8

/** A suggestion: the command, and the alias it was found by (shown as `REC (RECTANG)`). */
export type Suggestion = { entry: CatalogEntry; alias: string | null }

/** What looks like a command name being typed (not a point, a number, `name {json}` or `'zoom`). */
const NAME = /^-?[a-z][a-z0-9._-]*$/i

/**
 * The commands a typed name may mean, at most [`SUGGESTIONS`]: first the command it names exactly
 * (by its name, an alias or its id), then those whose name starts with it, then those with an alias
 * that does. Nothing when the text is not a name.
 */
export function suggest(catalog: Catalog, typed: string): Suggestion[] {
  const text = typed.trim().toLowerCase()
  if (!NAME.test(text)) return []
  const usable = catalog.entries.filter((entry) => entry.commandLine)
  const out: Suggestion[] = []
  const seen = new Set<string>()
  const add = (entry: CatalogEntry, alias: string | null) => {
    if (seen.has(entry.id) || out.length >= SUGGESTIONS) return
    seen.add(entry.id)
    out.push({ entry, alias })
  }
  for (const entry of usable) {
    const alias = entry.aliases.find((item) => item.toLowerCase() === text) ?? null
    if (entry.name.toLowerCase() === text || entry.id.toLowerCase() === text || alias) add(entry, entry.name.toLowerCase() === text ? null : alias)
  }
  const byName = (a: CatalogEntry, b: CatalogEntry) => a.name.length - b.name.length || a.name.localeCompare(b.name)
  for (const entry of usable.filter((item) => item.name.toLowerCase().startsWith(text)).sort(byName)) add(entry, null)
  for (const entry of usable.filter((item) => item.aliases.some((alias) => alias.toLowerCase().startsWith(text))).sort(byName)) {
    add(entry, entry.aliases.find((alias) => alias.toLowerCase().startsWith(text)) ?? null)
  }
  return out
}

/** Whether the typed text names a command exactly (then Enter runs it as typed). */
export function namesExactly(catalog: Catalog, typed: string): boolean {
  const text = typed.trim().toLowerCase()
  return catalog.entries.some((entry) => entry.name.toLowerCase() === text || entry.id.toLowerCase() === text || entry.aliases.some((alias) => alias.toLowerCase() === text))
}

/** Gives the command catalogue to everything below (inside the editor provider). */
export function CatalogProvider({ children }: { children: ReactNode }) {
  const catalog = useCatalogLoader()
  return createElement(CatalogContext.Provider, { value: catalog }, children)
}
