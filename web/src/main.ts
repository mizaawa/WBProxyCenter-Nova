import { createApp } from 'vue'
import App from './App.vue'
import './design/tokens.css'
import './design/app.css'
const dark = window.matchMedia('(prefers-color-scheme: dark)')
function theme() {
  document.documentElement.dataset.theme = dark.matches ? 'dark' : 'light'
}
theme()
dark.addEventListener('change', theme)
createApp(App).mount('#app')
