import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  getAppInfo,
  getPrivilegeContext,
  getSettings,
  getStoreStatus,
  recoverStore,
  setSetting,
  toAppError,
} from "../lib/ipc";

describe("toAppError", () => {
  it("preserva un objeto { kind, message } valido", () => {
    expect(toAppError({ kind: "internal", message: "boom" })).toEqual({
      kind: "internal",
      message: "boom",
    });
  });

  it("normaliza un string a kind unknown conservando el texto", () => {
    expect(toAppError("fallo crudo")).toEqual({ kind: "unknown", message: "fallo crudo" });
  });

  it("normaliza null a kind unknown con mensaje no vacio", () => {
    const err = toAppError(null);
    expect(err.kind).toBe("unknown");
    expect(err.message.length).toBeGreaterThan(0);
  });

  it("normaliza un Error usando su message", () => {
    expect(toAppError(new Error("exploto"))).toEqual({ kind: "unknown", message: "exploto" });
  });

  it("normaliza un objeto sin message a kind unknown con mensaje no vacio", () => {
    const err = toAppError({ kind: "internal" });
    expect(err.kind).toBe("unknown");
    expect(err.message.length).toBeGreaterThan(0);
  });
});

describe("getAppInfo", () => {
  afterEach(() => {
    clearMocks();
  });

  it("devuelve la respuesta tipada e invoca exactamente get_app_info", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      return { name: "pc-optimizer", version: "0.1.0" };
    });
    await expect(getAppInfo()).resolves.toEqual({ name: "pc-optimizer", version: "0.1.0" });
    expect(calls).toEqual(["get_app_info"]);
  });

  it("propaga otra version distinta (no hardcodeada)", async () => {
    mockIPC(() => ({ name: "pc-optimizer", version: "9.9.9" }));
    await expect(getAppInfo()).resolves.toMatchObject({ version: "9.9.9" });
  });

  it("rechaza con AppError cuando el backend devuelve { kind, message }", async () => {
    mockIPC(() => {
      throw { kind: "internal", message: "backend roto" };
    });
    await expect(getAppInfo()).rejects.toEqual({ kind: "internal", message: "backend roto" });
  });

  it("rechaza con kind unknown ante una forma de error desconocida", async () => {
    mockIPC(() => {
      throw 42;
    });
    await expect(getAppInfo()).rejects.toMatchObject({ kind: "unknown" });
  });
});

const SETTINGS = { uiMode: "simple", autostartTray: false, language: "es" };

describe("getSettings / setSetting", () => {
  afterEach(() => {
    clearMocks();
  });

  it("invoca exactamente get_settings y devuelve Settings tipado", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      return SETTINGS;
    });
    await expect(getSettings()).resolves.toEqual(SETTINGS);
    expect(calls).toEqual(["get_settings"]);
  });

  it("decodifica el modo avanzado", async () => {
    mockIPC(() => ({ ...SETTINGS, uiMode: "advanced" }));
    await expect(getSettings()).resolves.toMatchObject({ uiMode: "advanced" });
  });

  it("invoca set_setting con { key, value } y devuelve la configuracion", async () => {
    const calls: Array<{ cmd: string; args: unknown }> = [];
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      return { ...SETTINGS, uiMode: "advanced" };
    });
    await expect(setSetting("ui_mode", "advanced")).resolves.toMatchObject({ uiMode: "advanced" });
    expect(calls).toEqual([{ cmd: "set_setting", args: { key: "ui_mode", value: "advanced" } }]);
  });

  it("rechaza con invalid_response ante un modo desconocido o campos faltantes", async () => {
    const bad: unknown[] = [
      { ...SETTINGS, uiMode: "turbo" },
      { uiMode: "simple", language: "es" },
      { ...SETTINGS, autostartTray: "no" },
      { ...SETTINGS, language: "en" },
      null,
      "texto",
    ];
    for (const response of bad) {
      mockIPC(() => response);
      await expect(getSettings()).rejects.toMatchObject({ kind: "invalid_response" });
      await expect(setSetting("ui_mode", "simple")).rejects.toMatchObject({
        kind: "invalid_response",
      });
    }
  });

  it("propaga tal cual el error tipado del backend", async () => {
    mockIPC(() => {
      throw { kind: "setting_invalid", message: "invalid setting" };
    });
    await expect(setSetting("ui_mode", "advanced")).rejects.toEqual({
      kind: "setting_invalid",
      message: "invalid setting",
    });
  });

  it("no permite escribir claves de solo lectura (error de tipos)", async () => {
    const calls: unknown[] = [];
    mockIPC((_cmd, args) => {
      calls.push(args);
      return SETTINGS;
    });
    // @ts-expect-error autostart_tray no es escribible: no esta en WritableSettings.
    await setSetting("autostart_tray", true);
    // @ts-expect-error el valor debe ser un UiMode valido.
    await setSetting("ui_mode", "turbo");
    expect(calls).toHaveLength(2);
  });
});

describe("getStoreStatus / recoverStore", () => {
  afterEach(() => {
    clearMocks();
  });

  it("decodifica ready e invoca get_store_status", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      return { status: "ready" };
    });
    await expect(getStoreStatus()).resolves.toEqual({ status: "ready" });
    expect(calls).toEqual(["get_store_status"]);
  });

  it("decodifica unavailable con motivo y recuperabilidad", async () => {
    const corrupt = { status: "unavailable", reason: "corrupt", recoverable: true };
    mockIPC(() => corrupt);
    await expect(getStoreStatus()).resolves.toEqual(corrupt);
    const tooNew = { status: "unavailable", reason: "schema_too_new", recoverable: false };
    mockIPC(() => tooNew);
    await expect(getStoreStatus()).resolves.toEqual(tooNew);
  });

  it("rechaza con invalid_response un estado mal formado", async () => {
    const bad: unknown[] = [
      { status: "unavailable", reason: "otro", recoverable: true },
      { status: "unavailable", reason: "corrupt", recoverable: "si" },
      { status: "unavailable", reason: "corrupt" },
      { status: "broken" },
      undefined,
    ];
    for (const response of bad) {
      mockIPC(() => response);
      await expect(getStoreStatus()).rejects.toMatchObject({ kind: "invalid_response" });
    }
  });

  it("recoverStore invoca recover_store y devuelve el nombre del respaldo", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      return { backupFileName: "data.db.corrupt-1700000000000.bak" };
    });
    await expect(recoverStore()).resolves.toEqual({
      backupFileName: "data.db.corrupt-1700000000000.bak",
    });
    expect(calls).toEqual(["recover_store"]);
  });

  it("recoverStore rechaza con invalid_response si el nombre es vacio o no es string", async () => {
    for (const response of [{ backupFileName: "" }, { backupFileName: 3 }, {}, null]) {
      mockIPC(() => response);
      await expect(recoverStore()).rejects.toMatchObject({ kind: "invalid_response" });
    }
  });

  it("recoverStore propaga el error tipado del backend", async () => {
    mockIPC(() => {
      throw { kind: "store_not_recoverable", message: "x" };
    });
    await expect(recoverStore()).rejects.toMatchObject({ kind: "store_not_recoverable" });
  });
});

describe("getPrivilegeContext", () => {
  afterEach(() => {
    clearMocks();
  });

  it("invoca get_privilege_context y decodifica las cuatro formas validas", async () => {
    const calls: string[] = [];
    const valid = [
      { status: "match", elevated: true },
      { status: "match", elevated: false },
      { status: "mismatch", reason: "different_user", elevated: true },
      { status: "mismatch", reason: "undetermined", elevated: false },
    ];
    for (const shape of valid) {
      mockIPC((cmd) => {
        calls.push(cmd);
        return shape;
      });
      await expect(getPrivilegeContext()).resolves.toEqual(shape);
    }
    expect(calls).toEqual(Array(4).fill("get_privilege_context"));
  });

  it("rechaza con invalid_response las formas invalidas", async () => {
    const bad: unknown[] = [
      { status: "otro", elevated: true },
      { status: "mismatch", reason: "x", elevated: true },
      { status: "mismatch", elevated: true },
      { status: "match", elevated: "si" },
      { status: "match" },
      null,
      "match",
    ];
    for (const response of bad) {
      mockIPC(() => response);
      await expect(getPrivilegeContext()).rejects.toMatchObject({ kind: "invalid_response" });
    }
  });

  it("propaga el error tipado del backend", async () => {
    mockIPC(() => {
      throw { kind: "internal", message: "x" };
    });
    await expect(getPrivilegeContext()).rejects.toMatchObject({ kind: "internal" });
  });
});
