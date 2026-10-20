// CyberManju OS — Install (src/ui/install.ts).
// Role: shell theme/layout tokens.
// Exports: uiComponents, installUi.
// Integrates: vue, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import type { App, Component } from 'vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiCard from '@/components/ui/UiCard.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiChip from '@/components/ui/UiChip.vue'
import UiDivider from '@/components/ui/UiDivider.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiGrid from '@/components/ui/UiGrid.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiListRow from '@/components/ui/UiListRow.vue'
import UiMenu from '@/components/ui/UiMenu.vue'
import UiModal from '@/components/ui/UiModal.vue'
import UiProgress from '@/components/ui/UiProgress.vue'
import UiSection from '@/components/ui/UiSection.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSidebarItem from '@/components/ui/UiSidebarItem.vue'
import UiTrafficLights from '@/components/ui/UiTrafficLights.vue'
import UiSpinner from '@/components/ui/UiSpinner.vue'
import UiStack from '@/components/ui/UiStack.vue'
import UiText from '@/components/ui/UiText.vue'
import UiTitle from '@/components/ui/UiTitle.vue'
import UiTitlebarButtons from '@/components/ui/UiTitlebarButtons.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import UiToolbar from '@/components/ui/UiToolbar.vue'

/**
 * The CyberManju design system. Registered globally in `main.ts` so every
 * window, panel and dialog renders from the same primitives — this, together
 * with the `--ui-*` tokens, is the single source of truth for the OS's UI/UX.
 */
export const uiComponents = {
  UiBadge,
  UiButton,
  UiCard,
  UiCheckbox,
  UiChip,
  UiDivider,
  UiEmpty,
  UiError,
  UiGrid,
  UiInput,
  UiListRow,
  UiMenu,
  UiModal,
  UiProgress,
  UiSection,
  UiSegmented,
  UiSelect,
  UiSidebarItem,
  UiTrafficLights,
  UiSpinner,
  UiStack,
  UiText,
  UiTitle,
  UiTitlebarButtons,
  UiToggle,
  UiToolbar,
} satisfies Record<string, Component>

export function installUi(app: App) {
  for (const [name, component] of Object.entries(uiComponents)) {
    app.component(name, component)
  }
}

export default installUi
