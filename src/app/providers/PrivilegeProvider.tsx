import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { getPrivilegeContext, type PrivilegeContext } from "../../lib/ipc";

/** Ante un fallo o una respuesta invalida no se afirma nada: se trata como "no verificado". */
const FAIL_CLOSED: PrivilegeContext = { status: "mismatch", reason: "undetermined", elevated: false };

interface PrivilegeValue {
  phase: "loading" | "ready";
  context: PrivilegeContext;
}

const PrivilegeContextValue = createContext<PrivilegeValue | null>(null);

/** Consulta una sola vez el contexto de privilegios (no cambia durante la ejecucion). */
export function PrivilegeProvider({ children }: { children: ReactNode }) {
  const [value, setValue] = useState<PrivilegeValue>({ phase: "loading", context: FAIL_CLOSED });

  useEffect(() => {
    let active = true;
    getPrivilegeContext()
      .then((context) => active && setValue({ phase: "ready", context }))
      .catch(() => active && setValue({ phase: "ready", context: FAIL_CLOSED }));
    return () => {
      active = false;
    };
  }, []);

  const memo = useMemo(() => value, [value]);
  return <PrivilegeContextValue.Provider value={memo}>{children}</PrivilegeContextValue.Provider>;
}

export function usePrivilege(): PrivilegeValue {
  const value = useContext(PrivilegeContextValue);
  if (value === null) throw new Error("usePrivilege debe usarse dentro de PrivilegeProvider");
  return value;
}
