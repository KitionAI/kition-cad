import { MenuButton, MenuItem, MenuSeparator, useKitEditor } from '@kition/sdk/editor'
import { DRAFTING, type CadInfo, type DraftingMode } from './cad'

/**
 * The running object snap modes the Osnap button's menu sets (OSMODE's bits), in the order of the
 * drafting settings' list: the bit, and its label.
 */
const OBJECT_SNAPS: Array<{ bit: number; label: string }> = [
  { bit: 1, label: 'Endpoint' },
  { bit: 2, label: 'Midpoint' },
  { bit: 4, label: 'Center' },
  { bit: 1024, label: 'Geometric Center' },
  { bit: 8, label: 'Node' },
  { bit: 16, label: 'Quadrant' },
  { bit: 32, label: 'Intersection' },
  { bit: 4096, label: 'Extension' },
  { bit: 64, label: 'Insertion' },
  { bit: 128, label: 'Perpendicular' },
  { bit: 256, label: 'Tangent' },
  { bit: 512, label: 'Nearest' },
  { bit: 2048, label: 'Apparent Intersection' },
  { bit: 8192, label: 'Parallel' },
]
/** Every running object snap mode's bits together. */
const ALL_SNAPS = OBJECT_SNAPS.reduce((bits, mode) => bits | mode.bit, 0)
/** The increment angles the Polar button's menu offers (POLARANG), in degrees. */
const POLAR_ANGLES = [90, 45, 30, 22.5, 18, 15, 10, 5]

/** Which status bar mode has a menu of its settings beside it. */
const MENUS: Partial<Record<DraftingMode, 'osnap' | 'polar'>> = { osnap: 'osnap', polar: 'polar' }

/**
 * The status bar's right end: Zoom Extents, then the drafting modes in CAD's order. Each mode's
 * tooltip names it and its key (F3, F8…); Osnap and Polar have a small arrow beside them that opens
 * their settings: the running object snap modes (checked ones on; choosing one turns object snaps
 * on), and polar tracking's increment angle. They run between and inside commands alike.
 */
export function DraftingStatus({ info }: { info: CadInfo }) {
  const { run, enabled, phrase, t, byId } = useKitEditor()
  const tip = (command: string, fallback: string) => {
    const known = byId.get(command)
    const name = phrase(known?.title ?? fallback)
    return known?.shortcut ? `${name} (${known.shortcut})` : name
  }
  const osmode = info.drafting.osmode ?? 0
  const polarAngle = info.drafting.polarAngle ?? 90
  const setSnaps = (bits: number) => void run('drafting.objectSnap', { osmode: bits })
  const setAngle = async (angle: number) => {
    await run('variable.set', { name: 'POLARANG', value: angle })
    if (!info.drafting.polar) await run('drafting.polar', { on: true })
  }
  return (
    <span className="cad-drafting">
      <button type="button" className="cad-mode" title={t('zoomExtents')} disabled={!enabled('view.zoomToFit')} onClick={() => void run('view.zoomToFit', {})} data-testid="cad-status-zoom-extents">
        {t('zoomExtents')}
      </button>
      {DRAFTING.map((mode) => (
        <span key={mode.id} className="cad-mode-pair">
          <button
            type="button"
            className="cad-mode"
            aria-pressed={info.drafting[mode.id]}
            title={tip(mode.command, mode.label)}
            disabled={!enabled(mode.command)}
            onClick={() => void run(mode.command, { on: !info.drafting[mode.id] })}
            data-testid={`cad-mode-${mode.id}`}
          >
            {phrase(mode.label)}
          </button>
          {MENUS[mode.id] === 'osnap' ? (
            <MenuButton
              align="end"
              placement="auto"
              trigger={({ open, toggle }) => (
                <button type="button" className="cad-mode-arrow" aria-haspopup="menu" aria-expanded={open} aria-label={phrase('Object Snap Settings')} title={phrase('Object Snap Settings')} disabled={!enabled('drafting.objectSnap')} onClick={toggle} data-testid="cad-mode-osnap-menu">▾</button>
              )}
            >
              {OBJECT_SNAPS.map((mode) => (
                <MenuItem key={mode.bit} label={phrase(mode.label)} checked={(osmode & mode.bit) !== 0} onSelect={() => setSnaps(osmode ^ mode.bit)} testId={`cad-osnap-mode-${mode.bit}`} />
              ))}
              <MenuSeparator />
              <MenuItem label={phrase('Select All')} onSelect={() => setSnaps(ALL_SNAPS)} testId="cad-osnap-mode-all" />
              <MenuItem label={phrase('Clear All')} onSelect={() => setSnaps(0)} testId="cad-osnap-mode-none" />
            </MenuButton>
          ) : MENUS[mode.id] === 'polar' ? (
            <MenuButton
              align="end"
              placement="auto"
              trigger={({ open, toggle }) => (
                <button type="button" className="cad-mode-arrow" aria-haspopup="menu" aria-expanded={open} aria-label={phrase('Polar Angle Settings')} title={phrase('Polar Angle Settings')} disabled={!enabled('variable.set')} onClick={toggle} data-testid="cad-mode-polar-menu">▾</button>
              )}
            >
              {POLAR_ANGLES.map((angle) => (
                <MenuItem key={angle} label={`${angle}°`} checked={Math.abs(polarAngle - angle) < 1e-6} onSelect={() => void setAngle(angle)} testId={`cad-polar-angle-${angle}`} />
              ))}
            </MenuButton>
          ) : null}
        </span>
      ))}
    </span>
  )
}
