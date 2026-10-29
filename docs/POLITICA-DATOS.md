# Política Permanente de Separación de Datos Personales y Desarrollo — Orchestrator's Brain

Esta política establece la separación estricta entre el código público del proyecto y los datos personales del usuario, garantizando que el uso cotidiano de Brain y su desarrollo continuo coexistan con total seguridad y privacidad.

---

## 1. Principio fundamental de aislamiento

1. **GitHub contiene exclusivamente código, documentación y pruebas sintéticas:**
   - Queda terminantemente prohibido versionar rutas fijas de usuario, credenciales, llaves de cifrado, bases de datos reales o documentos personales.
   - Cualquier material visual o de prueba (capturas de pantalla, archivos PDF de prueba, registros o fixtures) debe generarse exclusivamente mediante la biblioteca sintética (`tools/biblioteca-sintetica.ps1`).

2. **Los datos personales reales viven en el perfil personal de Windows:**
   - La base de conocimiento activa (`brain.obrain`), su llave de cifrado (`brain.obrain.llave`) y los modelos de embeddings viven fuera del repositorio Git, en `%USERPROFILE%\.obrain` ([ADR-0029](adr/0029-una-carpeta-fija-fuera-de-appdata.md)).
   - La ejecución habitual de la aplicación instalada o de la CLI no escribe en el directorio del repositorio.

---

## 2. Reglas para desarrolladores y agentes de IA

1. **Bases y perfiles aislados para desarrollo y pruebas:**
   - Todo desarrollo, depuración o ejecución de pruebas automatizadas debe realizarse sobre bases temporales aisladas generadas en `%TEMP%`, nunca sobre `%USERPROFILE%\.obrain`.
   - Para ejecutar la CLI o la aplicación en desarrollo sin tocar el perfil personal, debe definirse la variable de entorno `OBRAIN_DB` o utilizar el parámetro `--db`:
     ```powershell
     $env:OBRAIN_DB = "$env:TEMP\obrain-dev\dev.obrain"
     ```
   - Las pruebas automatizadas en Rust deben seguir utilizando el arnés `CarpetaPrueba`, que genera directorios efímeros únicos y los destruye al terminar.

2. **Protección de la instalación personal y migraciones:**
   - La base personal de producción no debe someterse a migraciones automáticas silenciosas ni ensayos destructivos desde herramientas de desarrollo.
   - Cualquier cambio de esquema o migración sobre la base real requiere:
     1. **Respaldo previo verificado obligatorio:**
        ```powershell
        obrain respaldar "C:\Ruta\Segura\Respaldos\previo-migracion"
        ```
     2. **Autorización explícita del usuario** antes de aplicar la actualización o restauración sobre el perfil real.

---

## 3. Salvaguardas automáticas en el repositorio

1. **Protección en `.gitignore`:**
   El archivo `.gitignore` bloquea de forma preventiva cualquier archivo generado por Brain que pudiera crearse accidentalmente en el espacio de trabajo:
   - Archivos de base de datos: `*.obrain`, `*.obrain-*`, `*.obrain.*`, `*.sqlite*`, `*.db`.
   - Llaves criptográficas: `*.llave`.
   - Carpetas de perfil y temporales: `.obrain/`, `.obrain-*/`.
   - Respaldos y salvaguardas: `*obrain-respaldo*`, `*obrain-ensayo*`, `*obrain-salvaguarda*`, `*.transicion.json`.
   - Modelos y directorios de compilación externa: `*.onnx`, `/models/`, `obrain-target/`.
   - Registros de ejecución: `*.log`.

2. **Verificación previa a commits (`./tools/verificar-local.ps1`):**
   Antes de preparar o enviar cambios, el script de verificación local comprueba automáticamente:
   - Que ningún archivo preparado para commit (*staged*) contenga extensiones privadas prohibidas (`.llave`, `.obrain`, `.log`, `.onnx`, etc.).
   - Que ningún archivo en preparación pertenezca a directorios de perfil o respaldo.
   - Que los cambios no introduzcan rutas absolutas de usuario anfitrión hardcodeadas.
