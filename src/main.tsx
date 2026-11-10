import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// Desativa o menu de contexto do navegador (parece mais um programa nativo).
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!(t instanceof HTMLInputElement) && !(t instanceof HTMLTextAreaElement)) e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
