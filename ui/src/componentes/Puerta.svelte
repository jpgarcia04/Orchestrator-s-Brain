<!--
  El interruptor de la puerta de Aetheris (ADR-0038, B2 y ajuste 5; ADR-0039,
  paso 4), en el panel de Seguridad.

  Dice sin rodeos qué recibe Aetheris en cada posición: que lo normal puede
  salir, que los nombres de carpetas y colecciones también, y que lo protegido
  nunca, tengas PIN o no. La posición la decide el núcleo: esta vista sólo
  enseña la que él devuelve y, si no la pudo leer, no marca ninguna.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/app.svelte";
  import { frase } from "../lib/nombres";
  import { puertaPoner, puertaPosicion, type PosicionPuerta } from "../lib/nucleo";

  const opciones: { valor: PosicionPuerta; titulo: string; texto: string }[] = [
    {
      valor: "apagada",
      titulo: "Apagada",
      texto: "Aetheris no recibe nada de Brain.",
    },
    {
      valor: "solo_localizar",
      titulo: "Sólo localizar",
      texto:
        "Aetheris recibe el nombre de los documentos normales que tienen que ver con lo que pregunta, " +
        "su biblioteca, su carpeta y su ruta dentro de ella, la fecha, el tamaño y los nombres de tus " +
        "colecciones. Ningún texto de dentro. Ojo: que un documento salga ya dice que su texto tiene que " +
        "ver con lo que preguntó.",
    },
    {
      valor: "localizar_y_leer",
      titulo: "Localizar y leer lo normal",
      texto:
        "Además, pasajes literales del texto de lo normal y la ficha de un documento, con su ruta " +
        "completa en este equipo.",
    },
  ];
  const orden = (p: PosicionPuerta) => opciones.findIndex((o) => o.valor === p);

  /** La posición que dijo el núcleo; `null` mientras no se sabe. */
  let actual = $state<PosicionPuerta | null>(null);
  let error = $state("");
  let ocupado = $state(false);

  onMount(async () => {
    try {
      actual = await puertaPosicion();
    } catch (e) {
      error = `No se pudo leer cómo está la puerta: ${frase(e)}`;
    }
  });

  async function elegir(nueva: PosicionPuerta): Promise<void> {
    if (ocupado || actual === null || nueva === actual) return;
    ocupado = true;
    error = "";
    const abre = orden(nueva) > orden(actual);
    try {
      try {
        await puertaPoner(nueva);
      } catch (e) {
        // Abrir más con el PIN bloqueado: primero el PIN, y otra vez.
        const s = app.seguridad;
        if (!(abre && s?.pin && !s.desbloqueada)) throw e;
        if (!(await app.pedirPin())) return;
        await puertaPoner(nueva);
      }
      // Lo que enseña es lo que quedó guardado, no lo que se pidió.
      actual = await puertaPosicion();
      app.avisar(
        actual === "apagada"
          ? "Puerta apagada: Aetheris ya no recibe nada de Brain."
          : `Puerta en «${opciones.find((o) => o.valor === actual)?.titulo.toLowerCase()}».`,
      );
    } catch (e) {
      error = frase(e);
    } finally {
      ocupado = false;
    }
  }
</script>

<section class="puerta">
  <h3>La puerta de Aetheris</h3>
  <p>
    Con la puerta abierta, Aetheris puede preguntarle a Brain por tus documentos desde este mismo equipo, sin red.
    Sólo le llega <strong>lo normal ya clasificado</strong>: nunca lo sensible, lo de bóveda ni lo que está sin
    clasificar, tengas PIN o no. Aetheris no puede cambiar este ajuste; sólo esta ventana.
  </p>

  <div class="opciones" role="radiogroup" aria-label="Qué recibe Aetheris">
    {#each opciones as o (o.valor)}
      <button
        class="opcion hov"
        class:elegida={actual === o.valor}
        role="radio"
        aria-checked={actual === o.valor}
        disabled={ocupado || actual === null}
        onclick={() => elegir(o.valor)}
      >
        <span class="titulo">{o.titulo}</span>
        <span class="texto">{o.texto}</span>
      </button>
    {/each}
  </div>

  <p class="nota">
    <strong>Los nombres de carpetas y colecciones también salen.</strong> Si alguno dice algo que no quieres
    compartir, cámbialo o deja la puerta apagada. Lo que Aetheris recibe deja de estar bajo el control de Brain: lo que
    haga con ello lo decide Aetheris.
  </p>
  <p class="pequeno">
    Aetheris arranca la puerta cuando la necesita (<code>obrain-app --puerta</code>), sin ventana. Abre la base sólo
    para leer, en cada pregunta, y no la deja abierta mientras no pregunta. El ajuste se guarda en tu base cifrada.
  </p>
  {#if error}<p class="error-campo">{error}</p>{/if}
</section>

<style>
  .puerta {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h3 {
    font-size: 15px;
    font-weight: 600;
    color: var(--tinta);
  }
  p {
    font-size: 13.5px;
    line-height: 1.55;
    color: var(--tenue);
  }
  .opciones {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .opcion {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 10px 14px;
    border-radius: 12px;
    border: 1px solid var(--linea);
    background: transparent;
    text-align: left;
  }
  .opcion.elegida {
    border-color: var(--acento);
    background: var(--sel);
  }
  .opcion:disabled {
    cursor: default;
  }
  .titulo {
    font-size: 14px;
    font-weight: 600;
    color: var(--tinta);
  }
  .texto {
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--tenue);
  }
  .nota {
    padding: 9px 12px;
    border-radius: 10px;
    background: var(--calido-fondo);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .pequeno {
    font-size: 12px;
  }
  code {
    font-family: var(--mono);
    font-size: 11.5px;
  }
</style>
