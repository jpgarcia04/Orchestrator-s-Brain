// Marcar un archivo suelto como sensible, o decir que no lo es, desde su ficha.
// Monta la vista previa real con un núcleo sintético. No toca la bóveda.
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

const scratch = mkdtempSync(join(resolve('node_modules'), '.sensibilidad-ui-'));
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
const previewUrl = component(readFileSync('src/componentes/VistaPrevia.svelte', 'utf8'), 'VistaPrevia.svelte');
const harnessUrl = component(`<script>import { app } from '../lib/app.svelte'; import VistaPrevia from './VistaPrevia.svelte';</script>
  {#key app.securityEpoch}{#if app.seleccion}{#key app.seleccion}
    <VistaPrevia id={app.seleccion} consulta={app.consulta} tramo={app.tramo} expectedHash={app.expectedHash} expectedTextHash={app.expectedTextHash} seleccionVersion={app.seleccionVersion} />
  {/key}{/if}{/key}`, 'PreviewHarness.svelte');

const { mount, unmount, flushSync } = await import(pathToFileURL(resolve('node_modules/svelte/src/index-client.js')).href);
const { app } = await import(appUrl);
const { pinSession } = await import(pin);
after(async () => { pinSession.suspend(); await browser.happyDOM.close(); });
const PreviewHarness = (await import(harnessUrl)).default;
const noPin = { pin: false, desbloqueada: false, restante_ms: null };
const locked = { pin: true, desbloqueada: false, restante_ms: null };

const claim = (value, by_user) => ({
  correctable: true, assertion: `a-${value}-${by_user}`, predicate: 'sensitivity_level', label: 'sensibilidad', value,
  value_entity: null, value_time: null, method: by_user ? 'user' : 'inferred', confidence: 0.9, source: 'obrain-guard',
  evidence: 'regla=nombre:identidad', by_user, span: null,
});

// Cada documento sintético: su nivel, lo que se dijo de él y su carpeta marcada.
const DOCS = {
  normal: { sensitivity: 'normal', claims: [], protection: null },
  clasificado: { sensitivity: 'sensitive', claims: [claim('sensitive', false)], protection: null },
  porCarpeta: { sensitivity: 'sensitive', claims: [claim('sensitive', false)], protection: { folder: 'c', label: 'Identidad', level: 'sensitive', own: false } },
  boveda: { sensitivity: 'vault', claims: [claim('vault', false)], protection: null },
};
let security = noPin;
let corregidos = [];

globalThis.__brainInvoke = async (command, args = {}) => {
  if (command === 'seguridad') return security;
  if (command === 'entidad') {
    const d = DOCS[args.id];
    return { id: args.id, label: `${args.id}.txt`, path: '/sintetico', facets: [], sensitivity: d.sensitivity,
      protection: d.protection, by_part: null, parts: [], observed: [], inferred: d.claims, trashed: null, collections: [] };
  }
  if (command === 'previsualizar') {
    const blocked = DOCS[args.id].sensitivity !== 'normal' && security.pin && !security.desbloqueada;
    return { id: args.id, path: '/sintetico', availability: 'local', readable: true, sensitivity: DOCS[args.id].sensitivity,
      kind: blocked ? 'none' : 'text', text: blocked ? null : 'texto sintético', offset: 0, chars: 15, truncated: false,
      highlight: null, image: null, width: 0, height: 0, reason: null, locked: blocked };
  }
  if (command === 'corregir') { corregidos.push(args); return 'nueva'; }
  throw new Error(`comando inesperado: ${command}`);
};

async function settle(ms = 15) {
  if (ms) await new Promise((r) => setTimeout(r, ms));
  await Promise.resolve();
  flushSync();
  await Promise.resolve();
  flushSync();
}
const botones = (t, label) => [...t.querySelectorAll('button')].filter((b) => b.textContent.trim() === label);

async function abrir(id, seguridad = noPin) {
  security = seguridad;
  pinSession.apply(seguridad);
  corregidos = [];
  app.presentacionLista = true;
  const t = document.createElement('div');
  document.body.append(t);
  const c = mount(PreviewHarness, { target: t });
  app.ver(id);
  await settle();
  return { t, cerrar: () => { unmount(c); t.remove(); app.cerrarPanel(); } };
}

test('un archivo normal se marca como sensible él solo', async () => {
  const { t, cerrar } = await abrir('normal');
  assert.match(t.textContent, /Sensibilidad\s*Normal/);
  const [marcar] = botones(t, 'marcar como sensible');
  assert.ok(marcar, 'falta «marcar como sensible»');
  marcar.click();
  await settle();
  assert.deepEqual(corregidos, [{ subject: 'normal', predicate: 'sensitivity_level', object: { kind: 'text', value: 'sensitive' } }]);
  cerrar();
});

test('bajar lo que el clasificador protegió pide confirmación y sale una sola vez', async () => {
  for (const id of ['clasificado', 'porCarpeta']) {
    const { t, cerrar } = await abrir(id);
    const noEs = botones(t, 'no es sensible');
    assert.equal(noEs.length, 1, `${id}: un solo botón`);
    noEs[0].click();
    await settle(0);
    assert.equal(corregidos.length, 0, `${id}: el primer clic no guarda`);
    assert.match(t.textContent, /Dejará de estar protegido/);
    botones(t, 'sí, no es sensible')[0].click();
    await settle();
    assert.deepEqual(corregidos, [{ subject: id, predicate: 'sensitivity_level', object: { kind: 'text', value: 'normal' } }], id);
    cerrar();
  }
});

test('ni lo de bóveda ni lo bloqueado por el PIN se cambian desde aquí', async () => {
  for (const [id, seguridad] of [['boveda', noPin], ['clasificado', locked]]) {
    const { t, cerrar } = await abrir(id, seguridad);
    for (const label of ['marcar como sensible', 'no es sensible']) {
      assert.equal(botones(t, label).length, 0, `${id}: no debería ofrecer «${label}»`);
    }
    cerrar();
  }
});
