// CyberManju OS — Globals.d (src/ui/globals.d.ts).
/**
 * Global component typings for the design system. Lets templates use
 * `<UiButton>` etc. without per-file imports, and keeps vue-tsc happy.
 */
declare module 'vue' {
  export interface GlobalComponents {
    UiBadge: typeof import('@/components/ui/UiBadge.vue').default
    UiButton: typeof import('@/components/ui/UiButton.vue').default
    UiCard: typeof import('@/components/ui/UiCard.vue').default
    UiCheckbox: typeof import('@/components/ui/UiCheckbox.vue').default
    UiChip: typeof import('@/components/ui/UiChip.vue').default
    UiDivider: typeof import('@/components/ui/UiDivider.vue').default
    UiEmpty: typeof import('@/components/ui/UiEmpty.vue').default
    UiGrid: typeof import('@/components/ui/UiGrid.vue').default
    UiInput: typeof import('@/components/ui/UiInput.vue').default
    UiListRow: typeof import('@/components/ui/UiListRow.vue').default
    UiModal: typeof import('@/components/ui/UiModal.vue').default
    UiProgress: typeof import('@/components/ui/UiProgress.vue').default
    UiSection: typeof import('@/components/ui/UiSection.vue').default
    UiSelect: typeof import('@/components/ui/UiSelect.vue').default
    UiSpinner: typeof import('@/components/ui/UiSpinner.vue').default
    UiStack: typeof import('@/components/ui/UiStack.vue').default
    UiText: typeof import('@/components/ui/UiText.vue').default
    UiTitle: typeof import('@/components/ui/UiTitle.vue').default
    UiToggle: typeof import('@/components/ui/UiToggle.vue').default
    UiToolbar: typeof import('@/components/ui/UiToolbar.vue').default
  }
}

export {}
