# Spec Delta

## MODIFIED Requirements

### Requirement: Sin elevación forzada en el entorno de desarrollo
Los binarios de test y el ejecutable compilado en perfil de desarrollo MUST NOT solicitar elevación al iniciar, de modo que `cargo test` y la app en desarrollo corran desde una terminal no elevada; su manifest embebido SHALL declarar el nivel `asInvoker` y la dependencia de Common Controls v6.

#### Scenario: Ejecución no elevada
- **WHEN** se ejecuta la app en desarrollo o `cargo test` desde una terminal no elevada
- **THEN** el proceso inicia sin el error 740 ("la operación solicitada requiere elevación")

#### Scenario: Manifest de un binario de test
- **WHEN** la suite de tests inspecciona el manifest embebido en su propio binario de test
- **THEN** encuentra el nivel `asInvoker` y la dependencia `Microsoft.Windows.Common-Controls` versión 6

## ADDED Requirements

### Requirement: Ejecutable de release con elevación obligatoria
El ejecutable de la app compilado en perfil release SHALL embeber un manifest con el nivel de ejecución `requireAdministrator` y la dependencia de Common Controls v6, de modo que Windows pida UAC en cada arranque; si el usuario cancela el UAC, la app MUST NOT iniciar ni degradarse a un modo no elevado.

#### Scenario: Manifest del ejecutable de release
- **WHEN** la suite de tests, ejecutada en perfil release, inspecciona el manifest embebido en el ejecutable de la app
- **THEN** encuentra el nivel `requireAdministrator` y la dependencia `Microsoft.Windows.Common-Controls` versión 6

#### Scenario: Manifest del ejecutable de desarrollo
- **WHEN** la suite de tests, ejecutada en perfil de desarrollo, inspecciona el manifest embebido en el ejecutable de la app
- **THEN** encuentra el nivel `asInvoker`

#### Scenario: UAC cancelado
- **WHEN** el usuario abre el ejecutable de release y cancela el aviso de UAC
- **THEN** la app no inicia y no queda ningún proceso de la app en ejecución

#### Scenario: UAC aprobado
- **WHEN** el usuario abre el ejecutable de release y aprueba el aviso de UAC
- **THEN** la app inicia con el token elevado

### Requirement: Inspección del manifest sin dependencias nuevas
La verificación del manifest embebido SHALL leer el recurso de manifest del ejecutable con código propio, sin crates nuevos ni herramientas externas, y MUST fallar (sin pánico ni falso positivo) si el archivo no es un ejecutable válido o no contiene un manifest.

#### Scenario: Archivo que no es un ejecutable
- **WHEN** se inspecciona un archivo de texto o un ejecutable truncado
- **THEN** la inspección devuelve un error descriptivo y no informa ningún nivel de ejecución

#### Scenario: Texto del manifest fuera del recurso
- **WHEN** el ejecutable contiene la cadena `requireAdministrator` fuera de su recurso de manifest
- **THEN** la inspección informa solo el nivel declarado en el recurso de manifest
