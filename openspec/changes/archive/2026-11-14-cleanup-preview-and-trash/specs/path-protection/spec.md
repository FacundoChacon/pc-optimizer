# Spec Delta

## MODIFIED Requirements

### Requirement: Re-verificación anti-TOCTOU antes de borrar
Antes de mover un elemento a la Papelera, el sistema SHALL fijar los ancestros de su ruta real contra renombres y borrados, repetir la verificación completa y comparar la identidad de archivo (volumen e id) y el tipo con los del escaneo; cualquier diferencia o falla MUST dar `protected` y la fijación MUST mantenerse hasta terminar el movimiento. Una re-verificación `allowed` MUST informar el tamaño y la última escritura leídos del elemento abierto, no de un listado de carpeta.

#### Scenario: Archivo reemplazado por un symlink
- **WHEN** entre el escaneo y la ejecución `a.tmp` se reemplaza por un symlink hacia `C:\Windows\System32\x.dll`
- **THEN** la re-verificación da `protected` y el elemento no se mueve

#### Scenario: Archivo reemplazado por otro con el mismo nombre
- **WHEN** entre el escaneo y la ejecución `a.tmp` se borra y se crea otro `a.tmp`
- **THEN** la re-verificación da `protected` con motivo `identity_changed`

#### Scenario: Carpeta ancestro reemplazada por una junction
- **WHEN** entre el escaneo y la ejecución la carpeta padre se reemplaza por una junction hacia `C:\Windows`
- **THEN** la re-verificación da `protected`

#### Scenario: Ancestro fijado durante el movimiento
- **WHEN** la re-verificación dio `allowed` y la fijación sigue vigente
- **THEN** ningún otro proceso puede renombrar ni borrar los ancestros fijados hasta que se libera

#### Scenario: Ruta a mover
- **WHEN** la re-verificación da `allowed`
- **THEN** la ruta que se entrega para mover a la Papelera es la ruta real verificada y no la ruta original recibida

#### Scenario: Tamaño y fecha del elemento abierto
- **WHEN** la re-verificación da `allowed` para un archivo abierto para escritura por otro proceso cuya entrada de carpeta todavía muestra una fecha anterior
- **THEN** el resultado informa el tamaño y la última escritura vigentes del archivo abierto
