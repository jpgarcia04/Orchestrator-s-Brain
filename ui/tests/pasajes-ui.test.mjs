// Monta los componentes reales con un núcleo sintético. No toca la bóveda.
import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve, join } from 'node:path';
import ts from 'typescript';
import { compile, compileModule } from 'svelte/compiler';
import { Window } from 'happy-dom';

const browser = new Window({ url: 'http://localhost/' });
for (const name of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLInputElement', 'HTMLTextAreaElement', 'HTMLMediaElement', 'Node', 'Element', 'Text', 'Comment', 'Document', 'Event', 'MouseEvent', 'localStorage']) {
  Object.defineProperty(globalThis, name, { configurable: true, value: name === 'window' ? browser : browser[name] });
}
globalThis.requestAnimationFrame = (f) => setTimeout(f, 0);

const scratch = mkdtempSync(join(resolve('node_modules'), '.pasajes-ui-'));
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
  export const carpetaDe = (s) => s;
  export const tipoDe = () => 'txt';
  export const tipoLargo = () => 'Texto';
  export const nombreBonito = (s) => s;
  export const diasHasta = () => null;
  export const fecha = () => '';
  export const fechaIso = (s) => s;
  export const plazo = () => '';
  export const sinCodigos = (s) => s;
`);
const appCode = compileModule(transpile(readFileSync('src/lib/app.svelte.ts', 'utf8')), { filename: 'app.svelte.js', generate: 'client' }).js.code
  .replace('"./nucleo"', JSON.stringify(bridge))
  .replace('"./nombres"', JSON.stringify(names))
  .replace('"./pin-session"', JSON.stringify(pin));
const appUrl = module('app', appCode);
const empty = module('Empty', 'export default function Empty() {}');

function component(source, filename) {
  return module(filename.replace('.svelte', ''), compile(source, { filename, generate: 'client' }).js.code
    .replace(/"@lucide\/svelte\/icons\/[^"]+"/g, JSON.stringify(empty))
    .replaceAll('"../lib/app.svelte"', JSON.stringify(appUrl))
    .replaceAll("'../lib/app.svelte'", JSON.stringify(appUrl))
    .replaceAll('"../lib/nucleo"', JSON.stringify(bridge))
    .replaceAll('"../lib/nombres"', JSON.stringify(names))
    .replaceAll('"./Tipo.svelte"', JSON.stringify(empty))
    .replaceAll('"./ElegirColeccion.svelte"', JSON.stringify(empty))
    .replaceAll('"./Apartado.svelte"', () => JSON.stringify(apartado))
    .replaceAll('"../lib/pin-session"', JSON.stringify(pin))
    .replaceAll('"./VistaPrevia.svelte"', () => JSON.stringify(previewUrl))
    .replaceAll("'./VistaPrevia.svelte'", () => JSON.stringify(previewUrl)));
}
const apartado = component('<script>let { children } = $props();</script>{@render children?.()}', 'Apartado.svelte');
const buscadorUrl = component(readFileSync('src/componentes/Buscador.svelte', 'utf8'), 'Buscador.svelte');
const previewUrl = component(readFileSync('src/componentes/VistaPrevia.svelte', 'utf8'), 'VistaPrevia.svelte');
const harnessUrl = component(`<script>import { app } from '../lib/app.svelte'; import VistaPrevia from './VistaPrevia.svelte';</script>
  {#key app.securityEpoch}{#if app.seleccion}{#key app.seleccion}
    <VistaPrevia id={app.seleccion} consulta={app.consulta} tramo={app.tramo} expectedHash={app.expectedHash} expectedTextHash={app.expectedTextHash} seleccionVersion={app.seleccionVersion} />
  {/key}{/if}{/key}`, 'PreviewHarness.svelte');

const { mount, unmount, flushSync } = await import(pathToFileURL(resolve('node_modules/svelte/src/index-client.js')).href);
const { app } = await import(appUrl);
const { pinSession } = await import(pin);
after(async () => { pinSession.suspend(); await browser.happyDOM.close(); });
const Buscador = (await import(buscadorUrl)).default;
const PreviewHarness = (await import(harnessUrl)).default;
const noPin = { pin: false, desbloqueada: false, restante_ms: null };
const locked = { pin: true, desbloqueada: false, restante_ms: null };
const unlocked = { pin: true, desbloqueada: true, restante_ms: 300_000 };
const query = 'El contrato donde decía cuándo terminaba el alquiler';
const passage = { text: 'El alquiler termina el 31 de agosto de 2027.', span: [12, 58], published_hash: 'publicado', text_hash: 'texto' };
let security = noPin;
let previewCalls = [];
let commands = [];
const documentText = 'Ñandú abrió el contrato. Muñoz confirmó el plazo.';
const span = (word) => {
  const start = documentText.indexOf(word);
  return [Buffer.byteLength(documentText.slice(0, start)), Buffer.byteLength(documentText.slice(0, start + word.length))];
};
const first = span('Ñandú');
const second = span('Muñoz');

globalThis.__brainInvoke = async (command, args = {}) => {
  commands.push(command);
  if (command === 'seguridad') return security;
  if (command === 'recientes') return [];
  if (command === 'vistazo') {
    const sensitive = args.query === query;
    return { answer: { hits: [{ entity: sensitive ? 'contrato' : 'normal-doc', label: sensitive ? 'Contrato ficticio.txt' : 'Nota.txt', path: '/sintetico', snippet: sensitive ? 'NO MOSTRAR' : 'fragmento normal', ranks: [['bm25', 1]], sensitivity: sensitive ? 'sensitive' : 'normal', inside: null, passages: sensitive && security.pin && !security.desbloqueada ? [] : [passage] }], semantic: false } };
  }
  if (command === 'entidad') return {
    id: args.id, label: args.id, path: '/sintetico', facets: [], sensitivity: args.id === 'normal-doc' ? 'normal' : 'sensitive',
    protection: null, by_part: null, parts: [{ id: 'parte', label: 'Parte.txt', span: first, paragraphs: 1, of: 1, copies: [], trashed: false }],
    observed: [], inferred: [], trashed: null, collections: [],
  };
  if (command === 'previsualizar') {
    previewCalls.push(args.tramo);
    const blocked = args.id !== 'normal-doc' && security.pin && !security.desbloqueada;
    return { id: args.id, path: '/sintetico', availability: 'local', readable: true, sensitivity: args.id === 'normal-doc' ? 'normal' : 'sensitive',
      kind: blocked ? 'none' : 'text', text: blocked ? null : documentText, offset: 0, chars: documentText.length, truncated: false,
      highlight: blocked ? null : args.tramo, image: null, width: 0, height: 0, reason: null, locked: blocked };
  }
  if (command === 'explorar') return { focus: null, rows: [] };
  throw new Error(`comando inesperado: ${command}`);
};

async function settle(ms = 15) {
  if (ms) await new Promise((r) => setTimeout(r, ms));
  await Promise.resolve();
  flushSync();
  await Promise.resolve();
  flushSync();
}
function button(target, label) {
  const found = [...target.querySelectorAll('button')].find((b) => b.textContent.includes(label));
  assert.ok(found, `falta botón «${label}»`);
  return found;
}
function target() {
  const div = document.createElement('div');
  document.body.append(div);
  return div;
}

test('la consulta del contrato ofrece una acción, oculta la cita y abre su tramo sin PIN', async () => {
  security = noPin;
  pinSession.apply(noPin);
  app.presentacionLista = true;
  app.vista = 'coleccion';
  app.buscarEn = 'coleccion-sintetica';
  app.buscando = true;
  app.semilla = query;
  const host = target();
  const ui = mount(Buscador, { target: host });
  await settle(140);
  assert.match(host.textContent, /Contrato ficticio/, `input=${host.querySelector('input')?.value}; commands=${commands.join(',')}; html=${host.innerHTML}`);
  assert.doesNotMatch(host.textContent, /31 de agosto de 2027|NO MOSTRAR/);
  button(host, 'Consultar pasajes relacionados').click();
  await settle();
  assert.match(host.textContent, /31 de agosto de 2027/);
  button(host, 'Ver tramo en el documento').click();
  await settle();
  assert.deepEqual(app.tramo, passage.span);
  assert.equal(app.expectedHash, passage.published_hash);
  assert.equal(app.expectedTextHash, passage.text_hash);
  await unmount(ui);
  host.remove();
});

test('la búsqueda convencional sigue abriendo el resultado sin consultar pasajes', async () => {
  security = noPin;
  pinSession.apply(noPin);
  app.presentacionLista = true;
  app.vista = 'coleccion';
  app.buscarEn = 'coleccion-sintetica';
  app.buscando = true;
  app.semilla = 'nota';
  const host = target();
  const ui = mount(Buscador, { target: host });
  await settle(140);
  assert.match(host.textContent, /Nota.txt/);
  assert.doesNotMatch(host.textContent, /31 de agosto de 2027/);
  button(host, 'Nota.txt').click();
  await settle();
  assert.equal(app.seleccion, 'normal-doc');
  assert.equal(app.tramo, null);
  assert.equal(app.buscando, false);
  await unmount(ui);
  host.remove();
});

test('el PIN bloquea las citas, el desbloqueo explícito las revela y el bloqueo las retira', async () => {
  security = locked;
  pinSession.apply(locked);
  app.presentacionLista = true;
  app.buscando = true;
  app.semilla = query;
  const host = target();
  const ui = mount(Buscador, { target: host });
  await settle(140);
  assert.doesNotMatch(host.textContent, /31 de agosto de 2027/);
  button(host, 'Desbloquear y consultar pasajes').click();
  await settle();
  assert.equal(app.pidiendoPin, true);
  security = unlocked;
  pinSession.apply(unlocked);
  await app.pinResuelto(true);
  await settle(140);
  assert.match(host.textContent, /31 de agosto de 2027/);
  security = locked;
  pinSession.apply(locked);
  await settle();
  assert.doesNotMatch(host.textContent, /31 de agosto de 2027/);
  security = unlocked;
  pinSession.apply(unlocked);
  await settle(140);
  assert.doesNotMatch(host.textContent, /31 de agosto de 2027/, 'un desbloqueo posterior no repite la revelación');
  await unmount(ui);
  host.remove();
});

test('la vista previa reemplaza una marca anterior y sigue los bytes UTF-8 en el mismo documento', async () => {
  security = noPin;
  pinSession.apply(noPin);
  app.presentacionLista = true;
  app.ver('normal-doc', '', first);
  const host = target();
  const ui = mount(PreviewHarness, { target: host });
  await settle();
  assert.equal(host.querySelector('mark')?.textContent, 'Ñandú', `commands=${commands.join(',')}; html=${host.innerHTML}`);
  button(host, '¿por qué?').click();
  await settle();
  app.ver('normal-doc', '', second, 'publicado', 'texto');
  await settle();
  assert.equal(host.querySelector('mark')?.textContent, 'Muñoz');
  app.ver('normal-doc', '', first, 'publicado', 'texto');
  await settle();
  assert.equal(host.querySelector('mark')?.textContent, 'Ñandú');
  app.ver('normal-doc', '', second, 'publicado', 'texto');
  await settle();
  assert.equal(host.querySelector('mark')?.textContent, 'Muñoz');
  assert.deepEqual(previewCalls.slice(-3), [second, first, second]);
  await unmount(ui);
  host.remove();
});

test('la vista previa sensible exige Mostrar en cada selección, con o sin PIN', async () => {
  security = noPin;
  pinSession.apply(noPin);
  app.presentacionLista = true;
  app.ver('contrato', '', first);
  const host = target();
  const ui = mount(PreviewHarness, { target: host });
  await settle();
  assert.equal(host.querySelector('pre'), null);
  assert.doesNotMatch(host.textContent, /Ñandú abrió/);
  assert.match(host.textContent, /Mostrar/, `commands=${commands.join(',')}; html=${host.innerHTML}`);
  button(host, 'Mostrar').click();
  await settle();
  assert.equal(host.querySelector('mark')?.textContent, 'Ñandú');
  app.ver('contrato', '', second);
  await settle();
  assert.equal(host.querySelector('pre'), null);
  button(host, 'Mostrar').click();
  await settle();
  assert.equal(host.querySelector('mark')?.textContent, 'Muñoz');
  security = locked;
  pinSession.apply(locked);
  await settle();
  assert.equal(host.querySelector('pre'), null);
  assert.match(host.textContent, /Protegido con tu PIN/);
  security = unlocked;
  pinSession.apply(unlocked);
  await settle();
  assert.equal(host.querySelector('pre'), null);
  button(host, 'Mostrar').click();
  await settle();
  assert.equal(host.querySelector('mark')?.textContent, 'Muñoz');
  app.ver('normal-doc', '', first);
  await settle();
  app.ver('contrato', '', first);
  await settle();
  assert.equal(host.querySelector('pre'), null, 'cambiar de documento retira la revelación anterior');
  await unmount(ui);
  host.remove();
});
