import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { DEFAULT_SCREEN, type ScreenId } from "../navigation";

interface NavigationValue {
  current: ScreenId;
  navigate: (id: ScreenId) => void;
}

const NavigationContext = createContext<NavigationValue | null>(null);

export function NavigationProvider({ children }: { children: ReactNode }) {
  const [current, setCurrent] = useState<ScreenId>(DEFAULT_SCREEN);
  const moveFocus = useRef(false);

  const navigate = useCallback((id: ScreenId) => {
    moveFocus.current = true;
    setCurrent(id);
  }, []);

  // Tras navegar, el foco pasa a <main id="main"> para que teclado y lectores sigan el cambio.
  useEffect(() => {
    if (!moveFocus.current) return;
    moveFocus.current = false;
    document.getElementById("main")?.focus();
  }, [current]);

  const value = useMemo(() => ({ current, navigate }), [current, navigate]);
  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
}

export function useNavigation(): NavigationValue {
  const value = useContext(NavigationContext);
  if (value === null) throw new Error("useNavigation debe usarse dentro de NavigationProvider");
  return value;
}
