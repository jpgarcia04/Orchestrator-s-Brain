<!--
  La base está cifrada y este Windows no tiene su llave (ADR-0021): otro
  equipo, Windows reinstalado, o una contraseña restablecida por un
  administrador. Con la clave de recuperación se abre, y la llave vuelve a
  quedar en manos de Windows: la próxima vez abre sola.
-->
<script lang="ts">
  import KeyRound from "@lucide/svelte/icons/key-round";
  import { app } from "../lib/app.svelte";
  import { frase } from "../lib/nombres";
  import { recuperar } from "../lib/nucleo";

  let codigo = $state("");
  /** La carpeta de la base, no el archivo: es donde hay que ir a mirar. */
  const carpeta = $derived((app.info?.db ?? "").replace(/[\\/][^\\/]*$/, "") || "%USERPROFILE%\\.obrain");
  let error = $state("");
  let ocupado = $state(false);

  async function abrir(e: Event): Promise<void> {
    e.preventDefault();
    if (ocupado) return;
    ocupado = true;
    error = "";
    try {
      await recuperar(codigo);
      app.indexando = true;
      await app.refrescar();
      app.avisar("Tu base vuelve a abrirse en este equipo.");
    } catch (err) {
      error = frase(err);
    } finally {
      ocupado = false;
    }
  }
</script>

<div class="cerrada">
  <form class="caja entra" onsubmit={abrir}>
    <KeyRound size={30} />
    <h1 class="titular">Este equipo no tiene la llave de tu base</h1>
    <p>
      Tu base está cifrada y Windows no puede abrirla aquí: puede ser otro equipo, o que Windows se haya
      reinstalado. Escribe la clave de recuperación que guardaste cuando se cifró.
    </p>
    <input
      class="campo"
      autocomplete="off"
      spellcheck="false"
      placeholder="XXXXX-XXXXX-XXXXX-XXXXX-XXXXX"
      aria-label="Clave de recuperación"
      bind:value={codigo}
    />
    {#if error}<p class="error-campo">{error}</p>{/if}
    <button class="boton-prim hov" type="submit" disabled={codigo.trim().length < 25 || ocupado}>Abrir mi base</button>
    <p class="nota">
      ¿No la tienes? Tus documentos siguen en tus carpetas. Puedes empezar una base nueva: cierra la app y
      cambia de nombre <code>brain.obrain</code> y <code>brain.obrain.llave</code> en
      <code>{carpeta}</code>. Perderías tus carpetas marcadas y tus correcciones.
    </p>
  </form>
</div>

<style>
  .cerrada {
    display: grid;
    place-items: center;
    height: 100%;
    padding: 24px;
    background: var(--fondo);
  }
  .caja {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: min(520px, 100%);
    color: var(--tenue);
  }
  .caja :global(svg) {
    color: var(--calido);
  }
  h1 {
    font-size: 40px;
    color: var(--tinta);
  }
  p {
    line-height: 1.6;
  }
  .campo {
    font-family: var(--mono);
    font-size: 17px;
    letter-spacing: 1px;
  }
  .nota {
    font-size: 12.5px;
  }
  code {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--tinta);
    overflow-wrap: anywhere;
  }
</style>
