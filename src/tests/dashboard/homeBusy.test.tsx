import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { busyTask, type BusyFlags } from "../../features/dashboard/homeBusy";
import { busyReasonText } from "../../features/dashboard/homeText";
import { es } from "../../shared/i18n/es";
import { summary as junkSummary } from "../support/junkFixtures";
import { type BackendState, installBackend, OTHER_USER } from "../support/mockBackend";

afterEach(() => {
  cleanup();
  clearMocks();
});

const IDLE: BusyFlags = {
  junkScanning: false,
  cleaning: false,
  emptyingBin: false,
  diskScanning: false,
  duplicateScanning: false,
  duplicateCleaning: false,
};

describe("busyTask", () => {
  it("sin trabajos en curso no hay motivo", () => {
    expect(busyTask(IDLE)).toBeNull();
  });

  it.each([
    ["junkScanning", "junkScan"],
    ["cleaning", "cleanup"],
    ["emptyingBin", "emptyBin"],
    ["diskScanning", "diskScan"],
    ["duplicateScanning", "duplicateScan"],
    ["duplicateCleaning", "duplicateCleanup"],
  ] as const)("%s -> %s", (flag, task) => {
    expect(busyTask({ ...IDLE, [flag]: true })).toBe(task);
  });

  it("con varios trabajos informa el primero en un orden estable", () => {
    expect(busyTask({ ...IDLE, diskScanning: true, duplicateScanning: true })).toBe("diskScan");
    expect(busyTask({ ...IDLE, junkScanning: true, duplicateCleaning: true })).toBe("junkScan");
  });
});

function gateOf(): { gate: Promise<void>; release: () => void } {
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { gate, release };
}

async function openHome(overrides: Partial<BackendState> = {}) {
  const backend = installBackend(overrides);
  render(<App />);
  await screen.findByRole("heading", { level: 1, name: es.home.title });
  return backend;
}

const nav = () => screen.getByRole("navigation", { name: es.nav.label });
const goTo = async (label: string, heading: string) => {
  fireEvent.click(within(nav()).getByRole("button", { name: label }));
  await screen.findByRole("heading", { level: 1, name: heading });
};

describe("Limpiar ahora deshabilitado con motivo", () => {
  it("con una busqueda de duplicados en curso queda deshabilitado y el motivo se lee por aria-describedby", async () => {
    const hold = gateOf();
    await openHome({ gates: { start_duplicate_scan: hold.gate } });
    await goTo(es.nav.duplicates, es.duplicates.title);
    fireEvent.click(screen.getByRole("button", { name: /Buscar (duplicados|de nuevo)/ }));
    await screen.findByRole("region", { name: es.duplicates.progressTitle });
    await goTo(es.nav.home, es.home.title);
    const button = screen.getByRole("button", { name: es.home.cleanNow });
    expect(button).toBeDisabled();
    const reason = screen.getByText(busyReasonText("duplicateScan"));
    expect(button.getAttribute("aria-describedby")?.split(" ")).toContain(reason.id);
    hold.release();
  });

  it("con una busqueda de basura en curso queda deshabilitado y vuelve a habilitarse al terminar", async () => {
    const hold = gateOf();
    await openHome({ lastScan: null, scanResult: junkSummary(), gates: { start_scan: hold.gate } });
    fireEvent.click(screen.getByRole("button", { name: es.home.cleanNow }));
    await screen.findByText(es.cleanup.progressTitle);
    await goTo(es.nav.home, es.home.title);
    expect(screen.getByRole("button", { name: es.home.cleanNow })).toBeDisabled();
    expect(screen.getByText(busyReasonText("junkScan"))).toBeInTheDocument();
    hold.release();
    await waitFor(() => expect(screen.getByRole("button", { name: es.home.cleanNow })).toBeEnabled());
    expect(screen.queryByText(busyReasonText("junkScan"))).toBeNull();
  });

  it("con la base no disponible queda deshabilitado con el motivo de la base", async () => {
    await openHome({ store: { status: "unavailable", reason: "corrupt", recoverable: true } });
    const button = screen.getByRole("button", { name: es.home.cleanNow });
    await waitFor(() => expect(button).toBeDisabled());
    const reason = screen.getByText(es.home.busy.store);
    expect(button.getAttribute("aria-describedby")?.split(" ")).toContain(reason.id);
  });

  it("en over-the-shoulder (mismatch) sin trabajos queda habilitado", async () => {
    await openHome({ privilege: OTHER_USER });
    await screen.findByRole("region", { name: es.home.cards.space.title });
    expect(screen.getByRole("button", { name: es.home.cleanNow })).toBeEnabled();
    expect(screen.queryByText(es.home.busy.store)).toBeNull();
  });

  it("sin trabajos y con la base lista esta habilitado y no muestra ningun motivo", async () => {
    await openHome();
    expect(screen.getByRole("button", { name: es.home.cleanNow })).toBeEnabled();
    expect(screen.queryByText(es.home.busy.store)).toBeNull();
  });
});
