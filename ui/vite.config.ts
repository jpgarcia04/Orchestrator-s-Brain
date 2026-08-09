import { defineConfig } from "vite";

// Puerto fijo: `tauri.conf.json` apunta aquí y un puerto que baila rompe el
// arranque de la aplicación sin decir por qué.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 5183,
    strictPort: true,
    // `src-tauri/` está dentro de la raíz de Vite, así que su vigilante intenta
    // observar `target/` — y mientras cargo escribe el ejecutable, Windows lo
    // tiene bloqueado: `EBUSY: resource busy or locked, watch obrain_app.exe`,
    // y Vite se cae con él. No es cosmético: sin esto la aplicación no arranca.
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: { target: "chrome110", sourcemap: true },
});
