import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { acknowledgeEmptyRecycleBin, emptyRecycleBin, prepareEmptyRecycleBin } from "../../lib/ipc";
import { installBackend } from "./mockBackend";

afterEach(() => {
  clearMocks();
});

async function ready() {
  const request = await prepareEmptyRecycleBin();
  const ack = await acknowledgeEmptyRecycleBin(request.requestId);
  return { request, ack };
}

describe("mockBackend: vaciado de la Papelera (3 pasos)", () => {
  it("prepara, reconoce y vacia en orden, y deja la Papelera simulada en el estado posterior", async () => {
    const backend = installBackend();
    const { request, ack } = await ready();
    expect(request).toMatchObject({ requestId: 1, itemCount: 12, sizeBytes: 340_000_000 });
    expect(ack.confirmToken).not.toBe(request.requestId);
    await expect(emptyRecycleBin(request.requestId, ack.confirmToken)).resolves.toMatchObject({
      status: "emptied",
      itemsBefore: 12,
      itemsAfter: 0,
    });
    expect(backend.state.bin).toEqual({ itemCount: 0, sizeBytes: 0 });
  });

  it("rechaza vaciar sin preparar, sin reconocer, con otro token o por segunda vez", async () => {
    installBackend();
    await expect(emptyRecycleBin(1, 1)).rejects.toMatchObject({ kind: "empty_request_not_found" });
    const request = await prepareEmptyRecycleBin();
    await expect(emptyRecycleBin(request.requestId, 1)).rejects.toMatchObject({
      kind: "empty_not_acknowledged",
    });
    const second = await ready();
    await expect(
      emptyRecycleBin(second.request.requestId, second.ack.confirmToken + 1),
    ).rejects.toMatchObject({ kind: "empty_request_not_found" });
    const third = await ready();
    await emptyRecycleBin(third.request.requestId, third.ack.confirmToken);
    await expect(
      emptyRecycleBin(third.request.requestId, third.ack.confirmToken),
    ).rejects.toMatchObject({ kind: "empty_request_not_found" });
  });

  it("reconocer dos veces falla y una Papelera que cambio entre pasos dispara recycle_bin_changed", async () => {
    const backend = installBackend();
    const { request, ack } = await ready();
    await expect(acknowledgeEmptyRecycleBin(request.requestId)).rejects.toMatchObject({
      kind: "empty_request_not_found",
    });
    backend.state.bin = { itemCount: 13, sizeBytes: 340_001_000 };
    await expect(emptyRecycleBin(request.requestId, ack.confirmToken)).rejects.toMatchObject({
      kind: "recycle_bin_changed",
    });
    expect(backend.state.bin.itemCount).toBe(13);
  });

  it("una Papelera vacia o una sesion en mismatch se rechazan al preparar", async () => {
    installBackend({ bin: { itemCount: 0, sizeBytes: 0 } });
    await expect(prepareEmptyRecycleBin()).rejects.toMatchObject({ kind: "recycle_bin_already_empty" });
    clearMocks();
    installBackend({ privilege: { status: "mismatch", reason: "different_user", elevated: true } });
    await expect(prepareEmptyRecycleBin()).rejects.toMatchObject({ kind: "recycle_bin_blocked" });
  });

  it("el resultado configurado (parcial) actualiza la Papelera y respeta los fallos programados", async () => {
    const backend = installBackend({
      emptyOutcome: {
        status: "partial",
        itemsAfter: 2,
        bytesAfter: 40_000_000,
        freedBytes: 300_000_000,
        detail: "partial",
        logged: false,
      },
    });
    const { request, ack } = await ready();
    await expect(emptyRecycleBin(request.requestId, ack.confirmToken)).resolves.toMatchObject({
      status: "partial",
      logged: false,
    });
    expect(backend.state.bin).toEqual({ itemCount: 2, sizeBytes: 40_000_000 });
    backend.state.failureQueue.prepare_empty_recycle_bin = [
      { kind: "empty_request_expired", message: "x" },
    ];
    await expect(prepareEmptyRecycleBin()).rejects.toMatchObject({ kind: "empty_request_expired" });
    expect(backend.count("prepare_empty_recycle_bin")).toBe(2);
  });
});
