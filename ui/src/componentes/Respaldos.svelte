<!--
  Gestión de respaldos y restauración controlada en Tauri.

  Permite crear respaldos consistentes, verificar su integridad criptográfica,
  ensayar su apertura aislada sin tocar la base activa, y restaurar perfiles
  con salvaguarda previa, reversibilidad ante fallos y suspensión preventiva
  de la reindexación automática.
-->
<script lang="ts">
  import Archive from "@lucide/svelte/icons/archive";
  import CheckCircle from "@lucide/svelte/icons/check-circle";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import FolderDown from "@lucide/svelte/icons/folder-down";
  import FolderUp from "@lucide/svelte/icons/folder-up";
  import HardDrive from "@lucide/svelte/icons/hard-drive";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Play from "@lucide/svelte/icons/play";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { app } from "../lib/app.svelte";
  import { frase } from "../lib/nombres";
  import {
    abrirCarpetaSalvaguarda,
    motivoReindexadoPausado,
    respaldoComprobar,
    respaldoCrear,
    respaldoEnsayar,
    respaldoSeleccionar,
    restauracionEjecutar,
    restauracionPlanificar,
  } from "../lib/nucleo";
  import type {
    InformeEnsayo,
    InformeRestauracion,
    Manifiesto,
    PlanRestauracion,
    RespaldoInfoResumen,
    ResultadoCrearRespaldo,
  } from "../lib/nucleo";

  let ocupado = $state(false);
  let error = $state("");
  let exito = $state("");

  // Estado de creación
  let creado = $state<ResultadoCrearRespaldo | null>(null);

  // Estado de respaldo seleccionado
  let seleccionado = $state<RespaldoInfoResumen | null>(null);
  let clave = $state("");

  // Resultados de comprobación y ensayo
  let verificado = $state<Manifiesto | null>(null);
  let ensayo = $state<InformeEnsayo | null>(null);

  // Diagnóstico previo y confirmación de restauración
  let plan = $state<PlanRestauracion | null>(null);
  let confirmado = $state(false);
  let restaurado = $state<InformeRestauracion | null>(null);

  // Estado de pausa preventiva
  let reindexadoPausado = $derived(!!app.seguridad?.reindexado_pausado);
  let motivoPausa = $state<string | null>(null);

  $effect(() => {
    if (reindexadoPausado) {
      motivoReindexadoPausado().then((m: string | null) => (motivoPausa = m)).catch(() => {});
    }
  });

  function formatearFecha(ms: number): string {
    if (!ms) return "Desconocida";
    const d = new Date(ms);
    return d.toLocaleString("es-ES", {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function formatearBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  async function crear(): Promise<void> {
    if (ocupado) return;
    error = "";
    exito = "";
    ocupado = true;
    try {
      const res = await respaldoCrear();
      if (res) {
        creado = res;
        seleccionado = {
          carpeta: res.carpeta,
          nombre_carpeta: res.nombre_carpeta,
          manifiesto: res.manifiesto,
          requiere_clave: false,
        };
        exito = "Respaldo creado y verificado con éxito.";
        app.avisar("Respaldo creado y verificado.");
      }
    } catch (e) {
      error = frase(e);
    } finally {
      ocupado = false;
    }
  }

  async function seleccionar(): Promise<void> {
    if (ocupado) return;
    error = "";
    exito = "";
    verificado = null;
    ensayo = null;
    plan = null;
    restaurado = null;
    confirmado = false;
    ocupado = true;
    try {
      const res = await respaldoSeleccionar(clave.trim() || null);
      if (res) {
        seleccionado = res;
      }
    } catch (e) {
      error = frase(e);
    } finally {
      ocupado = false;
    }
  }

  async function comprobar(): Promise<void> {
    if (ocupado || !seleccionado) return;
    error = "";
    exito = "";
    ocupado = true;
    try {
      verificado = await respaldoComprobar(clave.trim() || null);
      exito =
        "Integridad verificada: la llave descifra una copia en solo lectura de la base y su contenido coincide íntegramente con el manifiesto, sin modificar tu perfil activo.";
    } catch (e) {
      error = frase(e);
    } finally {
      ocupado = false;
    }
  }

  async function ejecutarEnsayo(): Promise<void> {
    if (ocupado || !seleccionado) return;
    error = "";
    exito = "";
    ocupado = true;
    try {
      ensayo = await respaldoEnsayar(clave.trim() || null);
      exito =
        "Ensayo completado en una carpeta temporal aislada. La bóveda se abrió por completo y pudo recuperar su contenido sin tocar tu perfil activo.";
    } catch (e) {
      error = frase(e);
    } finally {
      ocupado = false;
    }
  }

  async function diagnosticar(): Promise<void> {
    if (ocupado || !seleccionado) return;
    error = "";
    exito = "";
    confirmado = false;
    ocupado = true;
    try {
      plan = await restauracionPlanificar(clave.trim() || null);
    } catch (e) {
      error = frase(e);
    } finally {
      ocupado = false;
    }
  }

  async function restaurar(): Promise<void> {
    if (ocupado || !seleccionado || !confirmado) return;
    error = "";
    exito = "";
    ocupado = true;
    try {
      const res = await restauracionEjecutar(true, clave.trim() || null);
      restaurado = res;
      plan = null;
      confirmado = false;
      exito = "Perfil restaurado satisfactoriamente. Se ha creado una salvaguarda del perfil anterior.";
      app.avisar("Perfil restaurado con éxito.");
      await app.recargarSeguridad();
      app.version++;
    } catch (e) {
      error = frase(e);
    } finally {
      ocupado = false;
    }
  }

  async function abrirSalvaguarda(ruta: string): Promise<void> {
    try {
      await abrirCarpetaSalvaguarda(ruta);
    } catch (e) {
      error = frase(e);
    }
  }

  async function reanudar(): Promise<void> {
    if (ocupado) return;
    ocupado = true;
    try {
      await app.reanudarSincronizacion();
      motivoPausa = null;
    } catch (e) {
      error = frase(e);
    } finally {
      ocupado = false;
    }
  }
</script>

<div class="respaldos-contenedor">
  {#if reindexadoPausado}
    <div class="tarjeta-aviso alerta-pausa">
      <div class="linea-alerta">
        <TriangleAlert size={18} class="icono-aviso" />
        <div>
          <h4>Sincronización de bibliotecas suspendida</h4>
          <p>
            {motivoPausa
              ? `Motivo: ${motivoPausa}.`
              : "La indexación automática está pausada para proteger tu conocimiento tras una restauración reciente."}
            Revisa tus carpetas y pulsa reanudar cuando confirmes su ubicación en este equipo.
          </p>
        </div>
      </div>
      <button class="boton-prim hov" onclick={reanudar} disabled={ocupado}>
        <RefreshCw size={14} /> Reanudar sincronización
      </button>
    </div>
  {/if}

  {#if error}
    <div class="alerta-error">
      <ShieldAlert size={18} />
      <span>{error}</span>
    </div>
  {/if}

  {#if exito}
    <div class="alerta-exito">
      <CheckCircle size={18} />
      <span>{exito}</span>
    </div>
  {/if}

  <!-- SECCIÓN 1: CREAR RESPALDO -->
  <section class="bloque">
    <div class="linea">
      <FolderDown size={20} />
      <div>
        <h3>Crear un respaldo</h3>
        <p>
          Guarda una instantánea consistente y cifrada de tu base en la carpeta que elijas.
          El respaldo se crea en una subcarpeta nueva y se verifica automáticamente antes de confirmarse.
        </p>
      </div>
    </div>

    <div class="acciones-linea">
      <button class="boton-prim hov" onclick={crear} disabled={ocupado}>
        <Archive size={15} />
        {ocupado ? "Generando y verificando..." : "Crear respaldo..."}
      </button>
    </div>

    {#if creado}
      <div class="resumen-respaldo bien">
        <div class="cabecera-resumen">
          <CheckCircle size={16} class="icono-bien" />
          <strong>Respaldo verificado: {creado.nombre_carpeta}</strong>
        </div>
        <div class="rejilla-datos">
          <div><span class="etiqueta">Fecha:</span> {formatearFecha(creado.manifiesto.creado_en_ms)}</div>
          <div><span class="etiqueta">Tamaño base:</span> {formatearBytes(creado.manifiesto.base.bytes)}</div>
          <div><span class="etiqueta">Entidades:</span> {creado.manifiesto.instantanea.entidades}</div>
          <div><span class="etiqueta">Aserciones:</span> {creado.manifiesto.instantanea.aserciones} ({creado.manifiesto.instantanea.aserciones_vigentes} vigentes)</div>
          <div><span class="etiqueta">Fragmentos:</span> {creado.manifiesto.instantanea.fragmentos}</div>
          <div><span class="etiqueta">Vectores:</span> {creado.manifiesto.instantanea.vectores}</div>
        </div>
        <div class="ruta-respaldo">
          <code>{creado.carpeta}</code>
        </div>
      </div>
    {/if}

    <p class="nota">
      <strong>Recuerda:</strong> El respaldo incluye tu base cifrada completa, su llave y el manifiesto.
      <strong>No incluye los documentos físicos originales</strong> (PDF, imágenes, contratos en sus carpetas),
      que deben respaldarse por separado en tu copia de seguridad habitual.
    </p>
  </section>

  <!-- SECCIÓN 2: SELECCIONAR Y COMPROBAR RESPALDO EXISTENTE -->
  <section class="bloque">
    <div class="linea">
      <FolderUp size={20} />
      <div>
        <h3>Comprobar o ensayar un respaldo existente</h3>
        <p>
          Selecciona una carpeta de respaldo para <strong>comprobar su integridad</strong> (verifica los archivos
          y descifra una copia en solo lectura para confirmar que el contenido coincide con el manifiesto) o
          <strong>ensayar su recuperación</strong> (abre la bóveda completa en una carpeta temporal descartable)
          sin sustituir en ningún momento tu base activa.
        </p>
      </div>
    </div>

    <div class="acciones-linea">
      <button class="boton-sec hov" onclick={seleccionar} disabled={ocupado}>
        Seleccionar carpeta de respaldo...
      </button>
    </div>

    {#if seleccionado}
      <div class="tarjeta-seleccionado">
        <div class="cabecera-seleccionado">
          <HardDrive size={16} />
          <strong>{seleccionado.nombre_carpeta}</strong>
          <span class="formato-badge">{seleccionado.manifiesto.formato}</span>
        </div>

        <div class="rejilla-datos">
          <div><span class="etiqueta">Creado:</span> {formatearFecha(seleccionado.manifiesto.creado_en_ms)}</div>
          <div><span class="etiqueta">Tamaño base:</span> {formatearBytes(seleccionado.manifiesto.base.bytes)}</div>
          <div><span class="etiqueta">Entidades:</span> {seleccionado.manifiesto.instantanea.entidades}</div>
          <div><span class="etiqueta">Aserciones:</span> {seleccionado.manifiesto.instantanea.aserciones}</div>
          <div><span class="etiqueta">Esquema:</span> v{seleccionado.manifiesto.instantanea.esquema_version}</div>
          <div>
            <span class="etiqueta">PIN:</span>
            {seleccionado.manifiesto.instantanea.tiene_pin ? "Protegido con PIN" : "Sin PIN"}
          </div>
        </div>

        {#if seleccionado.requiere_clave}
          <div class="aviso-clave">
            <KeyRound size={16} />
            <div>
              <strong>Requiere clave de recuperación</strong>
              <p>
                Este respaldo procede de otro equipo o usuario (su llave no está en el almacén de Windows de esta cuenta).
                Escribe tu clave de recuperación para poder comprobar o restaurar este respaldo.
              </p>
              <input
                type="password"
                class="campo"
                placeholder="Clave de recuperación (letras y números)"
                bind:value={clave}
                autocomplete="off"
              />
            </div>
          </div>
        {/if}

        <div class="botones-comprobacion">
          <button class="boton-sec hov" onclick={comprobar} disabled={ocupado}>
            <ShieldCheck size={14} /> Comprobar integridad
          </button>
          <button class="boton-sec hov" onclick={ejecutarEnsayo} disabled={ocupado}>
            <Play size={14} /> Ejecutar ensayo aislado
          </button>
          <button class="boton-prim hov" onclick={diagnosticar} disabled={ocupado}>
            Diagnosticar sustitución...
          </button>
        </div>

        {#if verificado}
          <div class="resultado-caja bien">
            <CheckCircle size={15} class="icono-bien" />
            <div>
              <strong>Integridad verificada con éxito</strong>
              <p>
                Los archivos del respaldo, su llave y el contenido cifrado de la instantánea se han comprobado en solo lectura y coinciden con el manifiesto, sin modificar tu perfil activo.
              </p>
            </div>
          </div>
        {/if}

        {#if ensayo}
          <div class="resultado-caja bien">
            <CheckCircle size={15} class="icono-bien" />
            <div>
              <strong>Ensayo de recuperación satisfactorio</strong>
              <p>
                La bóveda se abrió correctamente en un directorio temporal aislado sin sustituir tu base activa. Se recuperaron:
                {ensayo.documentos_en_boveda} documentos en bóveda, {ensayo.colecciones} colecciones,
                {ensayo.vencimientos} vencimientos.
              </p>
              <p class="bibliotecas-resumen">
                Bibliotecas registradas: {ensayo.bibliotecas_presentes} presentes en este equipo,
                {ensayo.bibliotecas_ausentes_en_disco.length} ausentes.
              </p>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </section>

  <!-- SECCIÓN 3: RESTAURACIÓN REAL -->
  {#if plan}
    <section class="bloque seccion-restauracion">
      <div class="linea">
        <TriangleAlert size={20} class="icono-aviso" />
        <div>
          <h3>Plan de sustitución del perfil activo</h3>
          <p>
            Compara el respaldo contra tu perfil actual antes de autorizar la operación.
          </p>
        </div>
      </div>

      <div class="tabla-comparativa">
        <div class="columna-comparativa">
          <h4>Respaldo a restaurar</h4>
          <p class="dato-comp"><strong>Entidades:</strong> {plan.manifiesto_respaldo.instantanea.entidades}</p>
          <p class="dato-comp"><strong>Aserciones:</strong> {plan.manifiesto_respaldo.instantanea.aserciones}</p>
          <p class="dato-comp"><strong>Fragmentos:</strong> {plan.manifiesto_respaldo.instantanea.fragmentos}</p>
          <p class="dato-comp"><strong>Vectores:</strong> {plan.manifiesto_respaldo.instantanea.vectores}</p>
        </div>

        <div class="columna-comparativa actual">
          <h4>Perfil activo actual</h4>
          {#if plan.perfil_actual.existe}
            <p class="dato-comp"><strong>Entidades:</strong> {plan.perfil_actual.entidades}</p>
            <p class="dato-comp"><strong>Aserciones:</strong> {plan.perfil_actual.aserciones}</p>
            <p class="dato-comp"><strong>Fragmentos:</strong> {plan.perfil_actual.fragmentos}</p>
            <p class="dato-comp"><strong>Vectores:</strong> {plan.perfil_actual.vectores}</p>
          {:else}
            <p class="dato-comp tenue">No existe base previa en este equipo.</p>
          {/if}
        </div>
      </div>

      {#if plan.advertencias.length > 0}
        <div class="caja-advertencias">
          <h4>Advertencias importantes</h4>
          <ul>
            {#each plan.advertencias as adv}
              <li>{adv}</li>
            {/each}
          </ul>
        </div>
      {/if}

      <div class="bibliotecas-estado">
        <h4>Bibliotecas del respaldo en este equipo:</h4>
        <div class="lista-bibliotecas">
          {#each plan.bibliotecas_respaldo as bib}
            <div class="item-bib" class:presente={bib.existe_en_disco} class:ausente={!bib.existe_en_disco}>
              <span class="punto-bib"></span>
              <span class="ruta-bib">{bib.ruta}</span>
              <span class="estado-bib">{bib.existe_en_disco ? "Presente" : "No encontrada"}</span>
            </div>
          {/each}
        </div>
      </div>

      <div class="garantia-salvaguarda">
        <ShieldCheck size={16} class="icono-bien" />
        <p>
          Se guardará una <strong>salvaguarda previa</strong> de tu perfil actual antes de sustituirlo.
          Si falla la validación o la sustitución se intenta restaurar la salvaguarda; ante una interrupción crítica o estado ambiguo,
          la aplicación detendrá la apertura automática y conservará todas las evidencias en disco.
        </p>
      </div>

      <div class="confirmacion-area">
        <label class="check-confirmar">
          <input type="checkbox" bind:checked={confirmado} disabled={ocupado} />
          <span>
            Comprendo que esta operación sustituirá la base de conocimiento activa por el respaldo seleccionado
            y que la sincronización se pausará hasta que revise mis bibliotecas.
          </span>
        </label>

        <div class="botones">
          <button class="boton-sec hov" onclick={() => (plan = null)} disabled={ocupado}>
            Cancelar
          </button>
          <button class="boton-peligro hov" onclick={restaurar} disabled={ocupado || !confirmado}>
            {ocupado ? "Restaurando perfil..." : "Confirmar y sustituir perfil"}
          </button>
        </div>
      </div>
    </section>
  {/if}

  {#if restaurado}
    <section class="bloque resultado-restauracion-final">
      <div class="alerta-exito destacada">
        <CheckCircle size={20} />
        <div>
          <h4>¡Perfil restaurado con éxito!</h4>
          <p>
            Tu base activa ha sido actualizada con el conocimiento del respaldo.
            La reindexación automática permanece suspendida preventivamente.
          </p>
        </div>
      </div>

      {#if restaurado.salvaguarda_creada}
        <div class="salvaguarda-info">
          <span>Salvaguarda del perfil anterior creada en:</span>
          <code>{restaurado.salvaguarda_creada}</code>
          <button class="boton-sec hov boton-pequeno" onclick={() => abrirSalvaguarda(restaurado?.salvaguarda_creada ?? "")}>
            <ExternalLink size={13} /> Ver en el Explorador
          </button>
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .respaldos-contenedor {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .bloque {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--linea);
  }
  .bloque:last-child {
    padding-bottom: 0;
    border-bottom: 0;
  }
  .linea {
    display: flex;
    gap: 14px;
    color: var(--tenue);
  }
  .linea :global(svg) {
    flex-shrink: 0;
    margin-top: 1px;
  }
  h3 {
    margin-bottom: 3px;
    font-size: 15px;
    font-weight: 600;
    color: var(--tinta);
  }
  h4 {
    margin-bottom: 4px;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--tinta);
  }
  .linea p {
    font-size: 13.5px;
    line-height: 1.55;
  }
  .acciones-linea {
    display: flex;
    gap: 10px;
    margin-left: 34px;
  }
  .nota {
    margin-left: 34px;
    padding: 9px 12px;
    border-radius: 10px;
    background: var(--calido-fondo);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .tarjeta-aviso {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--calido-fondo);
    border: 1px solid var(--calido);
  }
  .linea-alerta {
    display: flex;
    gap: 12px;
    font-size: 13px;
    line-height: 1.5;
  }
  .alerta-pausa button {
    align-self: flex-start;
  }
  .alerta-error {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-radius: 10px;
    background: rgba(220, 53, 69, 0.12);
    color: var(--peligro);
    font-size: 13px;
    border: 1px solid var(--peligro);
  }
  .alerta-exito {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-radius: 10px;
    background: rgba(40, 167, 69, 0.12);
    color: var(--bien);
    font-size: 13px;
    border: 1px solid var(--bien);
  }
  .alerta-exito.destacada {
    align-items: flex-start;
  }
  .resumen-respaldo, .tarjeta-seleccionado {
    margin-left: 34px;
    padding: 14px;
    border-radius: 12px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .cabecera-resumen, .cabecera-seleccionado {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    color: var(--tinta);
  }
  .formato-badge {
    margin-left: auto;
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 6px;
    background: var(--linea);
    color: var(--tenue);
  }
  .rejilla-datos {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px 16px;
    font-size: 12.5px;
    color: var(--tinta);
  }
  .etiqueta {
    color: var(--tenue);
    margin-right: 4px;
  }
  .ruta-respaldo {
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--fondo);
    font-size: 12px;
    overflow-x: auto;
  }
  .aviso-clave {
    display: flex;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--calido-fondo);
    border: 1px solid var(--calido);
  }
  .aviso-clave p {
    font-size: 12.5px;
    margin: 4px 0 8px;
    line-height: 1.45;
  }
  .botones-comprobacion {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 6px;
  }
  .resultado-caja {
    display: flex;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .resultado-caja.bien {
    border-color: var(--bien);
  }
  :global(.icono-bien) {
    color: var(--bien);
  }
  :global(.icono-aviso) {
    color: var(--calido);
  }
  .bibliotecas-resumen {
    margin-top: 4px;
    color: var(--tenue);
  }
  .tabla-comparativa {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    margin-left: 34px;
  }
  .columna-comparativa {
    padding: 12px;
    border-radius: 10px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
  }
  .columna-comparativa.actual {
    background: var(--fondo);
  }
  .dato-comp {
    font-size: 12.5px;
    margin: 4px 0;
  }
  .caja-advertencias {
    margin-left: 34px;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--calido-fondo);
    border: 1px solid var(--calido);
    font-size: 12.5px;
  }
  .caja-advertencias ul {
    margin: 4px 0 0 18px;
    padding: 0;
  }
  .caja-advertencias li {
    margin: 3px 0;
    line-height: 1.4;
  }
  .bibliotecas-estado {
    margin-left: 34px;
  }
  .lista-bibliotecas {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 6px;
    max-height: 120px;
    overflow-y: auto;
  }
  .item-bib {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 6px;
    background: var(--tarjeta);
    font-size: 12px;
  }
  .punto-bib {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .item-bib.presente .punto-bib {
    background: var(--bien);
  }
  .item-bib.ausente .punto-bib {
    background: var(--peligro);
  }
  .ruta-bib {
    flex-grow: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .estado-bib {
    font-size: 11px;
    color: var(--tenue);
  }
  .garantia-salvaguarda {
    display: flex;
    gap: 10px;
    margin-left: 34px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--tarjeta);
    border: 1px solid var(--linea);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .confirmacion-area {
    margin-left: 34px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .check-confirmar {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: 12.5px;
    line-height: 1.45;
    cursor: pointer;
  }
  .check-confirmar input {
    margin-top: 3px;
  }
  .botones {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .boton-peligro {
    background: var(--peligro);
    color: white;
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
  }
  .boton-peligro:hover:not(:disabled) {
    opacity: 0.9;
  }
  .boton-peligro:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .boton-pequeno {
    font-size: 12px;
    padding: 4px 8px;
    align-self: flex-start;
  }
  .salvaguarda-info {
    margin-left: 34px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
  }
</style>
