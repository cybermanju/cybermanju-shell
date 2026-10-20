// CyberManju OS — Main (src/main.ts).
// Role: module (see exports below).
// Exports: module root (see items below).
// Integrates: ./…, vue, pinia.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Main Entry
import './utils/deploymentStorage'
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import './assets/main.css'
import './assets/ui.css'
import { installUi } from './ui/install'
import { useTheme } from './composables/useTheme'

const app = createApp(App)
app.use(createPinia())
app.use(installUi)

// Publish the persisted theme before first paint (also runs at module load
// for the pre-Vue document state).
useTheme()

app.mount('#app')
