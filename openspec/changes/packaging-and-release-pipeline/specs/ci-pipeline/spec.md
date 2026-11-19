## Purpose

Verificar automáticamente, fuera de la máquina del dueño, que cada cambio propuesto al repositorio público cumple los gates de calidad, seguridad y empaquetado del proyecto, y que el propio pipeline no amplía permisos ni ejecuta código de terceros no fijado.

## ADDED Requirements

### Requirement: Disparadores del pipeline
El pipeline de CI SHALL ejecutarse en cada pull request (cualquier rama base), en cada push a `main` y a pedido manual, sobre un runner de Windows x64 con versión de imagen fijada; MUST NOT ejecutarse con el disparador `pull_request_target` ni en eventos programados.

#### Scenario: Pull request abierto
- **WHEN** se abre o actualiza un pull request contra cualquier rama
- **THEN** el pipeline completo corre sobre el commit de merge del PR y su resultado aparece como check del PR

#### Scenario: Push a main
- **WHEN** se fusiona un cambio en `main`
- **THEN** el pipeline completo corre sobre el commit resultante

#### Scenario: Ejecución repetida en la misma rama
- **WHEN** se empuja un commit nuevo a un PR mientras su ejecución anterior sigue en curso
- **THEN** la ejecución anterior se cancela y solo cuenta la del último commit

### Requirement: Gates de calidad Rust
El pipeline SHALL fallar si el código Rust no respeta el formato (`cargo fmt --check`), si clippy reporta cualquier advertencia con todas las features y targets (`-D warnings`) o si falla cualquier test no ignorado de `cargo test`, ejecutado con el lockfile bloqueado (`--locked`).

#### Scenario: Test fallido
- **WHEN** un commit hace fallar un test de Rust no ignorado
- **THEN** el job de tests de Rust termina en rojo y el check del PR queda en rojo

#### Scenario: Advertencia de clippy
- **WHEN** un commit introduce código que clippy reporta como advertencia
- **THEN** el job termina en rojo

#### Scenario: Lockfile desactualizado
- **WHEN** `Cargo.toml` cambia sin actualizar `Cargo.lock`
- **THEN** el pipeline falla por `--locked` en lugar de resolver dependencias nuevas

#### Scenario: Tests ignorados
- **WHEN** corre la suite de Rust en CI
- **THEN** los tests marcados como ignorados (estado real de la máquina, privilegios, hardware) no se ejecutan y no afectan el resultado

### Requirement: Gates de frontend
El pipeline SHALL instalar las dependencias de npm exactamente desde `package-lock.json` y fallar si falla `npm test` o `npm run build` (incluida la verificación de tipos).

#### Scenario: Test de Vitest fallido
- **WHEN** un commit hace fallar un test de Vitest
- **THEN** el job de frontend termina en rojo

#### Scenario: Error de tipos
- **WHEN** un commit introduce un error de TypeScript
- **THEN** `npm run build` falla y el job termina en rojo

#### Scenario: Lockfile de npm inconsistente
- **WHEN** `package.json` y `package-lock.json` no coinciden
- **THEN** la instalación falla en lugar de actualizar el lockfile

### Requirement: Gate de ausencia de crates de red
El pipeline SHALL ejecutar la auditoría del grafo de dependencias de Windows x64 (denylist de red, TLS, telemetría y plugins de red/shell/fs) como paso identificable por nombre, de modo que un crate prohibido ponga el pipeline en rojo con un mensaje que lo nombra.

#### Scenario: Crate de red agregado
- **WHEN** un commit agrega como dependencia directa o transitiva para Windows un crate de la denylist (p. ej. `ureq`) y actualiza el lockfile
- **THEN** el paso de auditoría de red falla listando el crate y el pipeline queda en rojo

#### Scenario: Grafo limpio
- **WHEN** el grafo de Windows no contiene crates de la denylist
- **THEN** el paso pasa

### Requirement: Manifest de release verificado en CI
El pipeline SHALL compilar el ejecutable de la app en perfil release y fallar si su manifest no declara `requireAdministrator` con Common Controls v6, sin ejecutar ese ejecutable.

#### Scenario: Elevación quitada por error
- **WHEN** un commit elimina o cambia el nivel de elevación del ejecutable de release
- **THEN** el job de manifest de release termina en rojo

### Requirement: Auditoría de vulnerabilidades de dependencias
El pipeline SHALL auditar las dependencias de Cargo contra la base de avisos de RustSec y las de npm contra la base del registro, y fallar según la política: cualquier vulnerabilidad en Cargo; severidad alta o crítica en npm (todas las dependencias) y moderada o mayor en dependencias de producción de npm. Los avisos informativos (sin mantenimiento, unsound, yanked) se muestran sin fallar.

#### Scenario: Vulnerabilidad en un crate
- **WHEN** `Cargo.lock` contiene un crate afectado por un aviso de vulnerabilidad de RustSec que no está en la lista de excepciones
- **THEN** el paso de auditoría de Cargo falla nombrando el aviso

#### Scenario: Vulnerabilidad alta en npm
- **WHEN** una dependencia de npm (de producción o de desarrollo) tiene una vulnerabilidad de severidad alta
- **THEN** el paso de auditoría de npm falla

#### Scenario: Vulnerabilidad moderada en una dependencia de desarrollo
- **WHEN** solo una dependencia de desarrollo de npm tiene una vulnerabilidad moderada
- **THEN** la auditoría la informa sin poner el pipeline en rojo

#### Scenario: Aviso informativo
- **WHEN** un crate tiene solo un aviso de "sin mantenimiento"
- **THEN** la auditoría lo informa sin poner el pipeline en rojo

### Requirement: Excepciones de auditoría versionadas y justificadas
Toda excepción a la auditoría de Cargo SHALL declararse en un archivo de configuración versionado, con el identificador del aviso y un comentario que justifique por qué no afecta al binario de Windows; MUST NOT existir excepciones pasadas por la línea de comandos del workflow.

#### Scenario: Excepción sin justificación
- **WHEN** se revisa el archivo de excepciones
- **THEN** cada identificador ignorado tiene su justificación en el comentario adyacente y fue aprobado por el dueño

#### Scenario: Excepción en el workflow
- **WHEN** el test de política revisa los workflows
- **THEN** ningún paso pasa avisos a ignorar por argumento de línea de comandos

### Requirement: Instalador generado y verificado en CI
El pipeline SHALL generar el instalador de Windows a partir del commit verificado, comprobar que el ejecutable que empaqueta declara `requireAdministrator` y conservar el instalador como artefacto de la ejecución con retención acotada.

#### Scenario: Instalador disponible para el smoke
- **WHEN** el pipeline de un PR termina en verde
- **THEN** el instalador del commit está disponible como artefacto de esa ejecución, junto con su suma SHA-256

#### Scenario: Ejecutable empaquetado sin elevación
- **WHEN** el ejecutable producido por el empaquetado no declara `requireAdministrator`
- **THEN** el job de empaquetado termina en rojo y no se conserva el artefacto

### Requirement: Permisos mínimos y acciones fijadas
Cada workflow SHALL declarar `permissions:` en su nivel superior con solo lectura de contenidos, conceder escritura únicamente al job que publica un borrador de release, fijar cada acción externa por SHA completo de commit y no persistir credenciales de git en el checkout.

#### Scenario: Acción fijada por tag
- **WHEN** un workflow referencia una acción por tag o rama (p. ej. `@v4`) en lugar de un SHA de 40 caracteres hexadecimales
- **THEN** el test de política del pipeline falla nombrando el archivo y la línea

#### Scenario: Workflow sin permisos declarados
- **WHEN** un workflow no declara `permissions:` en su nivel superior
- **THEN** el test de política falla

#### Scenario: Escritura fuera del job de publicación
- **WHEN** un job distinto del de publicación del release solicita algún permiso de escritura
- **THEN** el test de política falla

#### Scenario: Disparador peligroso
- **WHEN** un workflow usa `pull_request_target`
- **THEN** el test de política falla

### Requirement: Sin secretos propios ni datos sensibles en el pipeline
El pipeline SHALL funcionar sin secretos propios del repositorio (solo el token efímero de la ejecución) y MUST NOT imprimir el token ni interpolar datos controlados por terceros (títulos, ramas, cuerpos de PR) directamente en scripts.

#### Scenario: PR desde un fork
- **WHEN** un PR llega desde un fork del repositorio público
- **THEN** el pipeline corre con token de solo lectura, sin acceso a secretos, y no puede crear tags ni releases

#### Scenario: Datos del evento en un script
- **WHEN** el test de política revisa los pasos `run:` de los workflows
- **THEN** ninguno interpola expresiones `${{ github.event.* }}` ni `${{ github.head_ref }}` dentro del script

### Requirement: Gate demostrado en rojo
El pipeline SHALL haberse observado en rojo, antes de cerrar el change, ante un test fallido y ante un crate de la denylist, en ramas descartables cuyos cambios nunca se fusionan; la evidencia (ejecución, paso fallido y mensaje) MUST quedar registrada en el change.

#### Scenario: Demostración con test fallido
- **WHEN** se abre un PR borrador desde una rama descartable con un test que falla a propósito
- **THEN** el check del PR queda en rojo en el paso de tests y el PR se cierra sin fusionar

#### Scenario: Demostración con crate de red
- **WHEN** se abre un PR borrador desde una rama descartable que agrega un crate de la denylist
- **THEN** el check queda en rojo en el paso de auditoría de red nombrando el crate y el PR se cierra sin fusionar
