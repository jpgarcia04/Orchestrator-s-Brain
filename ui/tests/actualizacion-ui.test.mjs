// El aviso de versión nueva (ADR-0037): la ventana sólo pregunta y pide instalar.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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
const compiled = compileModule(transpile(readFileSync('src/lib/app.svelte.ts', 'utf8')), { filename: 'app.svelte.js', generate: 'client' }).js.code
  .replace('"./nucleo"', JSON.stringify(bridge))
  .replace('"./nombres"', JSON.stringify(names))
  .replace('"./pin-session"', JSON.stringify(pin))
  .replace(/['"]svelte\/internal\/client['"]/, JSON.stringify(pathToFileURL(resolve('node_modules/svelte/src/internal/client/index.js')).href));
globalThis.window = { clearTimeout, setTimeout: (fn, ms) => setTimeout(fn, ms).unref() };
const { app } = await import(data(compiled));

const aviso = { actual: '0.0.3', nueva: '0.0.4', notas: null };

test('una versión nueva queda ofrecida, sin instalar nada', async () => {
  const pedidos = [];
  globalThis.__brainInvoke = async (command, args) => { pedidos.push([command, args]); return aviso; };
  await app.buscarVersionNueva();
  assert.deepEqual(app.versionNueva, aviso);
  assert.deepEqual(pedidos, [['actualizacion_buscar', undefined]]);
});

test('sin red no hay aviso ni mensaje: la app sigue con la que tiene', async () => {
  app.versionNueva = aviso;
  app.aviso = '';
  globalThis.__brainInvoke = async () => { throw new Error('fixture: sin red'); };
  await app.buscarVersionNueva();
  assert.equal(app.versionNueva, null);
  assert.equal(app.aviso, '');
});

test('instalar no hace nada si no se ha ofrecido ninguna versión', async () => {
  app.versionNueva = null;
  let llamado = false;
  globalThis.__brainInvoke = async () => { llamado = true; };
  await app.instalarVersionNueva();
  assert.equal(llamado, false);
});

test('instalar no manda ni versión ni dirección: las decide el núcleo', async () => {
  app.versionNueva = aviso;
  const pedidos = [];
  globalThis.__brainInvoke = async (command, args) => { pedidos.push([command, args]); };
  await app.instalarVersionNueva();
  assert.deepEqual(pedidos, [['actualizacion_instalar', undefined]]);
  assert.equal(app.instalandoVersion, false);
});

test('un segundo clic mientras se descarga no lanza otra instalación', async () => {
  app.versionNueva = aviso;
  let veces = 0;
  let terminar;
  globalThis.__brainInvoke = () => { veces++; return new Promise((r) => { terminar = r; }); };
  const primera = app.instalarVersionNueva();
  await app.instalarVersionNueva();
  assert.equal(veces, 1);
  terminar();
  await primera;
});

test('si la firma no coincide, se dice y el aviso sigue ahí', async () => {
  app.versionNueva = aviso;
  globalThis.__brainInvoke = async () => { throw new Error('fixture: firma no válida'); };
  await app.instalarVersionNueva();
  assert.match(app.aviso, /No se pudo instalar la versión nueva/);
  assert.deepEqual(app.versionNueva, aviso);
  assert.equal(app.instalandoVersion, false);
});
