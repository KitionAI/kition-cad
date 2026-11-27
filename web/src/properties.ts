import { useCallback } from 'react'
import { useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import type { CadInfo, EditRequest } from './cad'

/** The engine's read command the Properties panel and the layer list are drawn from (`properties.rs`). */
export const DESCRIBE = 'object.describeProperties'

/** A choice of a choice or colour row; `load` marks a library linetype loaded first when chosen. */
export type RowOption = { value: string | number | boolean; label: string; rgb?: string; load?: boolean }

/** How a new value is written: `command` with `params`, the value under `key` (one coordinate of `point` at `index`). */
export type RowWrite = { command: string; params: Record<string, unknown>; key: string; point?: [number, number]; index?: number }

/** One row of the Properties panel, as the engine describes it. */
export type PropertyRow = {
  group: string
  key: string
  label: string
  kind: 'number' | 'text' | 'choice' | 'color' | 'boolean'
  unit?: 'length' | 'angle' | 'area' | 'number'
  value: string | number | boolean | null
  display: string
  varies: boolean
  editable: boolean
  positive?: boolean
  write?: RowWrite
  options?: RowOption[]
  rgb?: string
}

/** A layer as the layer list shows it; `deleteBlocked` says why it cannot be deleted. */
export type LayerDetails = {
  name: string
  color: string
  rgb: string
  linetype: string
  lineweight: string | number
  transparency: number
  on: boolean
  frozen: boolean
  locked: boolean
  current: boolean
  deleteBlocked: 'reserved' | 'current' | 'used' | null
}

export type Described = {
  count: number
  current: boolean
  locked: boolean
  types: Array<{ type: string; count: number }>
  rows: PropertyRow[]
  layers: LayerDetails[]
  linetypes: string[]
  library: string[]
  patterns: string[]
  colors: Array<{ value: string; rgb: string }>
  lineweights: RowOption[]
  inPlace?: EditRequest
  parsed?: Array<number | null>
}

/**
 * The description of the selection (or the current properties), asked again as the drawing or the
 * selection changes; a polyline's rows show its vertex `vertex` (from 0).
 */
export function useDescribed(info: CadInfo, vertex = 0): Described | null {
  const key = `${info.selectedObjects.map((object) => object.handle).join(',')}|${info.entities}|${info.currentLayer}|${JSON.stringify(info.layers)}|${vertex}`
  return useEngineQuery<Described>(DESCRIBE, vertex ? { vertex } : {}, key)
}

/** A row option's value as a `<select>` holds it: strings as they are (layer names), others as JSON. */
export const optionKey = (value: RowOption['value'] | null): string => (value == null ? '' : typeof value === 'string' ? value : JSON.stringify(value))

/**
 * Writes rows: `write(row, value)` runs the row's command with the value (a point keeps its other
 * coordinate); `parse(text, unit)` reads a typed value as the engine does (arithmetic, feet and
 * inches, a unit, angles), null when it does not read.
 */
export function useRowWriter() {
  const { run, query } = useKitEditor()
  const write = useCallback(async (row: { write?: RowWrite }, value: unknown) => {
    const w = row.write
    if (!w) return
    let next = value
    if (w.point && w.index != null) {
      const point = [...w.point]
      point[w.index] = value as number
      next = point
    }
    await run(w.command, { ...w.params, [w.key]: next })
  }, [run])
  const parse = useCallback(async (text: string, unit: string | undefined): Promise<number | null> => {
    const answer = await query<Described>(DESCRIBE, { handles: [], parse: [{ text, unit: unit === 'length' || unit === 'area' ? 'length' : unit === 'angle' ? 'angle' : 'number' }] })
    const value = answer?.parsed?.[0]
    return typeof value === 'number' && Number.isFinite(value) ? value : null
  }, [query])
  return { write, parse }
}
