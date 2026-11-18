import type { KitModuleManifest } from '@kition/kit-editor'

/** The engine, from this package's own folder. */
export function engineManifest(): KitModuleManifest {
  return {
    kitId: 'kit.cad',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_cad_kit',
    documentClass: 'CadDocument',
  }
}
