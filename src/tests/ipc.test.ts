import { Channel } from "@tauri-apps/api/core";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  cancelScan,
  getAppInfo,
  getLastJunkScan,
  getPrivilegeContext,
  getSettings,
  getStoreStatus,
  listJunkFindings,
  recoverStore,
  type ScanEvent,
  setSetting,
  startJunkScan,
  toAppError,
} from "../lib/ipc";
import { sendChannelMessages, summary } from "./support/junkFixtures";

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

describe("escaneo de basura", () => {
  afterEach(() => {
    clearMocks();
  });

  const RAW = summary({ temp_user: { scope: "per_user", itemCount: 2, sizeBytes: 3000 } });

  it("startJunkScan invoca start_scan con kind junk y un Channel, entrega los eventos y devuelve el resumen", async () => {
    const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];
    mockIPC((cmd, args) => {
      calls.push({ cmd, args: args as Record<string, unknown> });
      const channel = (args as { onProgress: Channel<unknown> }).onProgress;
      sendChannelMessages(channel, [
        { type: "started", scanId: 1 },
        { type: "category", category: "temp_user" },
        { type: "progress", category: "temp_user", filesSeen: 256, eligibleBytes: 9 },
      ]);
      return RAW;
    });
    const events: ScanEvent[] = [];
    await expect(startJunkScan((event) => events.push(event))).resolves.toEqual(RAW);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.cmd).toBe("start_scan");
    expect(calls[0]?.args["kind"]).toBe("junk");
    expect(calls[0]?.args["onProgress"]).toBeInstanceOf(Channel);
    expect(events).toEqual([
      { type: "started", scanId: 1 },
      { type: "category", category: "temp_user" },
      { type: "progress", category: "temp_user", filesSeen: 256, eligibleBytes: 9 },
    ]);
  });

  it("startJunkScan ignora los eventos invalidos sin cortar el escaneo", async () => {
    mockIPC((_cmd, args) => {
      const channel = (args as { onProgress: Channel<unknown> }).onProgress;
      sendChannelMessages(channel, [
        { type: "progress", category: "documents", filesSeen: 1, eligibleBytes: 1 },
        { type: "teleport" },
        { type: "progress", category: "wu_cache", filesSeen: -1, eligibleBytes: 1 },
        { type: "progress", category: "wu_cache", filesSeen: 1.5, eligibleBytes: 1 },
        null,
        "texto",
        { type: "category", category: "wu_cache" },
      ]);
      return RAW;
    });
    const events: ScanEvent[] = [];
    await startJunkScan((event) => events.push(event));
    expect(events).toEqual([{ type: "category", category: "wu_cache" }]);
  });

  it("startJunkScan acepta un escaneo cancelado con categorias vacias", async () => {
    const cancelled = { ...RAW, status: "cancelled", categories: [], totalItems: 0, totalBytes: 0 };
    mockIPC(() => cancelled);
    await expect(startJunkScan(() => undefined)).resolves.toMatchObject({
      status: "cancelled",
      categories: [],
    });
  });

  it("startJunkScan propaga el error tipado del backend", async () => {
    mockIPC(() => {
      throw { kind: "scan_in_progress", message: "a scan is already in progress" };
    });
    await expect(startJunkScan(() => undefined)).rejects.toEqual({
      kind: "scan_in_progress",
      message: "a scan is already in progress",
    });
  });

  it("los decodificadores rechazan con invalid_response un resumen mal formado", async () => {
    const withCategory = (index: number, patch: Record<string, unknown>) => ({
      ...RAW,
      categories: RAW.categories.map((c, i) => (i === index ? { ...c, ...patch } : c)),
    });
    const blocked = { status: "blocked", scope: "per_user" };
    const bad: unknown[] = [
      withCategory(0, { category: "documents" }),
      withCategory(0, { category: "custom_rule" }),
      withCategory(0, { status: "exploded" }),
      withCategory(0, { scope: "everywhere" }),
      withCategory(0, blocked),
      withCategory(0, { ...blocked, blockReason: "mystery" }),
      withCategory(0, { sizeBytes: -1 }),
      withCategory(0, { itemCount: 1.5 }),
      withCategory(0, { skippedCount: "3" }),
      withCategory(0, { inaccessibleCount: -2 }),
      { ...RAW, categories: RAW.categories.slice(1) },
      { ...RAW, categories: [...RAW.categories, RAW.categories[0]] },
      { ...RAW, status: "running" },
      { ...RAW, totalBytes: -5 },
      { ...RAW, scanId: "uno" },
      { ...RAW, startedAt: 1.5 },
      { ...RAW, finishedAt: "ayer" },
      null,
      "texto",
      [],
    ];
    for (const response of bad) {
      mockIPC(() => response);
      await expect(startJunkScan(() => undefined)).rejects.toMatchObject({ kind: "invalid_response" });
      // `null` es la respuesta valida de "todavia no hay escaneos".
      if (response !== null) {
        await expect(getLastJunkScan()).rejects.toMatchObject({ kind: "invalid_response" });
      }
    }
  });

  it("acepta blocked y partial con su motivo de bloqueo", async () => {
    const response = summary({
      temp_user: { scope: "per_user", status: "blocked", blockReason: "different_user" },
      crash_dumps: { scope: "mixed", status: "partial", blockReason: "undetermined", itemCount: 1, sizeBytes: 10 },
      old_logs: { status: "unavailable" },
    });
    mockIPC(() => response);
    const decoded = await getLastJunkScan();
    expect(decoded?.categories.find((c) => c.category === "temp_user")).toMatchObject({
      status: "blocked",
      blockReason: "different_user",
    });
    expect(decoded?.categories.find((c) => c.category === "crash_dumps")).toMatchObject({
      status: "partial",
      blockReason: "undetermined",
    });
  });

  it("getLastJunkScan devuelve null cuando no hay escaneos y el resumen cuando existe", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      return null;
    });
    await expect(getLastJunkScan()).resolves.toBeNull();
    expect(calls).toEqual(["get_last_junk_scan"]);
    mockIPC(() => RAW);
    await expect(getLastJunkScan()).resolves.toEqual(RAW);
  });

  it("cancelScan invoca cancel_scan con { scanId } y propaga scan_not_found", async () => {
    const calls: Array<{ cmd: string; args: unknown }> = [];
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      return null;
    });
    await expect(cancelScan(7)).resolves.toBeUndefined();
    expect(calls).toEqual([{ cmd: "cancel_scan", args: { scanId: 7 } }]);
    mockIPC(() => {
      throw { kind: "scan_not_found", message: "scan not found: 7" };
    });
    await expect(cancelScan(7)).rejects.toMatchObject({ kind: "scan_not_found" });
  });

  it("listJunkFindings envia los argumentos y decodifica la pagina", async () => {
    const page = {
      total: 3,
      items: [
        { id: 1, path: "C:/a", sizeBytes: 10, modifiedAt: 5, skipReason: null },
        { id: 2, path: "C:/b", sizeBytes: 0, modifiedAt: null, skipReason: "reparse_point" },
      ],
    };
    const calls: Array<{ cmd: string; args: unknown }> = [];
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      return page;
    });
    await expect(listJunkFindings(4, "temp_user", 100, 100)).resolves.toEqual(page);
    expect(calls).toEqual([
      { cmd: "list_junk_findings", args: { scanId: 4, category: "temp_user", offset: 100, limit: 100 } },
    ]);
  });

  it("listJunkFindings rechaza paginas mal formadas con invalid_response", async () => {
    const item = { id: 1, path: "C:/a", sizeBytes: 10, modifiedAt: 5, skipReason: null };
    const bad: unknown[] = [
      { total: -1, items: [] },
      { total: 1, items: [{ ...item, sizeBytes: -1 }] },
      { total: 1, items: [{ ...item, path: 5 }] },
      { total: 1, items: [{ ...item, skipReason: 7 }] },
      { total: 1, items: [{ ...item, modifiedAt: "ayer" }] },
      { total: 1, items: "no" },
      { items: [] },
      null,
    ];
    for (const response of bad) {
      mockIPC(() => response);
      await expect(listJunkFindings(1, "temp_user", 0, 10)).rejects.toMatchObject({
        kind: "invalid_response",
      });
    }
  });
});
