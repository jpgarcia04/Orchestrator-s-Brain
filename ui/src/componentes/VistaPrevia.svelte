<!--
  La vista previa: el panel de entidad de `EXPERIENCE.md` §4.4, en palabras de
  persona.

  Arriba, **el documento**: su texto —abierto por el párrafo que explica por
  qué salió en la búsqueda (ADR-0016)— o su imagen. Si es sensible, tapado
  hasta que lo pidas. Debajo, lo que el sistema sabe de él, con lo inferido
  corregible en un clic (I5), lo relacionado, y las salidas: abrir, verlo en
  su carpeta de Windows o en el mapa. Lo técnico existe, plegado.

  Cada apartado se pliega desde su título, y lo plegado se recuerda para
  todos los documentos (`Apartado`).
-->
<script lang="ts" module>
  import { SvelteSet } from "svelte/reactivity";

  /** Lo sensible que has pedido ver en esta sesión. No se recuerda al cerrar. */
  const revelados = new SvelteSet<string>();
</script>

<script lang="ts">
  import Cloud from "@lucide/svelte/icons/cloud";
  import Check from "@lucide/svelte/icons/check";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Eye from "@lucide/svelte/icons/eye";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Lock from "@lucide/svelte/icons/lock";
  import LockOpen from "@lucide/svelte/icons/lock-open";
  import Orbit from "@lucide/svelte/icons/orbit";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import Apartado from "./Apartado.svelte";
  import Tipo from "./Tipo.svelte";
  import { app } from "../lib/app.svelte";
  import {
    abrir,
    corregir,
    corregirVencimiento,
    desdeBytes,
    entidad,
    olvidar,
    previsualizar,
    revelar,
  } from "../lib/nucleo";
  import type { Claim, Parte, PorDentro, Vista, VistaEntidad } from "../lib/nucleo";
  import { diasHasta, fecha, fechaIso, nombreBonito, plazo, tipoDe, tipoLargo, tituloDe } from "../lib/nombres";

  let {
    id,
    consulta,
    tramo = null,
  }: { id: string; consulta: string; tramo?: [number, number] | null } = $props();

  let ficha = $state<VistaEntidad | null>(null);
  let vista = $state<Vista | null>(null);
  let error = $state("");
  let recarga = $state(0);
  let porques = new SvelteSet<string>();
  /** «cambiar» abierto sobre la fecha de caducidad, con la fecha que se edita. */
  let cambiando = $state<string | null>(null);
  let guardando = $state(false);
  /**
   * El documento de dentro cuyo «¿por qué?» está abierto (ADR-0030): la
   * previa se abre por su tramo y lo marca. Manda sobre el tramo con el que
   * se llegó.
   */
  let parteAbierta = $state<string | null>(null);
  /**
   * El tramo por el que se abre, como texto: cada carga trae una ficha nueva
   * con arrays nuevos, y comparar por valor es lo que evita recargar en bucle.
   */
  const tramoActual = $derived.by((): string | null => {
    const t = ficha?.parts.find((x) => x.id === parteAbierta)?.span ?? tramo;
    return t ? `${t[0]}..${t[1]}` : null;
  });

  $effect(() => {
    const miId = id;
    const q = consulta;
    const clave = tramoActual;
    const t = clave ? (clave.split("..").map(Number) as [number, number]) : null;
    void app.version;
    void recarga;
    let vigente = true;
    error = "";
    Promise.all([entidad(miId), previsualizar(miId, q || undefined, t)])
      .then(([f, v]) => {
        if (!vigente) return;
        ficha = f;
        vista = v;
      })
      .catch((e) => {
        if (vigente) error = String(e);
      });
    return () => {
      vigente = false;
    };
  });

  const PLOMERIA = new Set([
    "contained_by",
    "occurred_during",
    "file_size",
    "modified_at",
    "availability",
    "has_hash",
    "mime_guess",
    "extraction_status",
    "embedded_with",
    "folder_path",
  ]);

  const buscar = (p: string) => ficha?.observed.find((c) => c.predicate === p);

  const titulo = $derived(ficha ? tituloDe(ficha.label) : null);
  const tipo = $derived(ficha ? tipoDe(ficha.label) : "···");
  const carpeta = $derived(buscar("contained_by"));
  const tamano = $derived(buscar("file_size")?.value ?? "");
  const modificado = $derived(buscar("modified_at")?.value_time ?? 0);
  const enLaNube = $derived(!!vista && vista.availability !== null && vista.availability !== "local");
  /**
   * Sin PIN, lo sensible llega y se tapa hasta «Mostrar». Con PIN, o no llega
   * (bloqueado) o llega porque ya lo desbloqueaste: el PIN fue el gesto.
   */
  const bloqueado = $derived(!!vista?.locked);
  const oculto = $derived(
    !!ficha && ficha.sensitivity !== "normal" && !app.seguridad?.pin && !revelados.has(id),
  );

  /**
   * El «¿por qué?» de un documento de dentro: abre la previa por su tramo y
   * lo marca; otra vez, lo cierra.
   */
  function alternarParte(p: Parte): void {
    if (parteAbierta === p.id) {
      parteAbierta = null;
      return;
    }
    parteAbierta = p.id;
    if (!app.abierto("documento", true)) app.ponerApartado("documento", true);
  }

  /** Cuánto de él está aquí, en palabras. */
  function cuantoDe(p: Parte): string {
    return p.paragraphs >= p.of ? `sus ${p.of} párrafos, seguidos` : `${p.paragraphs} de sus ${p.of} párrafos, seguidos`;
  }

  /** El «¿por qué?» de un documento de dentro (ADR-0030). */
  function porqueParte(p: Parte): string {
    const nombre = tituloDe(p.label).titulo;
    const s =
      p.paragraphs >= p.of
        ? `Todo el texto de «${nombre}» está en este documento: sus ${p.of} párrafos, seguidos y en el mismo orden.`
        : `Casi todo el texto de «${nombre}» está en este documento: ${p.paragraphs} de sus ${p.of} párrafos, seguidos y en el mismo orden. Los que faltan están en la orilla, pegados al texto de al lado.`;
    if (bloqueado) return s;
    if (oculto) return `${s} Pulsa «Mostrar» para verlo marcado.`;
    return vista?.highlight ? `${s} Está marcado arriba.` : s;
  }

  /** Por qué es sensible el documento de dentro que hace sensible a éste. */
  function porQueLoEs(d: PorDentro): string {
    if (d.because === "folder" && d.folder) return `que está en tu carpeta «${nombreBonito(d.folder.label)}»`;
    if (d.because === "user") return "que marcaste tú";
    if (d.because === "content") return "que lo es por lo que dice";
    if (d.because === "name") return "que lo es por su nombre";
    if (d.because === "folder_name") return "que lo es por el nombre de su carpeta";
    return "que también lo es";
  }

  /** Relaciones con otra cosa que no es su carpeta: duplicados, versiones… */
  const relaciones = $derived(
    [...(ficha?.observed ?? []), ...(ficha?.inferred ?? [])].filter(
      (c) => c.value_entity !== null && !PLOMERIA.has(c.predicate),
    ),
  );

  /** Lo que se sabe y no es relación ni plomería: sensibilidad, procedencia… */
  const hechos = $derived(
    [...(ficha?.observed ?? []), ...(ficha?.inferred ?? [])].filter(
      (c) => c.value_entity === null && !PLOMERIA.has(c.predicate),
    ),
  );

  const leido = $derived.by(() => {
    if (!vista) return "";
    if (vista.kind === "text") return `${vista.chars.toLocaleString("es")} caracteres de texto`;
    if (vista.kind === "image") return `Imagen de ${vista.width} × ${vista.height}`;
    return "";
  });

  /** La fecha de caducidad, si la hay: la tuya o la que leí. */
  const vence = $derived(
    [...(ficha?.observed ?? []), ...(ficha?.inferred ?? [])].find((c) => c.predicate === "expires_on") ?? null,
  );

  /**
   * Sin fecha, se puede añadir una a mano (ADR-0026). No con lo sensible
   * bloqueado —no se corrige lo que no se ve, y la línea delataría que no hay
   * fecha— ni en lo que está en la papelera.
   */
  const puedeAnadirFecha = $derived(!!ficha && !vence && !bloqueado && !ficha.trashed && ficha.path !== null);

  /**
   * El tramo que explica una afirmación —la fecha de caducidad, o lo que hizo
   * sensible al documento—, dentro del texto que se ve, cuando se pide su
   * «¿por qué?». El núcleo da los bytes **del documento**; el texto de la
   * vista previa empieza en `offset`.
   */
  const marcado = $derived.by((): { claim: string; rango: [number, number] } | null => {
    if (!vista?.text) return null;
    for (const c of [...(ficha?.observed ?? []), ...(ficha?.inferred ?? [])]) {
      if (!c.span || !porques.has(c.assertion)) continue;
      const [a, b] = [c.span[0] - vista.offset, c.span[1] - vista.offset];
      if (a >= 0 && b > a) return { claim: c.assertion, rango: [a, b] };
    }
    return null;
  });
  const tramoVence = $derived(marcado?.rango ?? null);

  const texto = $derived.by(() => {
    const t = vista?.text ?? "";
    const h = tramoVence ?? vista?.highlight;
    if (!h) return { antes: t, marca: "", despues: "" };
    const [a, b] = [desdeBytes(t, h[0]), desdeBytes(t, h[1])];
    return { antes: t.slice(0, a), marca: t.slice(a, b), despues: t.slice(b) };
  });

  const tecnicos = $derived.by(() => {
    if (!ficha) return [];
    const filas: [string, string][] = [];
    if (ficha.path) filas.push(["ruta", ficha.path]);
    for (const [p, nombre] of [
      ["mime_guess", "tipo"],
      ["availability", "disponibilidad"],
      ["extraction_status", "lectura"],
      ["embedded_with", "significado"],
      ["has_hash", "huella"],
      // Arriba sólo sale el sitio; la dirección entera, aquí.
      ["downloaded_from", "descargado de"],
    ] as const) {
      const c = buscar(p);
      if (c) filas.push([nombre, c.value]);
    }
    if (ficha.facets.length) filas.push(["facetas", ficha.facets.join(" · ")]);
    filas.push(["identificador", ficha.id]);
    return filas;
  });

  function mayuscula(s: string): string {
    return s.charAt(0).toUpperCase() + s.slice(1);
  }

  /** El valor de un hecho, dicho como lo diría una persona. */
  function valor(c: Claim): string {
    if (c.predicate === "sensitivity_level") {
      return { sensitive: "Sensible", vault: "Bóveda", normal: "Normal" }[c.value] ?? c.value;
    }
    if (c.predicate === "ocr_engine") {
      // `windows.media.ocr/es-ES` → «el OCR de Windows (es-ES)»
      const [motor, idioma] = c.value.split("/");
      return motor === "windows.media.ocr" ? `el OCR de Windows${idioma ? ` (${idioma})` : ""}` : c.value;
    }
    if (c.predicate === "expires_on") {
      if (c.value === "nunca") return "No caduca";
      const d = diasHasta(c.value);
      return d === null ? c.value : `${fechaIso(c.value)} · ${plazo(d)}`;
    }
    if (c.predicate === "downloaded_from") return sitio(c.value);
    if (c.value_time !== null) return fecha(c.value_time);
    return c.value;
  }

  /**
   * De dónde se descargó, dicho por su sitio: `chatgpt.com`, no los 240
   * caracteres de la dirección de descarga (JP, 2026-09-23). La dirección
   * entera sale al pasar el ratón y en «Detalles técnicos».
   */
  function sitio(url: string): string {
    try {
      const u = new URL(url);
      return u.hostname ? u.hostname.replace(/^www\./, "") : url;
    } catch {
      return url;
    }
  }

  /**
   * El «¿por qué?» de un hecho. Si explica un tramo del texto, abre lo que
   * haga falta para verlo marcado: el documento, y el texto reconocido de una
   * foto.
   */
  function alternarPorque(c: Claim): void {
    if (porques.has(c.assertion)) {
      porques.delete(c.assertion);
      return;
    }
    porques.add(c.assertion);
    if (!c.span) return;
    if (!app.abierto("documento", true)) app.ponerApartado("documento", true);
    if (vista?.kind === "image" && !app.abierto("reconocido", false)) app.ponerApartado("reconocido", true);
  }

  /** Las palabras de caducidad de ADR-0023, dichas como las leería una persona. */
  const PALABRAS: Record<string, string> = {
    caducidad: "«caduca» o «caducidad»",
    validez: "«validez» o «válido hasta»",
    vencimiento: "«vence» o «vencimiento»",
    vigencia: "«vigencia»",
    hasta: "«hasta», con la vigencia del documento cerca",
  };

  /** El «¿por qué?» de una fecha de caducidad: la evidencia, en palabras. */
  function porqueVence(c: Claim): string {
    const ev = c.evidence ?? "";
    const regla = /regla=(\w+)/.exec(ev)?.[1] ?? "";
    const otras = Number(/otras=(\d+)/.exec(ev)?.[1] ?? 0);
    const hay = otras === 1 ? "Había otra fecha" : `Había otras ${otras} fechas`;
    // ADR-0028: la zona MRZ, y el plazo cuando el OCR se comió la palabra.
    if (regla === "mrz") {
      let s = "La leí en la zona de lectura mecánica (las líneas con «<<<» del pie), y cuadra con su dígito de control.";
      if (otras > 0) s += ` ${hay}; ésta manda porque el dígito de control descarta un número mal leído.`;
      return s + marcaDicha(c, "marcada");
    }
    if (regla === "plazo") {
      const n = Number(/anios=(\d+)/.exec(ev)?.[1] ?? 0);
      const justos = n === 1 ? "un año justo" : `${n} años justos`;
      let s = `No leí la palabra «caducidad» —el OCR no la vio—, pero es un documento de identidad y dos de sus fechas están a ${justos}: la de expedición y ésta, como en un pasaporte.`;
      if (otras > 0) s += ` ${hay}; me quedé con la más lejana.`;
      return s + marcaDicha(c, "marcadas");
    }
    let s = `La leí en el texto, junto a ${PALABRAS[regla] ?? "una palabra de caducidad"}.`;
    if (otras > 0) {
      s += ` ${hay}; me quedé con la de la palabra más clara y, entre ésas, la más lejana.`;
    }
    return s + marcaDicha(c, "marcada");
  }

  /** «Está marcada arriba», si el tramo de esta afirmación es el que se ve. */
  function marcaDicha(c: Claim, genero: "marcada" | "marcado" | "marcadas"): string {
    if (marcado?.claim !== c.assertion) return "";
    const ver = { marcada: "verla marcada", marcado: "verlo marcado", marcadas: "verlas marcadas" }[genero];
    if (oculto) return ` Pulsa «Mostrar» para ${ver}.`;
    if (bloqueado) return "";
    return genero === "marcadas" ? " Las dos están marcadas arriba." : ` Está ${genero} arriba.`;
  }

  /** Lo que encontró cada regla de contenido de la puerta, dicho por una persona. */
  const HALLAZGOS: Record<string, string> = {
    "dni-o-nie": "un DNI o NIE, con su letra de control correcta",
    iban: "un número de cuenta (IBAN) que cuadra con su dígito de control",
    "tarjeta-de-pago": "un número de tarjeta de pago válido",
    "clave-privada-pem": "una clave privada",
    "clave-de-aws": "una clave de acceso de Amazon (AWS)",
    "token-de-github": "una clave de acceso de GitHub",
    "token-de-slack": "una clave de acceso de Slack",
    "clave-de-google": "una clave de acceso de Google",
    "clave-de-modelo": "una clave de acceso de un servicio de IA",
    jwt: "un token de sesión (JWT)",
    "variable-con-secreto": "una línea que guarda una contraseña o un secreto",
    "url-con-contrasena": "una dirección con la contraseña escrita dentro",
  };

  /** Lo que dice el nombre, para las reglas de nombre de la puerta. */
  const NOMBRES_SENSIBLES: Record<string, string> = {
    identidad: "suena a documento de identidad (pasaporte, cédula, visado…)",
    "sigla-de-identidad": "lleva una sigla de identidad (DNI, NIE, CURP…)",
    financiero: "suena a papeles de dinero (nómina, banco, hipoteca…)",
    salud: "suena a un documento médico",
    contractual: "suena a contrato o póliza",
    secreto: "habla de contraseñas o secretos",
    monedero: "suena a un monedero de criptomonedas",
    "codigo-de-respaldo": "suena a códigos de recuperación",
    "material-de-clave": "es de los que guardan claves",
    dotenv: "es un archivo de configuración con secretos (.env)",
  };

  /** El «¿por qué?» de la sensibilidad, en palabras, como el de los vencimientos. */
  function porqueSensible(c: Claim): string {
    const regla = /regla=(\S+)/.exec(c.evidence ?? "")?.[1] ?? "";
    const [familia, cual] = regla.split(":");
    const nivel = c.value === "vault" ? "de bóveda" : "sensible";
    if (familia === "contenido") {
      return `Lo leí en el texto: hay ${HALLAZGOS[cual ?? ""] ?? "algo que no debería salir de tu computadora"}.${marcaDicha(c, "marcado")}`;
    }
    if (familia === "carpeta") {
      const casa = carpeta ? `«${nombreBonito(carpeta.value)}»` : "donde está";
      return `Por la carpeta ${casa}: su nombre suena a documentos personales, así que lo que tiene dentro lo trato como ${nivel}.`;
    }
    if (familia === "nombre" || familia === "extension") {
      return `Por su nombre, «${ficha?.label ?? ""}», que ${NOMBRES_SENSIBLES[cual ?? ""] ?? "suena a algo personal"}. No he tenido que leerlo.`;
    }
    return c.evidence ?? c.source;
  }

  function confirmar(c: Claim): void {
    corregir(id, c.predicate, c.value)
      .then(() => {
        recarga++;
        if (c.predicate === "expires_on") app.version++; // la tarjeta del inicio
      })
      .catch((e) => app.avisar(`No se pudo guardar — ${e}`));
  }

  function empezarCambio(c: Claim): void {
    cambiando = /^\d{4}-\d{2}-\d{2}$/.test(c.value) ? c.value : "";
  }

  /** Guarda tu fecha —o «no caduca», con `null`—: gana para siempre (I5). */
  async function guardarVence(nueva: string | null): Promise<void> {
    if (nueva === "") return;
    guardando = true;
    try {
      await corregirVencimiento(id, nueva);
      cambiando = null;
      app.version++; // la vista previa y la tarjeta del inicio
    } catch (e) {
      app.avisar(`No se pudo guardar — ${e}`);
    } finally {
      guardando = false;
    }
  }

  /** «Olvidar ya» desde la vista previa de algo de la papelera (ADR-0025). */
  async function olvidarYa(): Promise<void> {
    guardando = true;
    try {
      await olvidar(id);
      app.seleccion = null;
      app.version++;
    } catch (e) {
      app.avisar(`No se pudo olvidar — ${e}`);
    } finally {
      guardando = false;
    }
  }

  /**
   * Abrir o revelar algo sensible con el PIN puesto pide desbloquear antes: el
   * núcleo se niega y aquí se pide el PIN y se vuelve a intentar una vez.
   */
  async function conPin(f: () => Promise<void>, que: string): Promise<void> {
    try {
      await f();
    } catch (e) {
      if (String(e).includes("PIN") && (await app.pedirPin())) {
        try {
          await f();
          return;
        } catch (e2) {
          e = e2;
        }
      }
      app.avisar(`${que} — ${e}`);
    }
  }

  function abrirArchivo(): void {
    void conPin(() => abrir(id), "No se pudo abrir");
  }

  function mostrarEnCarpeta(): void {
    void conPin(() => revelar(id), "No se pudo enseñar en su carpeta");
  }

  async function desbloquear(): Promise<void> {
    if (await app.pedirPin()) recarga++;
  }

  /**
   * Centra el párrafo marcado dentro del texto, sin mover el panel entero. Y
   * otra vez cuando la marca cambia de sitio sin desaparecer —de un documento
   * de dentro a otro (ADR-0030)—: la marca es la misma, el sitio no.
   */
  function centrar(nodo: HTMLElement, _donde: number): { update: () => void } {
    const poner = () => {
      const caja = nodo.closest("pre");
      if (caja) caja.scrollTop = Math.max(0, nodo.offsetTop - caja.clientHeight / 3);
    };
    poner();
    // Al cambiar de sitio, después de que el texto de antes ya sea el nuevo.
    return { update: () => requestAnimationFrame(poner) };
  }
</script>

<aside class="previa entra" aria-label="Vista previa">
  <div class="barra">
    <span class="rotulo">Vista previa</span>
    <button class="cerrar hov-fondo" onclick={() => app.cerrarPanel()} aria-label="Cerrar la vista previa" title="Cerrar · Esc">
      <X size={16} />
    </button>
  </div>

  {#if error}
    <p class="error">No se pudo abrir este documento — {error}</p>
  {:else if !ficha || !vista || !titulo}
    <div class="cargando" aria-hidden="true">
      <span class="caja-fantasma"></span>
      <span class="linea-fantasma" style:width="70%"></span>
      <span class="linea-fantasma" style:width="45%"></span>
    </div>
  {:else}
    <div class="contenido">
      <Apartado id="documento" titulo="Documento">
      <div class="caja">
        {#if bloqueado}
          <div class="sin-vista protegida">
            <Lock size={24} />
            <p>Protegido con tu PIN</p>
            <button class="boton hov" onclick={desbloquear}><LockOpen size={15} /> Desbloquear</button>
          </div>
        {:else if vista.kind === "image" && vista.image}
          <img src={vista.image} alt={titulo.titulo} class:velado={oculto} />
        {:else if vista.kind === "text" && vista.text}
          <pre class="papel" class:velado={oculto}>{texto.antes}{#if texto.marca}<mark use:centrar={texto.antes.length}>{texto.marca}</mark>{/if}{texto.despues}</pre>
        {:else}
          <div class="sin-vista">
            <Tipo {tipo} />
            <p>{vista.reason ?? "No hay nada que enseñar de este archivo."}</p>
          </div>
        {/if}
        {#if oculto}
          <div class="velo">
            <Lock size={22} />
            <span>Contenido oculto por ser sensible</span>
            <button class="boton hov" onclick={() => revelados.add(id)}>
              <Eye size={15} /> Mostrar
            </button>
          </div>
        {/if}
      </div>

      {#if vista.kind === "image" && vista.text && !oculto && !bloqueado}
        <!-- Una foto con texto reconocido (OCR): la foto arriba, lo que dice aquí. -->
        <div class="reconocido">
          <Apartado id="reconocido" titulo="Texto reconocido en la imagen" deFabrica={false} tenue>
            <pre class="papel">{texto.antes}{#if texto.marca}<mark use:centrar={texto.antes.length}>{texto.marca}</mark>{/if}{texto.despues}</pre>
          </Apartado>
        </div>
      {/if}
      </Apartado>

      <div class="cabeza">
        <h2 class="titular">{titulo.titulo}</h2>
        <span class="archivo">{ficha.label}</span>
      </div>

      <div class="chips">
        {#if carpeta?.value_entity}
          {@const casa = carpeta.value_entity}
          <button class="chip hov" onclick={() => app.abrirCarpeta(casa, id)}>
            <FolderOpen size={13} /> {nombreBonito(carpeta.value)}
          </button>
        {/if}
        <span class="chip">{tipoLargo(tipo)}</span>
        {#if tamano}<span class="chip">{tamano}</span>{/if}
        {#if titulo.fecha || modificado}<span class="chip">{titulo.fecha ?? fecha(modificado)}</span>{/if}
        {#if enLaNube}<span class="chip"><Cloud size={13} /> En tu nube</span>{/if}
      </div>

      {#if ficha.trashed}
        {@const p = ficha.trashed}
        <section class="fuera" aria-label="En la papelera">
          <Trash2 size={16} />
          <div>
            <p class="que">Ya no está en tu bóveda</p>
            <p class="detalle">
              {p.folder ? `Estaba en «${nombreBonito(p.folder)}». ` : ""}{p.days_left <= 0
                ? "obrain olvida hoy lo que leyó de él."
                : `obrain olvida lo que leyó de él en ${p.days_left === 1 ? "1 día" : `${p.days_left} días`}.`}
              Si lo sacas de la Papelera de Windows, vuelve solo.
            </p>
          </div>
        </section>
      {/if}

      {#if hechos.length > 0 || leido || ficha.protection || ficha.by_part || puedeAnadirFecha}
        <Apartado id="sabido" titulo="Lo que sé de este documento">
          {#if leido}
            <div class="hecho"><span class="clave">Leído</span><span>{leido}</span></div>
          {/if}
          {#if ficha.protection}
            {@const p = ficha.protection}
            <div class="hecho">
              <span class="clave">Sensibilidad</span>
              <span class="valor">
                {p.level === "vault" ? "Bóveda" : "Sensible"}
                <button class="por-carpeta hov-fondo" onclick={() => app.abrirCarpeta(p.folder, id)} title="Abrir la carpeta que lo protege">
                  <ShieldCheck size={13} /> por la carpeta «{nombreBonito(p.label)}», que marcaste tú
                </button>
              </span>
            </div>
          {/if}
          {#if ficha.by_part}
            <!-- ADR-0030: un paquete es tan sensible como lo que lleva dentro. -->
            {@const d = ficha.by_part}
            <div class="hecho">
              <span class="clave">Sensibilidad</span>
              <span class="valor">
                {d.level === "vault" ? "Bóveda" : "Sensible"}
                <button class="por-carpeta hov-fondo" onclick={() => app.ver(d.part)} title="Ver el documento que lleva dentro">
                  <ShieldCheck size={13} /> porque contiene «{tituloDe(d.label).titulo}», {porQueLoEs(d)}
                </button>
              </span>
            </div>
          {/if}
          {#each hechos as c (c.assertion)}
            <div class="hecho" style:opacity={c.method === "inferred" && !c.by_user ? Math.max(0.55, c.confidence) : 1}>
              <span class="clave">{mayuscula(c.label)}</span>
              <span class="valor" title={c.predicate === "downloaded_from" ? c.value : undefined}>
                {valor(c)}
                {#if c.by_user}
                  <span class="acciones-hecho">
                    <span class="tuyo"><Check size={13} /> lo dijiste tú</span>
                    {#if c.predicate === "expires_on" && cambiando === null}
                      <button class="enlace" onclick={() => empezarCambio(c)}>cambiar</button>
                    {/if}
                  </span>
                {:else if c.method === "inferred"}
                  <span class="acciones-hecho">
                    <button class="enlace" onclick={() => alternarPorque(c)}>¿por qué?</button>
                    <button class="enlace" onclick={() => confirmar(c)}>es correcto</button>
                    {#if c.predicate === "expires_on" && cambiando === null}
                      <button class="enlace" onclick={() => empezarCambio(c)}>cambiar</button>
                    {/if}
                  </span>
                  {#if porques.has(c.assertion)}
                    <span class="porque">{c.predicate === "expires_on" ? porqueVence(c) : c.predicate === "sensitivity_level" ? porqueSensible(c) : (c.evidence ?? c.source)}</span>
                  {/if}
                {/if}
                {#if c.predicate === "expires_on" && cambiando !== null}
                  <span class="cambio">
                    <input type="date" bind:value={cambiando} aria-label="Caduca el" disabled={guardando} />
                    <span class="acciones-hecho">
                      <button class="enlace" disabled={guardando || !cambiando} onclick={() => guardarVence(cambiando)}>guardar</button>
                      <button class="enlace" disabled={guardando} onclick={() => guardarVence(null)}>no caduca</button>
                      <button class="enlace tenue" disabled={guardando} onclick={() => (cambiando = null)}>cancelar</button>
                    </span>
                  </span>
                {/if}
              </span>
            </div>
          {/each}
          {#if puedeAnadirFecha}
            <!-- ADR-0026: donde el detector no vio ninguna fecha, se la dices tú. -->
            <div class="hecho sin-fecha">
              <span class="clave">Caduca el</span>
              <span class="valor">
                {#if cambiando === null}
                  <span class="acciones-hecho">
                    <span class="tenue">—</span>
                    <button class="enlace" onclick={() => (cambiando = "")}>añadir</button>
                  </span>
                {:else}
                  <span class="cambio">
                    <input type="date" bind:value={cambiando} aria-label="Caduca el" disabled={guardando} />
                    <span class="acciones-hecho">
                      <button class="enlace" disabled={guardando || !cambiando} onclick={() => guardarVence(cambiando)}>guardar</button>
                      <button class="enlace tenue" disabled={guardando} onclick={() => (cambiando = null)}>cancelar</button>
                    </span>
                  </span>
                {/if}
              </span>
            </div>
          {/if}
        </Apartado>
      {/if}

      {#if ficha.parts.length > 0}
        <!-- ADR-0030: lo que este documento lleva dentro, en el orden de su texto. -->
        <Apartado id="contiene" titulo="Contiene" cuenta={ficha.parts.length}>
          {#each ficha.parts as p (p.id)}
            <div class="parte">
              <button class="relacion hov-fondo" onclick={() => app.ver(p.id)} title="Ver este documento suelto">
                <Tipo tipo={tipoDe(p.label)} />
                <span class="texto-rel">
                  <span class="nombre-rel">{tituloDe(p.label).titulo}</span>
                  <span class="por">{cuantoDe(p)}{p.trashed ? " · en la papelera" : ""}</span>
                </span>
              </button>
              {#if p.span}
                <button class="enlace" onclick={() => alternarParte(p)}>¿por qué?</button>
              {/if}
            </div>
            {#if parteAbierta === p.id}
              <span class="porque">{porqueParte(p)}</span>
            {/if}
          {/each}
        </Apartado>
      {/if}

      {#if relaciones.length > 0}
        <Apartado id="relacionado" titulo="Relacionado" cuenta={relaciones.length}>
          {#each relaciones as r (r.assertion)}
            {@const destino = r.value_entity ?? ""}
            <!-- «Está dentro de» abre el paquete por donde está éste, marcado (ADR-0030). -->
            <button
              class="relacion hov-fondo"
              onclick={() => app.ver(destino, "", r.target_span)}
              title={r.target_span ? "Abrirlo por donde está éste" : undefined}
            >
              <Tipo tipo={tipoDe(r.value)} />
              <span class="texto-rel">
                <span class="nombre-rel">{tituloDe(r.value).titulo}</span>
                <span class="por">{mayuscula(r.label)}</span>
              </span>
            </button>
          {/each}
        </Apartado>
      {/if}

      {#if ficha.trashed}
        <div class="salidas">
          <button class="boton hov" disabled={guardando} onclick={olvidarYa}>
            <Trash2 size={16} /> Olvidar ya
          </button>
        </div>
      {:else}
      <div class="salidas">
        <button class="primario hov" onclick={abrirArchivo} title={enLaNube ? "Está en tu nube: abrirlo lo descargará" : "Abrir con su programa"}>
          <ExternalLink size={16} /> {enLaNube ? "Abrir · se descargará" : "Abrir"}
        </button>
        <button class="boton hov" onclick={mostrarEnCarpeta}>
          <FolderOpen size={16} /> En su carpeta
        </button>
        <button class="boton hov" onclick={() => app.verEnMapa(id)}>
          <Orbit size={16} /> Ver en el mapa
        </button>
      </div>
      <p class="nota">Orchestrator's Brain nunca escribe en tus archivos; el programa que lo abra sí puede.</p>
      {/if}

      <div class="tecnico">
        <Apartado id="tecnico" titulo="Detalles técnicos" deFabrica={false} tenue>
          <dl>
            {#each tecnicos as [k, v] (k)}
              <dt>{k}</dt>
              <dd>{v}</dd>
            {/each}
          </dl>
        </Apartado>
      </div>
    </div>
  {/if}
</aside>

<style>
  .fuera {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--calido-fondo);
    color: var(--calido);
  }
  .fuera p {
    margin: 0;
  }
  .fuera :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }
  .fuera .que {
    color: var(--tinta);
    font-weight: 500;
  }
  .fuera .detalle {
    color: var(--tenue);
    font-size: 13px;
  }
  .previa {
    width: var(--ancho-previa);
    flex-shrink: 0;
    overflow-y: auto;
    border-left: 1px solid var(--linea);
    background: var(--panel);
  }

  .barra {
    display: flex;
    align-items: center;
    padding: 14px 14px 0 22px;
  }
  .barra .rotulo {
    flex-grow: 1;
  }
  .cerrar {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    color: var(--tenue);
  }

  .contenido,
  .cargando {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 12px 22px 28px;
  }

  .caja {
    position: relative;
    display: grid;
    place-items: center;
    min-height: 160px;
    border-radius: 14px;
    overflow: hidden;
    background: var(--chip);
  }
  .caja img {
    display: block;
    max-width: 100%;
    max-height: 300px;
    object-fit: contain;
  }
  .reconocido {
    margin-top: 4px;
  }
  .reconocido .papel {
    width: 100%;
    margin: 2px 0 0;
  }
  .papel {
    position: relative;
    width: calc(100% - 28px);
    max-height: 300px;
    margin: 14px;
    padding: 16px 18px;
    overflow: auto;
    border-radius: 6px;
    background: var(--papel);
    color: var(--papel-tinta);
    box-shadow: 0 12px 28px -14px var(--sombra);
    font: 12.5px/1.6 var(--sans);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
  mark {
    background: rgba(242, 194, 123, 0.45);
    color: inherit;
    border-radius: 3px;
  }
  .velado {
    filter: blur(7px);
    user-select: none;
  }
  .velo {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: var(--velo);
    font-size: 13.5px;
  }
  .sin-vista {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 24px;
    text-align: center;
    color: var(--tenue);
    font-size: 13px;
  }
  .sin-vista.protegida {
    padding: 34px 24px;
  }
  .sin-vista.protegida :global(svg:first-child) {
    color: var(--calido);
  }
  .sin-vista.protegida p {
    color: var(--tinta);
    font-size: 14px;
  }

  .cabeza {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .cabeza h2 {
    font-size: 29px;
    line-height: 1.1;
    overflow-wrap: anywhere;
  }
  .archivo {
    font-family: var(--mono);
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--tenue);
    overflow-wrap: anywhere;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--chip);
    color: var(--tenue);
    font-size: 12.5px;
  }
  button.chip {
    color: var(--tinta);
  }

  .hecho {
    display: grid;
    grid-template-columns: 116px minmax(0, 1fr);
    gap: 12px;
    font-size: 13.5px;
  }
  .clave {
    color: var(--tenue);
  }
  .valor {
    display: flex;
    flex-direction: column;
    gap: 4px;
    /* Una URL de «Descargado de» no tiene espacios: sin esto se salía del
       panel y le ponía barra horizontal. */
    overflow-wrap: anywhere;
  }
  .acciones-hecho {
    display: flex;
    gap: 12px;
  }
  .enlace {
    font-size: 12px;
    color: var(--acento);
  }
  .enlace:disabled {
    opacity: 0.45;
  }
  .enlace.tenue {
    color: var(--tenue);
  }
  .sin-fecha .clave,
  .sin-fecha .tenue {
    color: var(--tenue);
    opacity: 0.8;
  }
  .cambio {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 2px;
  }
  .cambio input {
    width: fit-content;
    padding: 5px 8px;
    border-radius: 8px;
    border: 1px solid var(--linea);
    background: var(--chip);
    color: var(--tinta);
    font: inherit;
    font-size: 13px;
    color-scheme: light dark;
  }
  .porque {
    padding-left: 10px;
    border-left: 2px solid var(--linea);
    font-size: 12px;
    color: var(--tenue);
  }
  .tuyo {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--bien);
  }
  .por-carpeta {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    align-self: flex-start;
    margin-left: -6px;
    padding: 2px 6px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--calido);
    text-align: left;
  }

  .relacion {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0 -8px;
    padding: 8px;
    border-radius: 10px;
  }
  .parte {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .parte .relacion {
    flex-grow: 1;
    min-width: 0;
    margin-right: 0;
  }
  .parte .enlace {
    flex-shrink: 0;
  }
  .parte + .porque {
    margin: -4px 0 4px;
  }
  .texto-rel {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .nombre-rel {
    font-size: 13.5px;
    font-weight: 500;
  }
  .por {
    font-size: 12px;
    color: var(--tenue);
  }

  .salidas {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .primario,
  .boton {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 14px;
    border-radius: 10px;
  }
  .primario {
    background: var(--prim);
    color: var(--prim-tinta);
    font-weight: 500;
  }
  .boton {
    border: 1px solid var(--linea);
    background: var(--tarjeta);
  }
  .nota {
    margin-top: -10px;
    font-size: 12px;
    color: var(--tenue);
  }

  .tecnico {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 14px;
    border-top: 1px solid var(--linea);
  }
  dl {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 6px 12px;
    margin: 0;
    font-family: var(--mono);
    font-size: 11.5px;
    color: var(--tenue);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
    user-select: text;
  }

  .error {
    padding: 20px 22px;
    color: var(--peligro);
  }
  .caja-fantasma {
    height: 200px;
    border-radius: 14px;
    background: var(--chip);
  }
  .linea-fantasma {
    height: 14px;
    border-radius: 6px;
    background: var(--chip);
  }
</style>
