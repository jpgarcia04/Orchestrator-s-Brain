import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PinSession } from '../src/lib/pin-session.ts';

const unlocked = { pin: true, desbloqueada: true, restante_ms: 300_000 };
const locked = { pin: true, desbloqueada: false, restante_ms: null };

test('los temporizadores conservan el receptor global requerido por el navegador', () => {
  let callback;
  const browserSchedule = function (f) { assert.equal(this, globalThis, 'receptor de setTimeout incorrecto'); callback = f; return 1; };
  const browserCancel = function () { assert.equal(this, globalThis, 'receptor de clearTimeout incorrecto'); };
  const session = new PinSession(() => 0, browserSchedule, browserCancel);
  session.apply({ pin: false, desbloqueada: false, restante_ms: null });
  assert.ok(session.state);
  session.apply(unlocked);
  callback();
  assert.equal(session.state, null);
});
function fixture() {
  let now = 0;
  let callback;
  const session = new PinSession(() => now, (f) => { callback = f; return 1; }, () => {});
  session.apply(unlocked);
  return { session, advance: (n) => { now += n; callback(); } };
}
function deferred() {
  let resolve;
  const promise = new Promise((r) => { resolve = r; });
  return { promise, resolve };
}

test('bloqueo explícito retira contenido recibido y rechaza respuestas en vuelo', async () => {
  const { session } = fixture();
  let content = await session.read(async () => ({ text: 'SINTETICO', image: 'data:fixture', date: '2030-01-01', url: 'https://portal.example' }), async () => unlocked);
  session.subscribe(() => { content = null; });
  const pending = deferred();
  const read = session.read(() => pending.promise, async () => unlocked);
  session.beginChange();
  assert.equal(content, null);
  pending.resolve('SINTETICO');
  await assert.rejects(read, /revocada/);
  assert.equal(content, null);
});

test('caducidad controlada retira datos sin una llamada al núcleo', async () => {
  const { session, advance } = fixture();
  let content = 'SINTETICO';
  session.subscribe(() => { content = null; });
  advance(300_000);
  assert.equal(content, null);
  assert.equal(session.state, null);
  await assert.rejects(session.read(async () => 'SINTETICO', async () => unlocked));
});

test('una respuesta anterior no se publica tras volver a desbloquear', async () => {
  const { session } = fixture();
  const pending = deferred();
  const read = session.read(() => pending.promise, async () => unlocked);
  session.suspend();
  session.apply(locked);
  session.apply(unlocked);
  pending.resolve('SINTETICO');
  await assert.rejects(read, /revocada/);
  assert.equal(await session.read(async () => 'actual', async () => unlocked), 'actual');
});

test('un error de consulta revoca la concesión y el contenido previo', async () => {
  const { session } = fixture();
  let content = 'SINTETICO';
  session.subscribe(() => { content = null; });
  await assert.rejects(session.read(async () => 'nuevo', async () => { throw new Error('fixture'); }));
  assert.equal(content, null);
  assert.equal(session.state, null);
});

test('bloquear durante la consulta de seguridad descarta su respuesta', async () => {
  const { session } = fixture();
  const pending = deferred();
  const read = session.read(async () => 'SINTETICO', () => pending.promise);
  await Promise.resolve();
  session.beginChange();
  pending.resolve(unlocked);
  await assert.rejects(read, /revocada/);
  assert.equal(session.state, null);
});

test('el sondeo no concede acceso durante un bloqueo pendiente o fallido', async () => {
  const { session } = fixture();
  session.beginChange();
  session.apply(unlocked);
  assert.equal(session.state, null);
  await assert.rejects(session.read(async () => 'SINTETICO', async () => unlocked));
  session.finishChange();
  session.apply(locked);
  assert.equal(await session.read(async () => 'nombre público', async () => locked), 'nombre público');
});

test('sin PIN se conservan las lecturas legítimas', async () => {
  const session = new PinSession();
  const noPin = { pin: false, desbloqueada: false, restante_ms: null };
  session.apply(noPin);
  assert.equal(await session.read(async () => 'documento', async () => noPin), 'documento');
});
