import { Children, useContext, useRef, useState, type ReactNode } from 'react'
import { ICON_STROKE, RibbonGroup, RibbonStack, useKitEditor, type IconComponent } from '@kition/sdk/editor'
import { DropPanel, FULL_RIBBON_FIT, RibbonFitContext, RibbonPresentationContext, ribbonGroupKey, type RibbonFit } from '@kition/tessera/ui'
import { ChevronDownIcon, EllipsisIcon } from '@kition/tessera/icons/react'

/** A group drawn as it is laid out: large buttons stay large (the frame's first step would draw them small, which widens a row of them). */
const AS_LAID_OUT: RibbonFit = { size: 'full', icons: new Set(), collapsed: new Set() }
/** A group drawn in its compact form: small buttons with their labels. */
const COMPACT: RibbonFit = { size: 'small', icons: new Set(), collapsed: new Set() }

/**
 * The rest of a compact group: a small "More" button in the group's last column that opens the
 * rest under it, at full size and with their labels (as the frame's collapsed group opens a whole
 * group). Test ids `cad-ribbon-more-<name>` and `cad-ribbon-more-panel-<name>`.
 */
function RibbonMore({ name, label, children }: { name: string; label: string; children: ReactNode }) {
  const { phrase } = useKitEditor()
  const anchor = useRef<HTMLSpanElement>(null)
  const [open, setOpen] = useState(false)
  const more = phrase('More')
  return (
    <span ref={anchor} className="cad-ribbon-more">
      <button
        type="button"
        className="kui-ribbon-button"
        data-size="small"
        title={`${more}: ${label}`}
        aria-label={`${more}: ${label}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        data-testid={`cad-ribbon-more-${name}`}
      >
        <EllipsisIcon size={16} strokeWidth={ICON_STROKE} aria-hidden="true" />
        <span>{more}</span>
        <ChevronDownIcon size={12} strokeWidth={ICON_STROKE} aria-hidden="true" />
      </button>
      <DropPanel open={open} anchor={anchor} onClose={() => setOpen(false)} label={label} className="kui-line-popover" testId={`cad-ribbon-more-panel-${name}`}>
        <RibbonPresentationContext.Provider value="full">
          <RibbonFitContext.Provider value={FULL_RIBBON_FIT}>
            <div className="kui-ribbon-group">
              <div className="kui-ribbon-group-body">
                <fieldset className="kui-element" role="presentation">{children}</fieldset>
              </div>
              <div className="kui-ribbon-group-label">{label}</div>
            </div>
          </RibbonFitContext.Provider>
        </RibbonPresentationContext.Provider>
      </DropPanel>
    </span>
  )
}

/**
 * A Classic ribbon group that keeps its labels when the ribbon runs out of room (walkthrough N18):
 * where the frame would draw the group as icons alone, it draws `shown` as small labelled buttons in
 * columns of three, the last place a "More" button that opens `more` with their labels. Without a
 * compact form (`shown` unset: a group with a field, such as Layers) it gives way as the frame says.
 * With `collapse` it gives way as one labelled button that opens the whole group instead (the
 * frame's collapsed group), so the groups before it keep all their commands in view. Large buttons
 * stay large until then. In Single line and in panels it is the shared group.
 * `name` is the group's test id part.
 */
export function FitGroup({ id, label, icon, name, shown, more = [], collapse = false, children }: { id?: string; label: string; icon?: IconComponent; name: string; shown?: ReactNode[]; more?: ReactNode[]; collapse?: boolean; children: ReactNode }) {
  const fit = useContext(RibbonFitContext)
  const presentation = useContext(RibbonPresentationContext)
  const key = ribbonGroupKey({ id, label })
  if (presentation !== 'full' || fit.collapsed.has(key)) return <RibbonGroup id={id} label={label} icon={icon}>{children}</RibbonGroup>
  if (!fit.icons.has(key)) {
    return (
      <RibbonFitContext.Provider value={AS_LAID_OUT}>
        <RibbonGroup id={id} label={label} icon={icon}>{children}</RibbonGroup>
      </RibbonFitContext.Provider>
    )
  }
  if (collapse) {
    return (
      <RibbonFitContext.Provider value={{ ...fit, collapsed: new Set([...fit.collapsed, key]) }}>
        <RibbonGroup id={id} label={label} icon={icon}>{children}</RibbonGroup>
      </RibbonFitContext.Provider>
    )
  }
  if (!shown) return <RibbonGroup id={id} label={label} icon={icon}>{children}</RibbonGroup>
  const items = Children.toArray(shown)
  const rest = Children.toArray(more)
  const cells = rest.length ? [...items, <RibbonMore key="more" name={name} label={label}>{rest}</RibbonMore>] : items
  const columns: ReactNode[][] = []
  for (let at = 0; at < cells.length; at += 3) columns.push(cells.slice(at, at + 3))
  return (
    <RibbonFitContext.Provider value={COMPACT}>
      <RibbonGroup id={id} label={label} icon={icon}>
        {columns.map((column, index) => <RibbonStack key={index}>{column}</RibbonStack>)}
      </RibbonGroup>
    </RibbonFitContext.Provider>
  )
}
