import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

// Keep the app sized to the visible area so the bottom search bar sits on top of the iOS keyboard.
const root = document.documentElement
const vv = window.visualViewport
function syncViewport() {
  const height = vv ? vv.height : window.innerHeight
  root.style.setProperty('--app-height', `${height}px`)
  root.style.setProperty('--app-top', `${vv ? vv.offsetTop : 0}px`)
  root.classList.toggle('keyboard', window.innerHeight - height > 120)
}
syncViewport()
vv?.addEventListener('resize', syncViewport)
vv?.addEventListener('scroll', syncViewport)
window.addEventListener('resize', syncViewport)

if ('serviceWorker' in navigator && import.meta.env.PROD) {
  navigator.serviceWorker.register('./sw.js').catch(() => {})
}

mount(App, { target: document.getElementById('app')! })
