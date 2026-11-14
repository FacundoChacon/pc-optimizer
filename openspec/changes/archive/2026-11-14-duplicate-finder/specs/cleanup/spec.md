## ADDED Requirements

### Requirement: Ítems de otras fuentes con la misma re-verificación
La ejecución de una limpieza SHALL aceptar, además de hallazgos de basura, ítems de otras fuentes declaradas (hoy: copias duplicadas, categoría `duplicates`, de alcance por-usuario), cada uno con ruta real, tamaño, fecha e identidad persistidos, y aplicarles exactamente la misma re-verificación, guardas, Papelera, abortos y registro que a la basura.

#### Scenario: Copia duplicada en mismatch
- **WHEN** se confirma un preview de duplicados creado en `match` y al confirmar el contexto está en mismatch
- **THEN** cada copia queda `skipped_protected` con detalle `blocked_different_user` o `blocked_undetermined`, sin fijar ni mover nada del perfil

#### Scenario: Copia duplicada cambiada desde el escaneo
- **WHEN** una copia a eliminar cambió de tamaño o de fecha desde el escaneo
- **THEN** queda `skipped_locked` con detalle `changed_since_scan` y sigue en su lugar

#### Scenario: Registro con la categoría de la fuente
- **WHEN** se mueve una copia duplicada a la Papelera
- **THEN** su entrada de `cleanup_log` tiene categoría `duplicates` y el identificador del escaneo de duplicados

### Requirement: Verificación adicional por ítem con la fijación viva
Una fuente SHALL poder agregar una verificación propia que corre después de la re-verificación de ruta y de las guardas de cambio y de 24 h, y antes de mover, mientras los ancestros siguen fijados. La verificación MAY omitir el ítem con un resultado (`skipped_locked` o `skipped_protected`) y un código de detalle estable, o interrumpirlo por cancelación; la limpieza de basura MUST NOT tener verificación adicional.

#### Scenario: Ítem omitido por la verificación adicional
- **WHEN** la verificación adicional de un ítem lo rechaza con `skipped_locked` y detalle `content_changed`
- **THEN** el ítem no se mueve, se registra con ese resultado y detalle y la ejecución continúa con el siguiente

#### Scenario: Interrupción por cancelación
- **WHEN** el usuario cancela mientras corre la verificación adicional del ítem 3 de 5
- **THEN** el ítem 3 no se mueve ni se registra, los ítems 4 y 5 no se procesan y el resultado es `cancelled` con 3 no procesados

#### Scenario: Fijación durante la verificación
- **WHEN** corre la verificación adicional de un ítem
- **THEN** los ancestros de su ruta real siguen fijados y se liberan recién después de moverlo o descartarlo
