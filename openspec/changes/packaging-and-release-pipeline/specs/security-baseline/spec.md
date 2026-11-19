## MODIFIED Requirements

### Requirement: Ejecutable de release con elevación obligatoria
El ejecutable de la app compilado en perfil release SHALL embeber un manifest con el nivel de ejecución `requireAdministrator` y la dependencia de Common Controls v6, de modo que Windows pida UAC en cada arranque; si el usuario cancela el UAC, la app MUST NOT iniciar ni degradarse a un modo no elevado. Lo mismo SHALL cumplir el ejecutable que el instalador empaqueta y el que queda instalado en el equipo.

#### Scenario: Manifest del ejecutable de release
- **WHEN** la suite de tests, ejecutada en perfil release, inspecciona el manifest embebido en el ejecutable de la app
- **THEN** encuentra el nivel `requireAdministrator` y la dependencia `Microsoft.Windows.Common-Controls` versión 6

#### Scenario: Manifest del ejecutable de desarrollo
- **WHEN** la suite de tests, ejecutada en perfil de desarrollo, inspecciona el manifest embebido en el ejecutable de la app
- **THEN** encuentra el nivel `asInvoker`

#### Scenario: Manifest del ejecutable empaquetado
- **WHEN** tras generar el instalador se inspecciona el ejecutable producido por el empaquetado (el mismo que el instalador copia)
- **THEN** encuentra el nivel `requireAdministrator` y la dependencia `Microsoft.Windows.Common-Controls` versión 6

#### Scenario: Manifest del ejecutable instalado
- **WHEN** tras instalar la app se inspecciona el ejecutable en la carpeta de instalación
- **THEN** encuentra el nivel `requireAdministrator`

#### Scenario: Ruta del ejecutable a inspeccionar ausente
- **WHEN** se pide inspeccionar el ejecutable empaquetado o instalado sin indicar una ruta existente
- **THEN** la verificación falla con un mensaje que lo indica, sin informar éxito

#### Scenario: UAC cancelado
- **WHEN** el usuario abre el ejecutable de release y cancela el aviso de UAC
- **THEN** la app no inicia y no queda ningún proceso de la app en ejecución

#### Scenario: UAC aprobado
- **WHEN** el usuario abre el ejecutable de release y aprueba el aviso de UAC
- **THEN** la app inicia con el token elevado
