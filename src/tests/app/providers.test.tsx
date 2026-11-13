import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SCREEN, SCREENS } from "../../app/navigation";
import { AppInfoProvider, useAppInfo } from "../../app/providers/AppInfoProvider";
import { Announcer, useAnnounce } from "../../app/providers/Announcer";
import { NavigationProvider, useNavigation } from "../../app/providers/NavigationProvider";
import { SettingsProvider, useSettings } from "../../app/providers/SettingsProvider";
import { PrivilegeProvider, usePrivilege } from "../../app/providers/PrivilegeProvider";
import { StoreStatusProvider, useStoreStatus } from "../../app/providers/StoreStatusProvider";
import type { AppError } from "../../lib/ipc";
import { CORRUPT, installBackend, OTHER_USER } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const unavailable: AppError = { kind: "store_unavailable", message: "x" };

describe("navegacion (7.1)", () => {
  it("SCREENS contiene exactamente home y settings con sus componentes", () => {
    expect(SCREENS.map((screen) => screen.id)).toEqual(["home", "settings"]);
    expect(DEFAULT_SCREEN).toBe("home");
    for (const def of SCREENS) expect(typeof def.Component).toBe("function");
  });

  it("current inicial es home y navigate cambia la pantalla y mueve el foco a #main", () => {
    function Probe() {
      const { current, navigate } = useNavigation();
      return (
        <>
          <button type="button" onClick={() => navigate("settings")}>
            ir
          </button>
          <main id="main" tabIndex={-1}>
            {current}
          </main>
        </>
      );
    }
    render(
      <NavigationProvider>
        <Probe />
      </NavigationProvider>,
    );
    expect(screen.getByRole("main")).toHaveTextContent("home");
    expect(screen.getByRole("main")).not.toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "ir" }));
    expect(screen.getByRole("main")).toHaveTextContent("settings");
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("useNavigation fuera del provider lanza un error claro", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => renderHook(() => useNavigation())).toThrow(/NavigationProvider/);
    spy.mockRestore();
  });
});

describe("SettingsProvider (7.2)", () => {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <SettingsProvider>{children}</SettingsProvider>
  );

  it("pasa de loading a ready con la configuracion del backend", async () => {
    installBackend({ settings: { uiMode: "advanced", autostartTray: false, language: "es" } });
    const { result } = renderHook(() => useSettings(), { wrapper });
    expect(result.current.phase).toBe("loading");
    await waitFor(() => expect(result.current.phase).toBe("ready"));
    expect(result.current.settings.uiMode).toBe("advanced");
  });

  it("error de get_settings deja failed con defaults simple", async () => {
    installBackend({ failures: { get_settings: unavailable } });
    const { result } = renderHook(() => useSettings(), { wrapper });
    await waitFor(() => expect(result.current.phase).toBe("failed"));
    expect(result.current.settings.uiMode).toBe("simple");
  });

  it("setUiMode llama set_setting, marca pending y aplica lo devuelto", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const backend = installBackend();
    const { result } = renderHook(() => useSettings(), { wrapper });
    await waitFor(() => expect(result.current.phase).toBe("ready"));
    backend.state.gates.set_setting = gate;
    act(() => {
      void result.current.setUiMode("advanced");
    });
    await waitFor(() => expect(result.current.pending).toBe(true));
    expect(result.current.settings.uiMode).toBe("simple");
    release();
    await waitFor(() => expect(result.current.settings.uiMode).toBe("advanced"));
    expect(result.current.pending).toBe(false);
    expect(backend.calls.find((call) => call.cmd === "set_setting")?.args).toEqual({
      key: "ui_mode",
      value: "advanced",
    });
  });

  it("si set_setting falla deja el modo y expone lastError", async () => {
    const backend = installBackend();
    const { result } = renderHook(() => useSettings(), { wrapper });
    await waitFor(() => expect(result.current.phase).toBe("ready"));
    backend.state.failures.set_setting = unavailable;
    await act(async () => {
      await result.current.setUiMode("advanced");
    });
    expect(result.current.settings.uiMode).toBe("simple");
    expect(result.current.lastError?.kind).toBe("store_unavailable");
    expect(result.current.pending).toBe(false);
  });

  it("reload vuelve a leer la configuracion", async () => {
    const backend = installBackend();
    const { result } = renderHook(() => useSettings(), { wrapper });
    await waitFor(() => expect(result.current.phase).toBe("ready"));
    backend.state.settings = { uiMode: "advanced", autostartTray: false, language: "es" };
    await act(async () => {
      await result.current.reload();
    });
    expect(result.current.settings.uiMode).toBe("advanced");
    expect(backend.count("get_settings")).toBe(2);
  });
});

describe("StoreStatusProvider (7.3)", () => {
  it("carga get_store_status", async () => {
    installBackend({ store: CORRUPT });
    const { result } = renderHook(() => useStoreStatus(), {
      wrapper: ({ children }) => <StoreStatusProvider onRecovered={() => {}}>{children}</StoreStatusProvider>,
    });
    expect(result.current.status).toBeNull();
    await waitFor(() => expect(result.current.status).toEqual(CORRUPT));
  });

  it("recover invoca recover_store una vez, refresca y dispara onRecovered", async () => {
    const onRecovered = vi.fn();
    const backend = installBackend({ store: CORRUPT });
    const { result } = renderHook(() => useStoreStatus(), {
      wrapper: ({ children }) => <StoreStatusProvider onRecovered={onRecovered}>{children}</StoreStatusProvider>,
    });
    await waitFor(() => expect(result.current.status).toEqual(CORRUPT));
    let outcome = "";
    await act(async () => {
      outcome = (await result.current.recover()).backupFileName;
    });
    expect(outcome).toBe("data.db.corrupt-1700000000000.bak");
    expect(backend.count("recover_store")).toBe(1);
    expect(result.current.status).toEqual({ status: "ready" });
    expect(onRecovered).toHaveBeenCalledTimes(1);
  });

  it("error de recover_store conserva el estado previo y lo propaga", async () => {
    const onRecovered = vi.fn();
    installBackend({ store: CORRUPT, failures: { recover_store: unavailable } });
    const { result } = renderHook(() => useStoreStatus(), {
      wrapper: ({ children }) => <StoreStatusProvider onRecovered={onRecovered}>{children}</StoreStatusProvider>,
    });
    await waitFor(() => expect(result.current.status).toEqual(CORRUPT));
    let caught: unknown;
    await act(async () => {
      await result.current.recover().catch((error: unknown) => {
        caught = error;
      });
    });
    expect(caught).toMatchObject({ kind: "store_unavailable" });
    expect(result.current.status).toEqual(CORRUPT);
    expect(onRecovered).not.toHaveBeenCalled();
  });

  it("dos recover seguidos con el primero pendiente invocan una sola vez", async () => {
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const backend = installBackend({ store: CORRUPT, gates: { recover_store: gate } });
    const { result } = renderHook(() => useStoreStatus(), {
      wrapper: ({ children }) => <StoreStatusProvider onRecovered={() => {}}>{children}</StoreStatusProvider>,
    });
    await waitFor(() => expect(result.current.status).toEqual(CORRUPT));
    let both: Promise<unknown> = Promise.resolve();
    act(() => {
      both = Promise.all([result.current.recover(), result.current.recover()]);
    });
    release();
    await act(async () => {
      await both;
    });
    expect(backend.count("recover_store")).toBe(1);
  });
});

describe("AppInfoProvider y Announcer (7.4)", () => {
  it("get_app_info se invoca una sola vez con dos consumidores", async () => {
    const backend = installBackend();
    function Consumer({ id }: { id: string }) {
      const info = useAppInfo();
      return <p data-testid={id}>{info.status === "ready" ? info.info.version : info.status}</p>;
    }
    render(
      <AppInfoProvider>
        <Consumer id="a" />
        <Consumer id="b" />
      </AppInfoProvider>,
    );
    await waitFor(() => expect(screen.getByTestId("a")).toHaveTextContent("0.1.0"));
    expect(screen.getByTestId("b")).toHaveTextContent("0.1.0");
    expect(backend.count("get_app_info")).toBe(1);
  });

  it("error de get_app_info da estado error sin lanzar", async () => {
    installBackend({ failures: { get_app_info: { kind: "internal", message: "x" } } });
    const { result } = renderHook(() => useAppInfo(), {
      wrapper: ({ children }) => <AppInfoProvider>{children}</AppInfoProvider>,
    });
    await waitFor(() => expect(result.current.status).toBe("error"));
  });

  it("announce escribe en la unica region status y el segundo anuncio reemplaza al primero", () => {
    function Button() {
      const announce = useAnnounce();
      return (
        <>
          <button type="button" onClick={() => announce("uno")}>
            a
          </button>
          <button type="button" onClick={() => announce("dos")}>
            b
          </button>
        </>
      );
    }
    render(
      <Announcer>
        <Button />
      </Announcer>,
    );
    const region = screen.getByRole("status");
    expect(region).toHaveAttribute("aria-live", "polite");
    expect(screen.getAllByRole("status")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "a" }));
    expect(region).toHaveTextContent("uno");
    fireEvent.click(screen.getByRole("button", { name: "b" }));
    expect(region).toHaveTextContent("dos");
    expect(region).not.toHaveTextContent("uno");
  });
});

describe("PrivilegeProvider (6.2)", () => {
  const wrapper = ({ children }: { children: ReactNode }) => <PrivilegeProvider>{children}</PrivilegeProvider>;
  const UNDETERMINED = { status: "mismatch", reason: "undetermined", elevated: false };

  it("pasa de loading a ready con el contexto del backend", async () => {
    installBackend({ privilege: OTHER_USER });
    const { result } = renderHook(() => usePrivilege(), { wrapper });
    expect(result.current.phase).toBe("loading");
    await waitFor(() => expect(result.current.phase).toBe("ready"));
    expect(result.current.context).toEqual(OTHER_USER);
  });

  it("consulta una sola vez aunque haya dos consumidores", async () => {
    const backend = installBackend();
    const { result } = renderHook(() => [usePrivilege(), usePrivilege()] as const, { wrapper });
    await waitFor(() => expect(result.current[0].phase).toBe("ready"));
    expect(result.current[1].context).toEqual(result.current[0].context);
    expect(backend.count("get_privilege_context")).toBe(1);
  });

  it("falla cerrado ante un error de IPC", async () => {
    installBackend({ failures: { get_privilege_context: { kind: "internal", message: "x" } } });
    const { result } = renderHook(() => usePrivilege(), { wrapper });
    await waitFor(() => expect(result.current.phase).toBe("ready"));
    expect(result.current.context).toEqual(UNDETERMINED);
  });

  it("falla cerrado ante una respuesta invalida", async () => {
    installBackend({ privilege: { status: "raro" } });
    const { result } = renderHook(() => usePrivilege(), { wrapper });
    await waitFor(() => expect(result.current.phase).toBe("ready"));
    expect(result.current.context).toEqual(UNDETERMINED);
  });

  it("usePrivilege fuera del provider lanza un error claro", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => renderHook(() => usePrivilege())).toThrow(/PrivilegeProvider/);
    vi.restoreAllMocks();
  });
});
