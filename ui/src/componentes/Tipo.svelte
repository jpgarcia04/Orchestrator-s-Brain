<!--
  La etiqueta del tipo de archivo: PDF, IMG, DOC… Su color dice el tipo antes
  de leer el nombre, y es el mismo en toda la aplicación.
-->
<script lang="ts">
  import { colorDeTipo, type Tipo } from "../lib/nombres";

  let { tipo }: { tipo: Tipo } = $props();
</script>

<span class="tipo" style:color={colorDeTipo(tipo)}>{tipo}</span>

<style>
  .tipo {
    flex-shrink: 0;
    width: 34px;
    padding: 3px 0;
    border-radius: 6px;
    background: var(--chip);
    font-family: var(--mono);
    font-size: 10.5px;
    font-weight: 500;
    text-align: center;
  }
</style>
