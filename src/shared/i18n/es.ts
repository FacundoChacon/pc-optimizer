/** Catalogo centralizado de textos de UI (espanol). Ningun componente define literales propios. */
export const es = {
  app: {
    title: "PC Optimizer",
    versionLabel: "Versión",
    loading: "Cargando…",
    ipcError: "No se pudo comunicar con el backend de la aplicación.",
  },
  nav: {
    label: "Navegación principal",
    home: "Inicio",
    settings: "Configuración",
  },
  home: {
    title: "Inicio",
    emptyTitle: "Todavía no hay nada para mostrar",
    emptyBody: "Cuando haya análisis disponibles, el resumen de tu equipo aparecerá acá.",
  },
  settings: {
    title: "Configuración",
    appearance: {
      heading: "Apariencia",
      modeLegend: "Modo de la interfaz",
      simple: "Simple",
      simpleDescription: "Pocas opciones, textos claros y una sola acción principal.",
      advanced: "Avanzado",
      advancedDescription: "Más información y controles en pantalla, con menos espacio entre ellos.",
      announceSimple: "Modo simple activado.",
      announceAdvanced: "Modo avanzado activado.",
      needsRecovery: "Primero recuperá la base de datos para poder guardar este cambio.",
    },
    autostart: {
      heading: "Inicio con Windows",
      label: "Iniciar con Windows en la bandeja",
      unavailable: "Disponible en una próxima versión.",
    },
    database: {
      heading: "Base de datos",
      statusLabel: "Estado",
      ready: "Lista",
      unavailable: "No disponible",
      recoverAction: "Respaldar y recrear",
      confirmTitle: "¿Recrear la base de datos?",
      confirmBody:
        "Se guardará una copia de la base actual junto a la nueva y se empezará de cero. No se borra nada.",
      confirmAction: "Respaldar y recrear",
      cancel: "Cancelar",
      working: "Recreando…",
      recovered: "Base de datos recreada. La copia anterior se guardó como {file}.",
      recoverRetry:
        "No se pudo recrear la base. Cerrá otros programas que la estén usando y reintentá.",
    },
    about: {
      heading: "Acerca de",
      nameLabel: "Aplicación",
      versionLabel: "Versión",
    },
  },
  store: {
    bannerTitle: "La base de datos local no está disponible",
    bannerAction: "Ir a Configuración",
    reasons: {
      corrupt: "La base de datos está dañada. Podés respaldarla y crear una nueva.",
      migration_failed:
        "No se pudo actualizar la base de datos. Podés respaldarla y crear una nueva.",
      schema_too_new:
        "La base de datos fue creada por una versión más nueva de la aplicación. Actualizá la aplicación para usarla.",
      unavailable:
        "No se puede acceder a la base de datos. Cerrá otros programas que la usen y reiniciá la aplicación.",
    },
  },
  errors: {
    internal: "Ocurrió un error inesperado.",
    unknown: "Ocurrió un error desconocido.",
    invalid_response: "La aplicación recibió una respuesta inesperada.",
    store_corrupt: "La base de datos local está dañada.",
    store_schema_too_new: "La base de datos fue creada por una versión más nueva de la aplicación.",
    store_migration_failed: "No se pudo actualizar la base de datos local.",
    store_unavailable: "No se puede acceder a la base de datos local.",
    store_not_recoverable: "La base de datos no necesita o no admite una recuperación.",
    setting_invalid: "El valor de ese ajuste no es válido.",
    setting_read_only: "Ese ajuste no se puede modificar todavía.",
  },
  a11y: {
    skipToContent: "Saltar al contenido",
    announcements: "Avisos",
  },
} as const;

export type Catalog = typeof es;
