/**
 * La entrada. Las fuentes van **dentro** de la aplicación: la política de
 * seguridad de Tauri (`default-src 'self'`) no deja pedir nada a internet, y
 * así el aspecto no depende de tener conexión.
 */

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource/instrument-serif/400.css";
import "./estilos/tema.css";

import { mount } from "svelte";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });
