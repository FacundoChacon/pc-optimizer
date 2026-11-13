import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { getAppInfo, toAppError } from "../lib/ipc";

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
