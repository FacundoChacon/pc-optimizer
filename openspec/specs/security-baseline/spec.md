# security-baseline Specification

## Purpose
Fija las garantías de seguridad de base de pc-optimizer (app local con privilegios altos y repo público) como verificaciones automáticas que fallan la suite de tests si alguna se rompe: CSP, capabilities, ausencia de red y pureza del dominio.

## Requirements

### Requirement: CSP estricta en producción
La política de seguridad de contenido de producción SHALL permitir scripts, estilos, imágenes y fuentes solo desde el propio origen de la app, MUST NOT incluir `unsafe-eval` ni `unsafe-inline`, MUST NOT permitir orígenes remotos y SHALL bloquear objetos embebidos, frames ancestros y envío de formularios.

#### Scenario: CSP válida
- **WHEN** la suite de tests valida la configuración de la app
- **THEN** la CSP de producción contiene `default-src 'self'` y `object-src 'none'` y no contiene `unsafe-eval`, `unsafe-inline` ni ningún origen `http:`, `https:`, `ws:` o `wss:` distinto del canal IPC local

#### Scenario: CSP debilitada
- **WHEN** alguien agrega `unsafe-eval` o un origen remoto a la CSP de producción
- **THEN** la suite de tests falla indicando la directiva infractora

### Requirement: Conexiones limitadas al canal IPC local
La directiva `connect-src` de producción SHALL permitir únicamente el canal IPC local de Tauri (`ipc:` y `http://ipc.localhost`) y ningún otro destino.

#### Scenario: connect-src con destino extra
- **WHEN** `connect-src` incluye cualquier fuente además de `ipc:` y `http://ipc.localhost`
- **THEN** la suite de tests falla

### Requirement: CSP de desarrollo restringida a localhost
La CSP de desarrollo SHALL permitir como orígenes adicionales únicamente el servidor de desarrollo local (`localhost`), y MUST NOT permitir `unsafe-eval` ni orígenes remotos.

#### Scenario: CSP de desarrollo con origen remoto
- **WHEN** la CSP de desarrollo incluye un origen que no es `localhost` ni el canal IPC
- **THEN** la suite de tests falla

### Requirement: Capabilities mínimas sin plugins peligrosos
Las capabilities de la app SHALL aplicar solo a la ventana principal y MUST NOT otorgar permisos de los plugins `shell`, `fs`, `http`, `process`, `updater` ni `opener`; la API global de Tauri MUST NOT exponerse en `window`.

#### Scenario: Permiso prohibido agregado
- **WHEN** una capability incluye un permiso con prefijo `shell:`, `fs:`, `http:`, `process:`, `updater:` u `opener:`
- **THEN** la suite de tests falla indicando el permiso

#### Scenario: API global deshabilitada
- **WHEN** la suite de tests valida la configuración
- **THEN** `withGlobalTauri` es `false`

### Requirement: Sin dependencias con red ni telemetría
El grafo de dependencias Rust efectivamente compilado para Windows x64 SHALL NOT contener crates de clientes/servidores HTTP, WebSocket, QUIC, pilas TLS, SDKs de telemetría ni plugins de Tauri de red, shell o filesystem.

#### Scenario: Grafo limpio
- **WHEN** se ejecuta la suite de tests sobre el lockfile actual
- **THEN** el test de auditoría de dependencias pasa sin reportar crates prohibidos

#### Scenario: Crate de red introducido
- **WHEN** una dependencia directa o transitiva para Windows incorpora un crate de la denylist (p. ej. `reqwest`, `hyper`, `ureq`, `rustls`)
- **THEN** el test falla listando cada crate prohibido encontrado

#### Scenario: Dependencias solo de otras plataformas
- **WHEN** un crate de la denylist aparece en el lockfile únicamente como dependencia de otra plataforma (p. ej. iOS/Android)
- **THEN** el test no lo reporta, porque no forma parte del binario de Windows

### Requirement: SQLite embebido
La dependencia de SQLite SHALL compilarse con la biblioteca SQLite embebida (bundled), sin depender de una SQLite del sistema.

#### Scenario: Feature bundled activa
- **WHEN** se audita el grafo de dependencias para Windows x64
- **THEN** la biblioteca nativa de SQLite figura con la feature `bundled` activada

### Requirement: Dominio puro sin I/O
Los módulos de la capa de dominio MUST NOT importar APIs de sistema de archivos, procesos, red, Windows, SQLite ni Tauri.

#### Scenario: Import prohibido en dominio
- **WHEN** un archivo bajo la capa de dominio importa `std::fs`, `std::process`, `std::net`, `windows`, `rusqlite` o `tauri`
- **THEN** la suite de tests falla indicando archivo e import

### Requirement: Sin elevación forzada en el entorno de desarrollo
Mientras no se implemente el modelo de privilegios (C-05), el ejecutable MUST NOT solicitar elevación al iniciar, de modo que el ciclo de desarrollo y los tests corran desde una terminal no elevada.

#### Scenario: Ejecución no elevada
- **WHEN** se ejecuta la app en desarrollo o `cargo test` desde una terminal no elevada
- **THEN** el proceso inicia sin el error 740 ("la operación solicitada requiere elevación")
