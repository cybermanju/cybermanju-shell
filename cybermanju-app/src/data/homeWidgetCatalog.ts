// CyberManju OS — Home widget reference library.
// Role: distinguish ready local widgets/app links from native and service concepts.
// Exports: typed catalog, category labels and verified inspiration links.
/**
 * Home widget reference catalog. This is descriptive metadata only: it does not
 * render widgets or imply native Android integration. Local entries are scoped
 * to browser-side UI implementations; native entries require a future host.
 */
export type HomeWidgetKind = 'local' | 'app-link' | 'native' | 'concept'

export interface HomeWidgetCatalogEntry {
  id: string
  title: string
  description: string
  category: string
  /** Iconify Solar icon name or a plain text/emoji glyph. */
  icon: string
  kind: HomeWidgetKind
  /** Existing MobileLauncher panel ID; used only by app-link entries. */
  appId?: string
  /** Public reference backing the idea or capability. */
  sourceUrl?: string
}

/** Verified public references used as inspiration; no third-party code/assets copied. */
export const homeWidgetSources = {
  smartspacer: 'https://github.com/KieronQuinn/Smartspacer',
  kvaesitso: 'https://f-droid.org/en/packages/de.mm20.launcher2.release/',
  fossify: 'https://www.fossify.org/',
  fossifyCalendar: 'https://www.fossify.org/apps/calendar/',
  gadgetbridge: 'https://github.com/gadgetbridge/gadgetbridge',
} as const

/** Stable category labels for grouping and filtering the reference catalog. */
export const homeWidgetCategories = [
  'Personal', 'Productivity', 'Wellbeing', 'CyberManju', 'Apps',
  'Time & context', 'Smart home', 'Health & devices', 'Ideas',
] as const

/**
 * `local` means implemented by the local-widget renderer. `app-link` points at
 * a real existing panel. `native`
 * requires Android/widget-host work; `concept` is exploratory and not shipped.
 */
export const homeWidgetCatalog: HomeWidgetCatalogEntry[] = [
  { id: 'clock', title: 'Clock', description: 'Local time and date, rendered in the app UI.', category: 'Personal', icon: 'solar:clock-circle-bold', kind: 'local' },
  { id: 'notes', title: 'Quick notes', description: 'Compact notes widget supported by the local-widget renderer.', category: 'Productivity', icon: 'solar:notes-bold', kind: 'local' },
  { id: 'counter', title: 'Counter', description: 'Tap-to-increment counter widget supported by the local-widget renderer.', category: 'Personal', icon: 'solar:calculator-bold', kind: 'local' },
  { id: 'pixel-pet', title: 'Pixel pet', description: 'Decorative companion widget supported by the local-widget renderer.', category: 'Personal', icon: 'solar:star-bold', kind: 'local' },
  { id: 'focus', title: 'Focus timer', description: 'In-app focus timer widget; it does not promise background-accurate Android alarms.', category: 'Wellbeing', icon: 'solar:hourglass-bold', kind: 'local' },
  { id: 'checklist', title: 'Checklist', description: 'Compact checklist widget supported by the local-widget renderer.', category: 'Productivity', icon: 'solar:checklist-minimalistic-bold', kind: 'local' },
  { id: 'daily-intention', title: 'Daily intention', description: 'An idea for a brief daily prompt; no renderer exists yet.', category: 'Wellbeing', icon: 'solar:sun-2-bold', kind: 'concept' },
  { id: 'quote', title: 'Quote card', description: 'An idea for a user-managed quote card; no renderer exists yet.', category: 'Personal', icon: 'solar:chat-square-quote-bold', kind: 'concept' },
  { id: 'vault-status', title: 'Private vaults', description: 'Shortcut to the existing vault/storage panel; a future compact status view.', category: 'CyberManju', icon: 'solar:shield-check-bold', kind: 'app-link', appId: 'storage' },
  { id: 'sync-status', title: 'Connected sources', description: 'Shortcut to the existing provider sync panel; not a live widget yet.', category: 'CyberManju', icon: 'solar:refresh-bold', kind: 'app-link', appId: 'sync' },
  { id: 'files', title: 'Files', description: 'Open the existing file manager panel.', category: 'Apps', icon: 'solar:folder-bold', kind: 'app-link', appId: 'files' },
  { id: 'search', title: 'Search', description: 'Open existing unified search.', category: 'Apps', icon: 'solar:magnifier-bold', kind: 'app-link', appId: 'search' },
  { id: 'agent', title: 'Agent', description: 'Open the existing AI assistant panel.', category: 'Apps', icon: 'solar:bot-bold', kind: 'app-link', appId: 'agent' },
  { id: 'terminal', title: 'Terminal', description: 'Open the existing cybsh terminal panel.', category: 'Apps', icon: 'solar:file-terminal-bold', kind: 'app-link', appId: 'terminal' },
  { id: 'editor', title: 'Code Studio', description: 'Open the existing code editor panel.', category: 'Apps', icon: 'solar:file-code-bold', kind: 'app-link', appId: 'editor' },
  { id: 'map', title: 'Map', description: 'Open existing map of GPS-tagged files.', category: 'Apps', icon: 'solar:map-bold', kind: 'app-link', appId: 'map' },
  { id: 'library', title: 'Library', description: 'Open existing collections/library panel.', category: 'Apps', icon: 'solar:library-bold', kind: 'app-link', appId: 'collections' },
  { id: 'settings', title: 'Settings', description: 'Open existing settings panel.', category: 'Apps', icon: 'solar:settings-bold', kind: 'app-link', appId: 'settings' },
  { id: 'agenda-native', title: 'Agenda glance', description: 'Android-hosted calendar events and reminders; requires a native widget host and calendar access.', category: 'Time & context', icon: 'solar:calendar-bold', kind: 'native', sourceUrl: homeWidgetSources.fossifyCalendar },
  { id: 'weather-native', title: 'Weather glance', description: 'Contextual forecast tile; provider, location permissions and refresh policy remain undecided.', category: 'Time & context', icon: 'solar:cloud-sun-2-bold', kind: 'native', sourceUrl: homeWidgetSources.smartspacer },
  { id: 'commute-native', title: 'Commute', description: 'Transit/departure context surfaced through a future host or plugin integration.', category: 'Time & context', icon: 'solar:bus-bold', kind: 'native', sourceUrl: homeWidgetSources.smartspacer },
  { id: 'steps-native', title: 'Activity summary', description: 'Steps/activity require compatible health data or a device integration; not available here.', category: 'Health & devices', icon: 'solar:walking-bold', kind: 'native', sourceUrl: homeWidgetSources.gadgetbridge },
  { id: 'battery-native', title: 'Device battery', description: 'System/device battery information via a future native bridge.', category: 'Health & devices', icon: 'solar:battery-charge-bold', kind: 'native' },
  { id: 'smartspace-concept', title: 'Context stack', description: 'Priority-based cards such as events, travel and reminders; Smartspacer demonstrates plugin-driven contextual stacks.', category: 'Ideas', icon: 'solar:widget-5-bold', kind: 'concept', sourceUrl: homeWidgetSources.smartspacer },
  { id: 'launcher-search-concept', title: 'Search-first home', description: 'A launcher concept emphasizing global search across apps and information, inspired by Kvaesitso.', category: 'Ideas', icon: 'solar:magnifier-bold', kind: 'concept', sourceUrl: homeWidgetSources.kvaesitso },
  { id: 'calendar-privacy-concept', title: 'Private calendar glance', description: 'Calendar widget direction with local events and reminders; the Fossify project documents such app capabilities, not an integration with this app.', category: 'Ideas', icon: 'solar:calendar-mark-bold', kind: 'concept', sourceUrl: homeWidgetSources.fossifyCalendar },
  { id: 'device-telemetry-concept', title: 'Open device telemetry', description: 'Possible future opt-in device/health summaries through compatible external integrations.', category: 'Ideas', icon: 'solar:watch-round-bold', kind: 'concept', sourceUrl: homeWidgetSources.gadgetbridge },
]
