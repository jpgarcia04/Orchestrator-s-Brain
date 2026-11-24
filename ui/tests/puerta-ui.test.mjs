// El interruptor de la puerta de Aetheris (ADR-0038, ADR-0039 paso 4), montado
// con un núcleo sintético. No toca ninguna base.
import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve, join } from 'node:path';
import ts from 'typescript';
import { compile, compileModule } from 'svelte/compiler';
import { Window } from 'happy-dom';

const browser = new Window({ url: 'http://localhost/' });
for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLInputElement', 'Node', 'Element', 'Text', 'Comment', 'Document', 'Event', 'MouseEvent', 'localStorage']) {
  Object.defineProperty(globalThis, name, { configurable: true, value: name === 'window' ? browser : browser[name] });
}
globalThis.requestAnimationFrame = (f) => setTimeout(f, 0);

const scratch = mkdtempSync(join(resolve('node_modules'), '.puerta-ui-'));
process.on('exit', () => { try { rmSync(scratch, { recursive: true, force: true }); } catch { /* temporal propio si Windows lo retiene */ } });
const module = (name, code) => {
  const path = join(scratch, `${name}.mjs`);
  writeFileSync(path, code);
  return pathToFileURL(path).href;
};
const transpile = (code) => ts.transpileModule(code, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const pin = pathToFileURL(resolve('src/lib/pin-session.ts')).href;
const core = module('tauri-core', 'export const invoke = (...args) => globalThis.__brainInvoke(...args);');
const events = module('tauri-events', 'export const listen = async () => () => {};');
const bridge = module('nucleo', transpile(readFileSync('src/lib/nucleo.ts', 'utf8'))
  .replace('"@tauri-apps/api/core"', JSON.stringify(core))
  .replace('"@tauri-apps/api/event"', JSON.stringify(events))
  .replace('"./pin-session"', JSON.stringify(pin)));
const names = module('nombres', `
  export const tituloDe = (s) => ({ titulo: s, fecha: null });
  export const frase = (e) => String(e).replace(/^Error:\\s*/, '');
`);
const appUrl = module('app', compileModule(transpile(readFileSync('src/lib/app.svelte.ts', 'utf8')), { filename: 'app.svelte.js', generate: 'client' }).js.code
  .replace('"./nucleo"', JSON.stringify(bridge))
  .replace('"./nombres"', JSON.stringify(names))
  .replace('"./pin-session"', JSON.stringify(pin)));
const svelteClient = pathToFileURL(resolve('node_modules/svelte/src/index-client.js')).href;
// `onMount` tiene que ser el del cliente: Node resolvería `svelte` a la versión
// de servidor, donde no hace nada.
const puertaUrl = module('Puerta', compile(readFileSync('src/componentes/Puerta.svelte', 'utf8'), { filename: 'Puerta.svelte', generate: 'client' }).js.code
  .replaceAll(/from ['"]svelte['"]/g, `from ${JSON.stringify(svelteClient)}`)
  .replaceAll('"../lib/app.svelte"', JSON.stringify(appUrl))
  .replaceAll('"../lib/nucleo"', JSON.stringify(bridge))
  .replaceAll('"../lib/nombres"', JSON.stringify(names)));

const { mount, unmount, flushSync } = await import(svelteClient);
const { app } = await import(appUrl);
const Puerta = (await import(puertaUrl)).default;
const { pinSession } = await import(pin);
// El desbloqueo sintético arma la caducidad del PIN: se suelta al acabar.
after(async () => { pinSession.suspend(); await browser.happyDOM.close(); });

const BLOQUEADO = 'está protegido con tu PIN: desbloquéalo para verlo o abrirlo';
const ORDEN = ['apagada', 'solo_localizar', 'localizar_y_leer'];

/**
 * Un núcleo que guarda la posición como el de verdad: sin ajuste, apagada;
 * abrir más con el PIN bloqueado falla; cerrar, nunca.
 */
function nucleo({ posicion = 'apagada', pin = false, desbloqueada = false, leer = null } = {}) {
  const n = { posicion, pin, desbloqueada, llamadas: [] };
  globalThis.__brainInvoke = async (command, args = {}) => {
    n.llamadas.push([command, args]);
    if (command === 'puerta_posicion') {
      if (leer) throw new Error(leer);
      return n.posicion;
    }
    if (command === 'puerta_poner') {
      if (ORDEN.indexOf(args.posicion) > ORDEN.indexOf(n.posicion) && n.pin && !n.desbloqueada) throw BLOQUEADO;
      n.posicion = args.posicion;
      return null;
    }
    throw new Error(`comando inesperado: ${command}`);
  };
  app.seguridad = { pin, desbloqueada, restante_ms: null };
  return n;
}

async function settle(ms = 10) {
  flushSync();
  await new Promise((r) => setTimeout(r, ms));
  flushSync();
  await Promise.resolve();
  flushSync();
}
function opcion(target, titulo) {
  const found = [...target.querySelectorAll('[role="radio"]')].find((b) => b.textContent.includes(titulo));
  assert.ok(found, `falta la opción «${titulo}»`);
  return found;
}
const elegida = (target) => [...target.querySelectorAll('[role="radio"]')]
  .filter((b) => b.getAttribute('aria-checked') === 'true')
  .map((b) => b.querySelector('.titulo').textContent);
async function montar() {
  const host = document.createElement('div');
  document.body.append(host);
  const ui = mount(Puerta, { target: host });
  await settle();
  return { host, ui };
}

test('enseña la posición que dice el núcleo y dice qué sale en cada una', async () => {
  nucleo({ posicion: 'solo_localizar' });
  const { host, ui } = await montar();
  assert.deepEqual(elegida(host), ['Sólo localizar']);
  const texto = host.textContent;
  // Lo que el ADR-0038 pide que el interruptor diga sin rodeos.
  assert.match(texto, /Los nombres de carpetas y colecciones también salen/);
  assert.match(texto, /nunca lo sensible, lo de bóveda ni lo que está sin\s+clasificar/);
  assert.match(opcion(host, 'Sólo localizar').textContent, /Ningún texto de dentro/);
  assert.match(opcion(host, 'Localizar y leer lo normal').textContent, /pasajes literales/);
  assert.match(texto, /todavía no trae el programa de la puerta/);
  await unmount(ui);
});

test('si no puede leer el ajuste no marca ninguna posición ni deja elegir', async () => {
  const n = nucleo({ leer: 'la base está cerrada' });
  const { host, ui } = await montar();
  assert.deepEqual(elegida(host), []);
  assert.match(host.textContent, /No se pudo leer cómo está la puerta: la base está cerrada/);
  opcion(host, 'Localizar y leer lo normal').click();
  await settle();
  assert.ok(!n.llamadas.some(([c]) => c === 'puerta_poner'), 'no escribe a ciegas');
  await unmount(ui);
});

test('sin PIN, elegir mueve el interruptor y enseña lo que quedó guardado', async () => {
  const n = nucleo();
  const { host, ui } = await montar();
  assert.deepEqual(elegida(host), ['Apagada']);
  opcion(host, 'Localizar y leer lo normal').click();
  await settle();
  assert.deepEqual(n.llamadas.filter(([c]) => c === 'puerta_poner'), [['puerta_poner', { posicion: 'localizar_y_leer' }]]);
  assert.equal(n.posicion, 'localizar_y_leer');
  assert.deepEqual(elegida(host), ['Localizar y leer lo normal']);
  await unmount(ui);
});

test('con el PIN bloqueado, abrir pide el PIN y no abre si se cancela', async () => {
  const n = nucleo({ pin: true });
  const { host, ui } = await montar();
  opcion(host, 'Sólo localizar').click();
  await settle();
  assert.equal(app.pidiendoPin, true, 'pide el PIN');
  await app.pinResuelto(false);
  await settle();
  assert.equal(n.posicion, 'apagada');
  assert.deepEqual(elegida(host), ['Apagada']);
  await unmount(ui);
});

test('con el PIN bloqueado, abrir vuelve a intentarlo tras escribir el PIN', async () => {
  const n = nucleo({ pin: true });
  const { host, ui } = await montar();
  opcion(host, 'Sólo localizar').click();
  await settle();
  assert.equal(app.pidiendoPin, true);
  // La ventana del PIN lo comprobó: ya está desbloqueada.
  n.desbloqueada = true;
  globalThis.__brainInvoke = ((orig) => async (command, args) =>
    command === 'seguridad' ? { pin: true, desbloqueada: true, restante_ms: 300_000 } : orig(command, args))(globalThis.__brainInvoke);
  await app.pinResuelto(true);
  await settle();
  assert.equal(n.posicion, 'solo_localizar');
  assert.deepEqual(elegida(host), ['Sólo localizar']);
  await unmount(ui);
});

test('con el PIN bloqueado, cerrar no pide nada', async () => {
  const n = nucleo({ posicion: 'localizar_y_leer', pin: true });
  const { host, ui } = await montar();
  opcion(host, 'Apagada').click();
  await settle();
  assert.equal(app.pidiendoPin, false);
  assert.equal(n.posicion, 'apagada');
  assert.deepEqual(elegida(host), ['Apagada']);
  await unmount(ui);
});
