/**
 * Arrastrar un documento a una colección de la barra lateral (ADR-0033).
 *
 * Con **eventos de puntero**, como el arrastre del mapa, y no con el del
 * navegador (HTML5): en Windows Tauri se queda con ése para lo que sueltas
 * desde fuera (`dragDropEnabled`), y así no hay que apagarlo. Lo que se ve en
 * Chromium es lo mismo que en WebView2.
 *
 * Un gesto es un arrastre a partir de 5 píxeles; menos es un clic, y el clic,
 * el doble clic y Ctrl+clic siguen haciendo lo de siempre. El clic que el
 * navegador manda al soltar se descarta. Los destinos son lo que lleva
 * `data-soltar`: el id de una colección, o `"nueva"`.
 */

import type { Action } from "svelte/action";
import { app } from "./app.svelte";

/** A partir de cuántos píxeles un gesto deja de ser un clic. */
const UMBRAL = 5;

export interface Arrastrable {
  id: string;
  /** Cómo se llama, para la etiqueta que sigue al ratón. */
  titulo: string;
}

/** El destino bajo el ratón: lo que dice su `data-soltar`, o nada. */
function destino(x: number, y: number): string | null {
  const el = document.elementFromPoint(x, y);
  return el?.closest<HTMLElement>("[data-soltar]")?.dataset["soltar"] ?? null;
}

/** El clic que llega detrás de soltar no es un clic: no abre la vista previa. */
function tragarClic(): void {
  const tragar = (e: MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
  };
  window.addEventListener("click", tragar, { capture: true, once: true });
  // Si no llega (soltaste fuera de la ventana), que no se coma el siguiente.
  window.setTimeout(() => window.removeEventListener("click", tragar, { capture: true }), 0);
}

export const arrastrable: Action<HTMLElement, Arrastrable> = (nodo, inicial) => {
  let p = inicial;
  let inicio: { x: number; y: number; puntero: number } | null = null;
  let arrastrando = false;

  const ctrl = (e: PointerEvent | KeyboardEvent) => e.ctrlKey || e.metaKey;

  function mueve(e: PointerEvent): void {
    if (!inicio || e.pointerId !== inicio.puntero) return;
    if (!arrastrando) {
      if (Math.hypot(e.clientX - inicio.x, e.clientY - inicio.y) < UMBRAL) return;
      arrastrando = true;
      document.documentElement.classList.add("arrastrando");
      app.empezarArrastre(p.id, p.titulo, e.clientX, e.clientY, ctrl(e));
    } else if (!app.arrastre) {
      // Se canceló con Esc: el gesto sigue hasta soltar, pero ya no lleva nada.
      document.documentElement.classList.remove("arrastrando");
      return;
    }
    app.moverArrastre(e.clientX, e.clientY, ctrl(e), destino(e.clientX, e.clientY));
  }

  function suelta(e: PointerEvent): void {
    if (!inicio || e.pointerId !== inicio.puntero) return;
    const era = arrastrando;
    terminar();
    if (!era) return;
    tragarClic();
    app.moverArrastre(e.clientX, e.clientY, ctrl(e), destino(e.clientX, e.clientY));
    void app.soltarArrastre();
  }

  function cancela(e: PointerEvent): void {
    if (!inicio || e.pointerId !== inicio.puntero) return;
    const era = arrastrando;
    terminar();
    if (era) app.cancelarArrastre();
  }

  /** Ctrl sin mover el ratón también cambia «Mover» por «Añadir». */
  function tecla(e: KeyboardEvent): void {
    if (arrastrando && (e.key === "Control" || e.key === "Meta")) app.copiarArrastre(ctrl(e));
  }

  /** Nada de texto seleccionado mientras se arrastra. */
  function sinSeleccion(e: Event): void {
    if (arrastrando) e.preventDefault();
  }

  function terminar(): void {
    inicio = null;
    arrastrando = false;
    document.documentElement.classList.remove("arrastrando");
    window.removeEventListener("pointermove", mueve);
    window.removeEventListener("pointerup", suelta);
    window.removeEventListener("pointercancel", cancela);
    window.removeEventListener("keydown", tecla);
    window.removeEventListener("keyup", tecla);
    document.removeEventListener("selectstart", sinSeleccion);
  }

  function abajo(e: PointerEvent): void {
    // Sólo el botón principal del ratón o un lápiz: con el dedo, la lista se
    // desplaza como siempre.
    if (e.button !== 0 || e.pointerType === "touch") return;
    terminar();
    inicio = { x: e.clientX, y: e.clientY, puntero: e.pointerId };
    window.addEventListener("pointermove", mueve);
    window.addEventListener("pointerup", suelta);
    window.addEventListener("pointercancel", cancela);
    window.addEventListener("keydown", tecla);
    window.addEventListener("keyup", tecla);
    document.addEventListener("selectstart", sinSeleccion);
  }

  /** Que una miniatura no empiece el arrastre del navegador por su cuenta. */
  function nativo(e: DragEvent): void {
    e.preventDefault();
  }

  nodo.addEventListener("pointerdown", abajo);
  nodo.addEventListener("dragstart", nativo);

  return {
    update(nuevo: Arrastrable) {
      p = nuevo;
    },
    destroy() {
      if (arrastrando) app.cancelarArrastre();
      terminar();
      nodo.removeEventListener("pointerdown", abajo);
      nodo.removeEventListener("dragstart", nativo);
    },
  };
};
