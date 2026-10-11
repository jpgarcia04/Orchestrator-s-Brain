<!--
  La papelera (ADR-0025): lo que ya no está en tu bóveda.

  Cuando borras un documento de una de tus carpetas, sale de todas partes y
  aparece aquí durante 30 días, con lo que obrain leyó de él. Después se
  olvida. «Olvidar ya» y «Vaciar papelera» lo adelantan. Nada de esto toca tus
  archivos —ya no están—: borra lo que obrain guardaba de ellos. Si el archivo
  vuelve (lo sacas de la Papelera de Windows), vuelve solo, con todo.
-->
<script lang="ts">
  import Lock from "@lucide/svelte/icons/lock";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Tipo from "../componentes/Tipo.svelte";
  import { app } from "../lib/app.svelte";
  import { olvidar, papelera } from "../lib/nucleo";
  import type { EnPapelera } from "../lib/nucleo";
  import { haceCuanto, nombreBonito, tipoDe, tituloDe } from "../lib/nombres";

  let filas = $state<EnPapelera[] | null>(null);
  /** El segundo clic de «Vaciar papelera»: dura unos segundos y se olvida. */
  let confirmando = $state(false);
  let olvido: number | undefined;
  let trabajando = $state(false);

  $effect(() => {
    void app.version;
    papelera()
      .then((p) => (filas = p))
      .catch(() => (filas = []));
  });

  function quedan(d: number): string {
    if (d <= 0) return "se olvida hoy";
    if (d === 1) return "se olvida mañana";
    return `se olvida en ${d} días`;
  }

  async function olvidarYa(id: string | null): Promise<void> {
    trabajando = true;
    try {
      const n = await olvidar(id);
      if (id === null) {
        app.avisar(n === 1 ? "Olvidé lo que había leído de 1 documento." : `Olvidé lo que había leído de ${n} documentos.`);
      }
      if (id !== null && app.seleccion === id) app.seleccion = null;
      if (id === null) app.seleccion = null;
      app.version++;
    } catch (e) {
      app.avisar(`No se pudo olvidar — ${e}`);
    } finally {
      trabajando = false;
    }
  }

  function vaciar(): void {
    if (!confirmando) {
      confirmando = true;
      window.clearTimeout(olvido);
      olvido = window.setTimeout(() => (confirmando = false), 4000);
      return;
    }
    confirmando = false;
    void olvidarYa(null);
  }
</script>

<div class="papelera entra">
  <div class="cabeza">
    <div class="titulo">
      <h1 class="titular">Papelera</h1>
      <p>
        Lo que ya no está en tu bóveda. Se queda aquí 30 días, por si fue un error, y después obrain olvida lo que
        leyó de ello. Tus archivos no se tocan: eso ya lo decidiste tú.
      </p>
    </div>
    {#if filas && filas.length > 0}
      <button
        class="vaciar hov"
        class:confirmando
        disabled={trabajando}
        onclick={vaciar}
        title={confirmando ? "Pulsa otra vez para vaciarla" : "Olvidar ya todo lo de la papelera"}
      >
        <Trash2 size={15} />
        <span>{confirmando ? "¿Vaciar la papelera?" : "Vaciar papelera"}</span>
      </button>
    {/if}
  </div>

  {#if filas && filas.length === 0}
    <p class="nada">
      La papelera está vacía. Si borras un documento de tus carpetas, aparecerá aquí.
    </p>
  {:else if filas}
    <div class="lista">
      {#each filas as f (f.id)}
        {@const t = tituloDe(f.label)}
        <div class="fila" class:elegida={app.seleccion === f.id}>
          <button class="abre hov-fondo" onclick={() => app.ver(f.id)} title={f.path}>
            <span class="primera">
              {#if f.sensitivity !== "normal"}<Lock size={15} />{:else}<Tipo tipo={tipoDe(f.label)} />{/if}
              <span class="texto">
                <span class="nombre">{t.titulo}</span>
                <span class="tenue">
                  {f.folder ? `estaba en ${nombreBonito(f.folder)} · ` : ""}se fue {haceCuanto(f.since)}
                </span>
              </span>
            </span>
            <span class="tenue quedan" class:pronto={f.days_left <= 3}>{quedan(f.days_left)}</span>
          </button>
          <button class="olvidar enlace" disabled={trabajando} onclick={() => olvidarYa(f.id)}>Olvidar ya</button>
        </div>
      {/each}
    </div>
    <p class="pista">
      ¿Lo borraste por error? Sácalo de la Papelera de Windows, o vuelve a copiarlo en tu bóveda, y regresa solo
      con todo lo que sabías de él.
    </p>
  {/if}
</div>

<style>
  .papelera {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 32px 36px 44px;
  }
  .cabeza {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 24px;
  }
  .titulo {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 680px;
  }
  .titulo h1 {
    font-size: 42px;
  }
  .titulo p,
  .nada,
  .tenue,
  .pista {
    color: var(--tenue);
  }
  .tenue {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tenue.pronto {
    color: var(--calido);
  }
  .pista {
    margin: 0;
    max-width: 680px;
    font-size: 13px;
  }

  .vaciar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding: 8px 14px;
    border-radius: 999px;
    border: 1px solid var(--linea);
    background: var(--tarjeta);
    color: var(--tinta);
    font-size: 13px;
  }
  .vaciar.confirmando {
    color: var(--peligro);
    background: var(--peligro-fondo);
  }

  .lista {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--linea);
    border-radius: 14px;
    overflow: hidden;
    background: var(--tarjeta);
  }
  .fila {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--linea);
  }
  .fila:last-child {
    border-bottom: 0;
  }
  .fila.elegida {
    background: var(--sel);
  }
  .abre {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 16px;
    align-items: center;
    padding: 11px 8px 11px 20px;
    text-align: left;
  }
  .primera {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    color: var(--tenue);
  }
  .texto {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .quedan {
    justify-self: end;
  }
  .nombre {
    color: var(--tinta);
    font-size: 14.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .olvidar {
    flex-shrink: 0;
    padding: 0 20px 0 8px;
    font-size: 13px;
  }
</style>
