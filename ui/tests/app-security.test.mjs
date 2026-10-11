// Ejecuta el estado real de Svelte y el transporte real con un núcleo sintético.
// OBRAIN_BASELINE=1 permite repetir las dos regresiones sobre c183f3c.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import ts from 'typescript';
import { compileModule } from 'svelte/compiler';

const data = (code) => `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
const transpile = (code) => ts.transpileModule(code, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const pin = pathToFileURL(resolve('src/lib/pin-session.ts')).href;
const core = data('export const invoke = (...args) => globalThis.__brainInvoke(...args);');
const events = data('export const listen = async () => () => {};');
const bridge = data(transpile(readFileSync('src/lib/nucleo.ts', 'utf8'))
  .replace('"@tauri-apps/api/core"', JSON.stringify(core))
  .replace('"@tauri-apps/api/event"', JSON.stringify(events))
  .replace('"./pin-session"', JSON.stringify(pin)));
const names = data('export const tituloDe = (s) => ({ titulo: s });');
const source = process.env.OBRAIN_BASELINE
  ? execFileSync('git', ['show', 'c183f3c:ui/src/lib/app.svelte.ts'], { encoding: 'utf8' })
  : readFileSync('src/lib/app.svelte.ts', 'utf8');
const compiled = compileModule(transpile(source), { filename: 'app.svelte.js', generate: 'client' }).js.code
  .replace('"./nucleo"', JSON.stringify(bridge))
  .replace('"./nombres"', JSON.stringify(names))
  .replace('"./pin-session"', JSON.stringify(pin))
  .replace(/['"]svelte\/internal\/client['"]/, JSON.stringify(pathToFileURL(resolve('node_modules/svelte/src/internal/client/index.js')).href));
globalThis.window = { clearTimeout, setTimeout };
const { app } = await import(data(compiled));
const { pinSession } = await import(pin);

test('un fallo de seguridad no se interpreta como PIN ausente', async () => {
  globalThis.__brainInvoke = async () => { throw new Error('fixture: seguridad inaccesible'); };
  await app.recargarSeguridad();
  assert.equal(await app.pedirPin(), false);
});

test('bloquear revoca el cliente antes de esperar al comando y a su consulta', async () => {
  pinSession.finishChange();
  globalThis.__brainInvoke = async () => ({ pin: true, desbloqueada: true, restante_ms: 300_000 });
  await app.recargarSeguridad();
  const epoch = app.securityEpoch;
  let finish;
  globalThis.__brainInvoke = (command) => command === 'pin_bloquear'
    ? new Promise((r) => { finish = r; })
    : Promise.resolve({ pin: true, desbloqueada: false, restante_ms: null });
  const blocking = app.bloquear();
  assert.ok(app.securityEpoch > epoch, 'no invalidó el contenido antes del await');
  assert.equal(app.seguridad, null);
  finish();
  await blocking;
  assert.equal(app.seguridad.desbloqueada, false);
});

test('una indexación retira la selección abierta y rechaza respuestas anteriores', async () => {
  const { explorar, previsualizar } = await import(bridge);
  pinSession.finishChange();
  pinSession.apply({ pin: false, desbloqueada: false, restante_ms: null });
  let finishExplore;
  let finishPreview;
  globalThis.__brainInvoke = (command) => {
    if (command === 'explorar') return new Promise((resolve) => { finishExplore = resolve; });
    if (command === 'previsualizar') return new Promise((resolve) => { finishPreview = resolve; });
    return Promise.resolve({ pin: false, desbloqueada: false, restante_ms: null });
  };
  const anterior = explorar('carpeta-vieja');
  const contenidoAnterior = previsualizar('documento-pendiente');
  app.presentacionLista = true;
  app.seleccion = 'documento-pendiente';
  app.listado = { focus: 'carpeta-vieja', rows: [{ id: 'documento-pendiente' }] };
  app.empezoIndexado();
  assert.equal(app.presentacionLista, false);
  assert.equal(app.seleccion, null);
  assert.equal(app.listado, null);
  finishExplore({ focus: 'carpeta-vieja', rows: [{ id: 'documento-pendiente' }] });
  finishPreview({ id: 'documento-pendiente', kind: 'text', text: 'contenido antiguo' });
  await assert.rejects(anterior, /vista cambió/);
  await assert.rejects(contenidoAnterior, /vista cambió/);
  assert.equal(app.listado, null);
});

test('la carpeta se vuelve a leer tras indexar sin reabrir la selección anterior', async () => {
  const requested = [];
  globalThis.__brainInvoke = async (command, args) => {
    requested.push([command, args]);
    if (command === 'estado') return { cerrada: false, indexing: false, libraries: [] };
    if (command === 'seguridad') return { pin: false, desbloqueada: false, restante_ms: null };
    if (command === 'explorar') return { focus: args.foco, rows: [] };
    if (command === 'colecciones') return [];
    throw new Error(`comando inesperado: ${command}`);
  };
  app.presentacionLista = true;
  app.vista = 'carpeta';
  app.listado = { focus: 'carpeta-vigente', rows: [{ id: 'documento-viejo' }] };
  app.seleccion = 'documento-viejo';
  app.empezoIndexado();
  await app.refrescar();
  assert.equal(app.presentacionLista, true);
  assert.equal(app.listado.focus, 'carpeta-vigente');
  assert.equal(app.seleccion, null);
  assert.ok(requested.some(([command, args]) => command === 'explorar' && args.foco === 'carpeta-vigente'));
});
