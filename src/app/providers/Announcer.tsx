import { createContext, type ReactNode, useContext, useState } from "react";

const AnnounceContext = createContext<((text: string) => void) | null>(null);

/** Region aria-live unica: cada anuncio reemplaza al anterior. */
export function Announcer({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState("");
  return (
    <AnnounceContext.Provider value={setMessage}>
      {children}
      <div role="status" aria-live="polite" className="sr-only">
        {message}
      </div>
    </AnnounceContext.Provider>
  );
}

export function useAnnounce(): (text: string) => void {
  const value = useContext(AnnounceContext);
  if (value === null) throw new Error("useAnnounce debe usarse dentro de Announcer");
  return value;
}
