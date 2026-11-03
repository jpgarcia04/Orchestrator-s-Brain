// Pruebas unitarias de la integración UI para retirar bibliotecas registradas.
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
const events = data('export const listen = async (name, cb) => { (globalThis.__brainListeners ??= {})[name] = cb; return () => {}; };');
const bridge = data(transpile(readFileSync('src/lib/nucleo.ts', 'utf8'))
  .replace('"@tauri-apps/api/core"', JSON.stringify(core))
  .replace('"@tauri-apps/api/event"', JSON.stringify(events))
  .replace('"./pin-session"', JSON.stringify(pin)));
const names = data('export const tituloDe = (s) => ({ titulo: s });');
const source = readFileSync('src/lib/app.svelte.ts', 'utf8');
const compiled = compileModule(transpile(source), { filename: 'app.svelte.js', generate: 'client' }).js.code
  .replace('"./nucleo"', JSON.stringify(bridge))
  .replace('"./nombres"', JSON.stringify(names))
  .replace('"./pin-session"', JSON.stringify(pin))
  .replace(/['"]svelte\/internal\/client['"]/, JSON.stringify(pathToFileURL(resolve('node_modules/svelte/src/internal/client/index.js')).href));
globalThis.window = { clearTimeout, setTimeout: (fn, ms) => setTimeout(fn, ms).unref() };
const { app } = await import(data(compiled));
const { retirarBiblioteca } = await import(bridge);

test('retirarBiblioteca invoca el comando seguro de Tauri con la ruta indicada', async () => {
  let invokedCommand = null;
  let invokedArgs = null;
  globalThis.__brainInvoke = async (command, args) => {
    invokedCommand = command;
    invokedArgs = args;
    return {
      ruta: "C:\\Usuarios\\Pruebas\\Videos\\Captures",
      documentos_retirados: 3,
      documentos_conservados: 0,
      vigilancia_desactivada: true,
      aviso_vigilancia: null,
    };
  };

  const informe = await retirarBiblioteca("C:\\Usuarios\\Pruebas\\Videos\\Captures");
  assert.equal(invokedCommand, "retirar_biblioteca");
  assert.deepEqual(invokedArgs, { ruta: "C:\\Usuarios\\Pruebas\\Videos\\Captures" });
  assert.equal(informe.documentos_retirados, 3);
  assert.equal(informe.documentos_conservados, 0);
  assert.equal(informe.vigilancia_desactivada, true);
  assert.equal(informe.aviso_vigilancia, null);
});

test('retirarBiblioteca transmite correctamente fallos o advertencias de vigilancia', async () => {
  globalThis.__brainInvoke = async () => ({
    ruta: "C:\\Vault\\Personal",
    documentos_retirados: 2,
    documentos_conservados: 1,
    vigilancia_desactivada: false,
    aviso_vigilancia: "Error simulado al desuscribir del monitor de Windows",
  });

  const informe = await retirarBiblioteca("C:\\Vault\\Personal");
  assert.equal(informe.vigilancia_desactivada, false);
  assert.equal(informe.aviso_vigilancia, "Error simulado al desuscribir del monitor de Windows");
});

test('app.retirarCarpeta refresca el estado y redirige al inicio si la vista es carpeta', async () => {
  let refrescado = false;
  globalThis.__brainInvoke = async (command, args) => {
    if (command === 'retirar_biblioteca') {
      return {
        ruta: "C:\\Usuarios\\Pruebas\\Descargas",
        documentos_retirados: 5,
        documentos_conservados: 2,
        vigilancia_desactivada: true,
        aviso_vigilancia: null,
      };
    }
    if (command === 'estado') {
      refrescado = true;
      return {
        libraries: [],
        entities: 0,
        assertions: 0,
        assertions_current: 0,
        predicates: 0,
        chunks: 0,
        vectors: 0,
        storage_bytes: 0,
        indexing: false,
        cerrada: true,
      };
    }
    return null;
  };

  app.vista = "carpeta";
  const informe = await app.retirarCarpeta("C:\\Usuarios\\Pruebas\\Descargas");

  assert.ok(informe !== null, "Debe retornar el informe de retiro");
  assert.equal(informe.documentos_retirados, 5);
  assert.equal(informe.documentos_conservados, 2);
  assert.equal(refrescado, true, "Debe haber llamado a app.refrescar()");
  assert.equal(app.vista, "inicio", "Debe haber vuelto a la vista de inicio");
});

test('app.retirarCarpeta maneja errores sin lanzar excepciones no controladas', async () => {
  let avisoEmitido = null;
  app.avisar = (msg) => { avisoEmitido = msg; };

  globalThis.__brainInvoke = async (command) => {
    if (command === 'retirar_biblioteca') {
      throw new Error("No existe la biblioteca");
    }
    return null;
  };

  const informe = await app.retirarCarpeta("C:\\Ruta\\Inexistente");
  assert.equal(informe, null);
  assert.ok(avisoEmitido && avisoEmitido.includes("No existe la biblioteca"), "Debe notificar el error");
});
