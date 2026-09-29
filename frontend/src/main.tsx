import React from 'react'
import ReactDOM from 'react-dom/client'

import App from './App'
import './styles.css'

const host = document.getElementById('root')
if (!host) {
  throw new Error('в index.html нет элемента #root')
}

ReactDOM.createRoot(host).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
