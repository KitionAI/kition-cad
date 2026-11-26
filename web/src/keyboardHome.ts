import { useEffect } from 'react'

/** Where the drawing takes the keyboard: the editor frame's root, as after a click on the canvas. */
const drawing = () => document.querySelector<HTMLElement>('.kui-app')

/** Places that keep the keyboard they take: dialogs, menus and the panels a button opens. */
const KEEPS = '[role="dialog"], [role="menu"], .kui-drop-panel'

/** Whether the keyboard is where nothing takes it: the page, or a button the pointer just pressed. */
function stranded(element: Element | null): boolean {
  if (!element || element === document.body || element === document.documentElement) return true
  return element instanceof HTMLButtonElement && element.closest('.kui-app') != null && element.closest(KEEPS) == null
}

/**
 * CAD's keyboard follows the command line, not the last thing clicked (regression
 * issue-keys-after-ribbon): after a button of the ribbon, the status bar or a panel is clicked with
 * the pointer, the drawing takes the keyboard again, so a point typed after the ribbon's Polyline,
 * Enter and Space reach the command instead of the button; and a key pressed while the page has
 * the keyboard (a dialog or a menu just closed) goes to the drawing. A button pressed with the
 * keyboard keeps it (keyboard navigation of the ribbon).
 */
export function useKeyboardHome() {
  useEffect(() => {
    let frame = 0
    const clicked = (event: MouseEvent) => {
      if (event.detail === 0) return
      window.cancelAnimationFrame(frame)
      // After the click's own work: a dialog or a panel it opened has the keyboard by then.
      frame = window.requestAnimationFrame(() => {
        if (stranded(document.activeElement)) drawing()?.focus({ preventScroll: true })
      })
    }
    const pressed = (event: KeyboardEvent) => {
      const target = event.target instanceof Element ? event.target : null
      if (target && target !== document.body && target !== document.documentElement) return
      const home = drawing()
      if (!home) return
      event.preventDefault()
      event.stopImmediatePropagation()
      home.focus({ preventScroll: true })
      home.dispatchEvent(new KeyboardEvent('keydown', { key: event.key, code: event.code, shiftKey: event.shiftKey, ctrlKey: event.ctrlKey, altKey: event.altKey, metaKey: event.metaKey, repeat: event.repeat, bubbles: true, cancelable: true }))
    }
    document.addEventListener('click', clicked)
    document.addEventListener('keydown', pressed, true)
    return () => {
      window.cancelAnimationFrame(frame)
      document.removeEventListener('click', clicked)
      document.removeEventListener('keydown', pressed, true)
    }
  }, [])
}
