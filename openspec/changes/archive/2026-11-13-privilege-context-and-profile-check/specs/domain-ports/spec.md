# Spec Delta

## ADDED Requirements

### Requirement: Identidad de la sesión
El dominio SHALL definir un puerto que informe, de forma independiente y cada una con su propio error tipado, el SID del usuario del token del proceso, si ese token está elevado, el SID del dueño del shell de la sesión del proceso y el SID del usuario de logon de esa sesión; una señal inexistente MUST informarse como ausente y no como error.

#### Scenario: Shell no disponible
- **WHEN** la sesión del proceso no tiene proceso de shell
- **THEN** el puerto informa esa señal como ausente y las demás con su valor

#### Scenario: Fallo de una consulta
- **WHEN** falla la lectura del usuario de logon de la sesión
- **THEN** el puerto devuelve un error tipado solo para esa consulta, sin afectar las demás

### Requirement: Doble de prueba de la identidad de la sesión
El puerto de identidad de la sesión SHALL tener un doble en memoria configurable por respuesta (valor, ausente o error) para cada consulta, excluido del binario de producción.

#### Scenario: Caso over-the-shoulder simulado
- **WHEN** un test configura el doble con token de `admin` elevado y ambas señales de sesión de `ana`
- **THEN** el doble devuelve exactamente esos valores sin consultar el sistema operativo
