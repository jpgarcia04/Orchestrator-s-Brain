// Pruebas unitarias de la integración UI para respaldos y restauración controlada.
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
const {
  respaldoCrear,
  respaldoSeleccionar,
  respaldoComprobar,
  respaldoEnsayar,
  restauracionPlanificar,
  restauracionEjecutar,
  reanudarReindexado,
  motivoReindexadoPausado,
} = await import(bridge);

test('respaldoCrear invoca el comando seguro de Tauri sin enviar rutas', async () => {
  let invokedCommand = null;
  let invokedArgs = null;
  globalThis.__brainInvoke = async (command, args) => {
    invokedCommand = command;
    invokedArgs = args;
    return {
      carpeta: "C:\\Respaldos\\obrain-respaldo-123456",
      nombre_carpeta: "obrain-respaldo-123456",
      manifiesto: {
        version: 1,
        formato: "obrain-respaldo-v1",
        creado_en_ms: 1729600000000,
        base: { nombre: "brain.obrain", bytes: 10240, blake3: "abcdef" },
        llave: { nombre: "brain.obrain.llave", bytes: 512, blake3: "123456", huella_recuperacion: "7890" },
        instantanea: {
          esquema_version: 1,
          entidades: 42,
          aserciones: 100,
          aserciones_vigentes: 95,
          predicados: 5,
          fragmentos: 80,
          vectores: 42,
          publicaciones_listas: 42,
          publicaciones_pendientes: 0,
          tiene_pin: false,
          tiene_recuperacion_interna: true,
          huella_contenido: "huella123",
        },
      },
    };
  };

  const res = await respaldoCrear();
  assert.equal(invokedCommand, "respaldo_crear");
  assert.deepEqual(invokedArgs, undefined);
  assert.equal(res.nombre_carpeta, "obrain-respaldo-123456");
  assert.equal(res.manifiesto.instantanea.entidades, 42);
});

test('respaldoSeleccionar y comprobación transmiten clave opcional sin exponer rutas', async () => {
  const calls = [];
  globalThis.__brainInvoke = async (command, args) => {
    calls.push({ command, args });
    if (command === "respaldo_seleccionar") {
      return {
        carpeta: "E:\\Backups\\obrain-respaldo-001",
        nombre_carpeta: "obrain-respaldo-001",
        manifiesto: {
          version: 1,
          formato: "obrain-respaldo-v1",
          creado_en_ms: 1729600000000,
          base: { nombre: "brain.obrain", bytes: 20480, blake3: "basehash" },
          llave: { nombre: "brain.obrain.llave", bytes: 512, blake3: "keyhash", huella_recuperacion: "rechash" },
          instantanea: {
            esquema_version: 1,
            entidades: 10,
            aserciones: 20,
            aserciones_vigentes: 20,
            predicados: 2,
            fragmentos: 15,
            vectores: 10,
            publicaciones_listas: 10,
            publicaciones_pendientes: 0,
            tiene_pin: false,
            tiene_recuperacion_interna: true,
            huella_contenido: "h1",
          },
        },
        requiere_clave: true,
      };
    }
    if (command === "respaldo_comprobar") {
      return { version: 1 };
    }
    if (command === "respaldo_ensayar") {
      return {
        recuperada_con_clave: true,
        documentos_en_boveda: 10,
        bibliotecas_presentes: 2,
        bibliotecas_ausentes_en_disco: ["D:\\Perdida"],
      };
    }
  };

  const sel = await respaldoSeleccionar("clave-recuperacion-secreta");
  assert.equal(sel.requiere_clave, true);
  assert.equal(calls[0].command, "respaldo_seleccionar");
  assert.equal(calls[0].args.clave, "clave-recuperacion-secreta");

  await respaldoComprobar("clave-recuperacion-secreta");
  assert.equal(calls[1].command, "respaldo_comprobar");
  assert.equal(calls[1].args.clave, "clave-recuperacion-secreta");

  const ensayo = await respaldoEnsayar("clave-recuperacion-secreta");
  assert.equal(calls[2].command, "respaldo_ensayar");
  assert.equal(ensayo.documentos_en_boveda, 10);
  assert.equal(ensayo.bibliotecas_ausentes_en_disco.length, 1);
});

test('restauracionEjecutar exige confirmación explícita y planifica previamente', async () => {
  const calls = [];
  globalThis.__brainInvoke = async (command, args) => {
    calls.push({ command, args });
    if (command === "restauracion_planificar") {
      return {
        manifiesto_respaldo: { version: 1 },
        requiere_clave_recuperacion: false,
        advertencias: ["Se sustituirá el perfil actual"],
        bibliotecas_respaldo: [{ ruta: "C:\\Docs", existe_en_disco: true }],
      };
    }
    if (command === "restauracion_ejecutar") {
      if (!args.confirmar) {
        throw new Error("NoAutorizado: confirmación requerida");
      }
      return {
        db_destino: "C:\\Users\\test\\brain.obrain",
        salvaguarda_creada: "C:\\Users\\test\\salvaguardas\\salvaguarda-123",
        reindexado_en_pausa: true,
      };
    }
  };

  const plan = await restauracionPlanificar(null);
  assert.equal(plan.advertencias.length, 1);
  assert.equal(calls[0].command, "restauracion_planificar");

  await assert.rejects(
    async () => await restauracionEjecutar(false, null),
    /NoAutorizado/
  );

  const res = await restauracionEjecutar(true, null);
  assert.equal(res.reindexado_en_pausa, true);
  assert.ok(res.salvaguarda_creada.includes("salvaguarda"));
});

test('la reindexación pausada se refleja en la seguridad y reanudarSincronizacion la reactiva', async () => {
  let reanudado = false;
  globalThis.__brainInvoke = async (command) => {
    if (command === "seguridad") {
      return {
        cifrada: true,
        proteccion: "dpapi",
        recuperacion_pendiente: false,
        pin: false,
        desbloqueada: true,
        espera_ms: 0,
        restante_ms: null,
        reindexado_pausado: !reanudado,
      };
    }
    if (command === "reanudar_reindexado") {
      reanudado = true;
      return;
    }
    if (command === "motivo_reindexado_pausado") {
      return reanudado ? null : "restauración de respaldo reciente";
    }
    return {};
  };

  await app.recargarSeguridad();
  assert.equal(app.seguridad.reindexado_pausado, true);

  const motivo = await motivoReindexadoPausado();
  assert.equal(motivo, "restauración de respaldo reciente");

  await app.reanudarSincronizacion();
  assert.equal(reanudado, true);
  assert.equal(app.seguridad.reindexado_pausado, false);
});

test('una pausa preventiva no muestra falso error de lectura o biblioteca ausente, pero conserva errores reales al reanudar', async () => {
  let pausado = true;
  let anadirInvocado = false;
  globalThis.__brainInvoke = async (command) => {
    if (command === "estado") {
      return {
        db: "C:\\Users\\test\\brain.obrain",
        cerrada: false,
        semantic: false,
        entities: 20,
        assertions: 200,
        libraries: ["C:\\Docs"],
        indexing: false,
        ocr: null,
      };
    }
    if (command === "seguridad") {
      return {
        cifrada: true,
        proteccion: "dpapi",
        recuperacion_pendiente: false,
        pin: false,
        desbloqueada: true,
        espera_ms: 0,
        restante_ms: null,
        reindexado_pausado: pausado,
      };
    }
    if (command === "explorar") {
      return { focus: null, label: "Bóveda", path: "", trail: [], rows: [], folders: 0, files: 0, protection: null };
    }
    if (command === "colecciones") {
      return [];
    }
    if (command === "actualizar") {
      return true;
    }
    if (command === "anadir_biblioteca") {
      anadirInvocado = true;
      return true;
    }
    return {};
  };

  await app.arrancar();
  app.aviso = "";

  // 1. Pasada automática al arrancar mientras la sincronización está en pausa: no debe emitir falso error.
  await globalThis.__brainListeners["indexado"]({
    payload: {
      report: {
        libraries: [],
        missing: [],
        errors: [],
        paused: "restauración de respaldo reciente",
        deferred: [],
        files_seen: 0,
        files_new: 0,
        edges_new: 0,
        documents_read: 0,
        chunks_new: 0,
        pending: 3,
        ocr: 0,
        sensitive: 0,
        vault: 0,
        inherited: 0,
        expiring: 0,
        trashed: 0,
        returned: 0,
        forgotten: 0,
        vectors_new: 0,
        semantic: false,
        elapsed_ms: 1,
      },
      error: null,
      live: false,
    },
  });
  assert.equal(app.aviso, "");

  // 2. Pulsar «Actualizar» o «Añadir carpeta» durante la pausa informa de la pausa sin fingir error de lectura.
  await app.actualizar();
  assert.match(app.aviso, /sincronización está en pausa/i);
  assert.doesNotMatch(app.aviso, /falta una biblioteca|error de lectura/i);

  app.aviso = "";
  await app.anadirCarpeta();
  assert.equal(anadirInvocado, false);
  assert.match(app.aviso, /sincronización está en pausa/i);

  // 3. Una vez reanudada la sincronización, una biblioteca realmente ausente sí muestra el aviso de aplazamiento.
  pausado = false;
  await app.recargarSeguridad();
  app.aviso = "";
  await globalThis.__brainListeners["indexado"]({
    payload: {
      report: {
        libraries: [],
        missing: ["D:\\BibliotecaDesconectada"],
        errors: [],
        paused: null,
        deferred: [],
        files_seen: 0,
        files_new: 0,
        edges_new: 0,
        documents_read: 0,
        chunks_new: 0,
        pending: 0,
        ocr: 0,
        sensitive: 0,
        vault: 0,
        inherited: 0,
        expiring: 0,
        trashed: 0,
        returned: 0,
        forgotten: 0,
        vectors_new: 0,
        semantic: false,
        elapsed_ms: 2,
      },
      error: null,
      live: false,
    },
  });
  assert.match(app.aviso, /falta una biblioteca o hubo un error de lectura/);
});

test('el indicador .punto-aviso en Seguridad.svelte mantiene posición limpia sin margin-left duplicado', () => {
  const svelte = readFileSync('src/componentes/Seguridad.svelte', 'utf8');
  const match = svelte.match(/\.punto-aviso\s*\{([^}]*)\}/);
  assert.ok(match, 'debe existir la regla .punto-aviso en Seguridad.svelte');
  const bloque = match[1];
  const aparicionesMargin = bloque.match(/margin-left\s*:/g) ?? [];
  assert.ok(aparicionesMargin.length <= 1, `no debe haber margin-left duplicado en .punto-aviso: ${bloque}`);
  assert.doesNotMatch(bloque, /34px/, 'no debe desplazar el indicador 34px en la pestaña');
});
