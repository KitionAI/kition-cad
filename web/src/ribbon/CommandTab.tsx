import { COMMAND_FALLBACK_ICON, CommandButton, RibbonButton, RibbonGroup, commandGroups, commandIcon, useKitEditor, useUi, type IconComponent, type KitCommand } from '@kition/sdk/editor'
import { startsPrompt, useCatalog, useStartCommand } from '../commands'

/**
 * A ribbon button for a command that asks for its input in prompts: it starts them on the command
 * line, as CAD's buttons do, instead of running the command's JSON form or opening its parameters
 * dialog (walkthrough B6). Test id and UI id as the shared `CommandButton`'s.
 */
export function PromptButton({ command, icon, label, size = 'small', primary, lineLabel }: { command: string; icon?: IconComponent; label?: string; size?: 'large' | 'small' | 'icon'; primary?: boolean; lineLabel?: boolean }) {
  const editor = useKitEditor()
  const ui = useUi()
  const start = useStartCommand()
  const state = ui.state(`command:${command}`)
  if (state.hidden) return null
  const known = editor.byId.get(command)
  return (
    <RibbonButton
      icon={icon ?? (known ? commandIcon(known) : undefined)}
      label={label ?? editor.phrase(known?.title ?? command)}
      size={size}
      primary={primary}
      lineLabel={lineLabel}
      disabled={!editor.enabled(command) || !editor.enabled('command.runLine') || state.disabled}
      onClick={() => { if (!state.denied) start(command) }}
      onContextMenu={ui.offerHide ? (event) => { event.preventDefault(); ui.offerHide?.(`command:${command}`, event.clientX, event.clientY) } : undefined}
      data-testid={`${editor.testId}-cmd-${command}`}
      data-kit-ui-id={`command:${command}`}
    />
  )
}

/** One of a menu tab's commands: its prompts when it has them, else as the shared button runs it; Hatch… its dialog. */
function MenuCommand({ command, primary, onHatch }: { command: KitCommand; primary: boolean; onHatch?: () => void }) {
  const catalog = useCatalog()
  const editor = useKitEditor()
  const icon = commandIcon(command)
  if (command.id === 'hatch.add' && onHatch) {
    return (
      <RibbonButton
        icon={icon}
        label={editor.phrase(command.title)}
        size="small"
        primary={primary}
        lineLabel={icon === COMMAND_FALLBACK_ICON}
        disabled={!editor.enabled(command.id) || !editor.enabled('command.runLine')}
        onClick={onHatch}
        data-testid={`${editor.testId}-cmd-${command.id}`}
        data-kit-ui-id={`command:${command.id}`}
      />
    )
  }
  if (startsPrompt(catalog.byId.get(command.id))) return <PromptButton command={command.id} icon={icon} primary={primary} lineLabel={icon === COMMAND_FALLBACK_ICON} />
  return <CommandButton command={command.id} icon={icon} primary={primary} lineLabel={icon === COMMAND_FALLBACK_ICON} />
}

/**
 * A ribbon tab of the engine's menu `menu`, grouped by its second menu segment as kit-editor's
 * `CommandTab` groups it, where a command that asks for its input in prompts starts them
 * ([`PromptButton`]). `primary` lists the commands Single line's row shows.
 */
export function CadCommandTab({ menu, exclude = [], primary = [], onHatch }: { menu: string; exclude?: readonly string[]; primary?: readonly string[]; onHatch?: () => void }) {
  const { commands, phrase } = useKitEditor()
  const groups = commandGroups(commands.filter((command) => !exclude.includes(command.id)), [menu])
  return (
    <>
      {[...groups].map(([name, { path, items }]) => (
        <RibbonGroup key={name} id={`menu:${path}`} label={phrase(name)} icon={items[0] ? commandIcon(items[0]) : undefined}>
          <div className="kui-command-grid">
            {items.map((command) => <MenuCommand key={command.id} command={command} primary={primary.includes(command.id)} onHatch={onHatch} />)}
          </div>
        </RibbonGroup>
      ))}
    </>
  )
}
