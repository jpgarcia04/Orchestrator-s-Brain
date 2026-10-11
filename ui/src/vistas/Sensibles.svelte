<!--
  Lo sensible, junto. Son los documentos que la puerta de egreso retiene
  (`PROJECT-MASTER.md` §5.2): nunca salen de la máquina ni entran en un modelo
  remoto, y su contenido no se enseña hasta que lo pides.

  Arriba, las carpetas que marcaste (ADR-0020): lo que tienen dentro es
  sensible aunque su nombre o su texto no lo delaten. Una carpeta marcada sin
  nada dentro casi siempre es una que se renombró o se movió, y se dice.
-->
<script lang="ts">
  import FolderLock from "@lucide/svelte/icons/folder-lock";
  import Lock from "@lucide/svelte/icons/lock";
  import { app } from "../lib/app.svelte";
  import { carpetasMarcadas, sensibles } from "../lib/nucleo";
  import type { CarpetaMarcada, Fila } from "../lib/nucleo";
  import { carpetaDe, fecha, nombreBonito, tituloDe } from "../lib/nombres";

  let filas = $state<Fila[] | null>(null);
  let marcadas = $state<CarpetaMarcada[]>([]);

  $effect(() => {
    void app.version;
    sensibles().then((s) => (filas = s)).catch(() => (filas = []));
    carpetasMarcadas().then((m) => (marcadas = m)).catch(() => (marcadas = []));
  });
</script>

<div class="sensibles entra">
  <div class="titulo">
    <h1 class="titular">Sensibles</h1>
    <p>
      Nunca salen de tu computadora ni entran en ningún modelo remoto. Su contenido se enseña sólo
      cuando lo pides.
    </p>
  </div>

  <section class="marcadas" aria-label="Carpetas marcadas">
    <h2 class="rotulo">Carpetas sensibles</h2>
    {#if marcadas.length === 0}
      <p class="pista">
        Marca una carpeta —por ejemplo, la de tus documentos de identidad— con el escudo de su cabecera, y
        todo lo que tenga dentro será sensible, también lo que llegue después.
      </p>
    {:else}
      <div class="carpetas">
        {#each marcadas as m (m.id)}
          <button class="carpeta hov" onclick={() => app.abrirCarpeta(m.id)} title={m.path}>
            <FolderLock size={18} />
            <span class="texto">
              <span class="nombre">{nombreBonito(m.label)}</span>
              <span class="cuenta" class:aviso={m.documents === 0}>
                {m.documents === 0
                  ? "vacía — ¿la borraste, o la moviste a otro disco?"
                  : m.documents === 1
                    ? "1 documento"
                    : `${m.documents} documentos`}
              </span>
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </section>

  {#if filas && filas.length === 0}
    <p class="nada">Ninguno de tus documentos está marcado como sensible.</p>
  {:else if filas}
    <div class="lista">
      {#each filas as f (f.id)}
        {@const t = tituloDe(f.label)}
        <button
          class="fila hov-fondo"
          class:elegida={app.seleccion === f.id}
          onclick={() => app.ver(f.id)}
          ondblclick={() => app.irA(f.id)}
          title={f.path}
        >
          <span class="primera">
            <Lock size={15} />
            <span class="nombre">{t.titulo}</span>
          </span>
          <span class="tenue">{carpetaDe(f.path)}</span>
          <span class="tenue">{t.fecha ?? fecha(f.recency)}</span>
          <span class="nivel">{f.sensitivity === "vault" ? "Bóveda" : "Sensible"}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .sensibles {
    display: flex;
    flex-direction: column;
    gap: 24px;
    padding: 32px 36px 44px;
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
  .tenue {
    color: var(--tenue);
  }
  .tenue {
    font-size: 13px;
  }

  .lista {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--linea);
    border-radius: 14px;
    overflow: hidden;
    background: var(--tarjeta);
  }
  .marcadas {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .pista {
    margin: 0;
    max-width: 680px;
    color: var(--tenue);
    font-size: 13px;
  }
  .carpetas {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
  }
  .carpeta {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 14px;
    border-radius: 12px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    color: var(--calido);
  }
  .carpeta .texto {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .carpeta .nombre {
    color: var(--tinta);
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .carpeta .cuenta {
    font-size: 12px;
    color: var(--tenue);
  }
  .carpeta .cuenta.aviso {
    color: var(--peligro);
  }

  .fila {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 150px 120px 80px;
    gap: 16px;
    align-items: center;
    padding: 13px 20px;
    border-bottom: 1px solid var(--linea);
  }
  .fila:last-child {
    border-bottom: 0;
  }
  .fila.elegida {
    background: var(--sel);
  }
  .primera {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    color: var(--tenue);
  }
  .nombre {
    color: var(--tinta);
    font-size: 14.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nivel {
    justify-self: end;
    font-size: 11.5px;
    font-weight: 500;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--chip);
    color: var(--tenue);
  }
</style>
