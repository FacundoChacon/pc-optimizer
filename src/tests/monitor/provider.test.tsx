import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MonitorProvider, useMonitor } from "../../features/monitor/MonitorProvider";
import { installBackend, type MockBackend } from "../support/mockBackend";
import { sample, sampleEvent, snapshot } from "../support/monitorFixtures";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
  Object.defineProperty(document, "hidden", { configurable: true, value: false });
});

function Probe({ id = "probe" }: { id?: string }) {
  const monitor = useMonitor();
  return (
    <p data-testid={id}>
      {`${monitor.status}|${monitor.samples.map((s) => s.seq).join(",")}|stale=${String(monitor.stale)}|err=${monitor.error?.kind ?? "-"}`}
    </p>
  );
}

async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

function text(id = "probe"): string {
  return screen.getByTestId(id).textContent ?? "";
}

async function mount(backend: MockBackend, ui = <Probe />): Promise<void> {
  render(<MonitorProvider>{ui}</MonitorProvider>);
  await flush();
  expect(backend.count("start_monitor_stream")).toBeGreaterThan(0);
}

describe("MonitorProvider", () => {
  it("montar un consumidor abre una sola suscripcion y carga la ventana inicial", async () => {
    const backend = installBackend({ monitorSnapshot: snapshot([sample(1), sample(2), sample(3)]) });
    await mount(backend);
    expect(backend.count("start_monitor_stream")).toBe(1);
    expect(text()).toBe("live|1,2,3|stale=false|err=-");
  });

  it("sin consumidores no hay suscripcion", async () => {
    const backend = installBackend();
    render(<MonitorProvider>{null}</MonitorProvider>);
    await flush();
    expect(backend.count("start_monitor_stream")).toBe(0);
  });

  it("dos consumidores comparten la suscripcion y se cancela al desmontar el ultimo", async () => {
    const backend = installBackend({ monitorSnapshot: snapshot([sample(1)], { subscriptionId: 9 }) });
    function Two({ showSecond }: { showSecond: boolean }) {
      return (
        <>
          <Probe id="a" />
          {showSecond && <Probe id="b" />}
        </>
      );
    }
    const view = render(
      <MonitorProvider>
        <Two showSecond />
      </MonitorProvider>,
    );
    await flush();
    expect(backend.count("start_monitor_stream")).toBe(1);
    expect(text("a")).toBe(text("b"));
    view.rerender(
      <MonitorProvider>
        <Two showSecond={false} />
      </MonitorProvider>,
    );
    await flush();
    expect(backend.count("stop_monitor_stream")).toBe(0);
    view.unmount();
    await flush();
    const stops = backend.calls.filter((c) => c.cmd === "stop_monitor_stream");
    expect(stops).toHaveLength(1);
    expect(stops[0]?.args).toEqual({ subscriptionId: 9 });
  });

  it("las muestras del canal se agregan sin duplicar la ventana (por seq)", async () => {
    const backend = installBackend({ monitorSnapshot: snapshot([sample(1), sample(2), sample(3)]) });
    await mount(backend);
    act(() => backend.emitMonitor([sampleEvent(3), sampleEvent(4), sampleEvent(2), sampleEvent(5)]));
    expect(text()).toBe("live|1,2,3,4,5|stale=false|err=-");
  });

  it("los eventos que llegan durante el arranque se aplican sin perder ni repetir muestras", async () => {
    const backend = installBackend({
      monitorSnapshot: snapshot([sample(1), sample(2)]),
      monitorEvents: [sampleEvent(2), sampleEvent(3)],
    });
    await mount(backend);
    expect(text()).toBe("live|1,2,3|stale=false|err=-");
  });

  it("la ventana se acota a 300 muestras", async () => {
    const full = Array.from({ length: 300 }, (_, i) => sample(i + 1));
    const backend = installBackend({ monitorSnapshot: snapshot(full) });
    function Count() {
      const { samples } = useMonitor();
      return <p data-testid="n">{`${samples.length}:${samples[0]?.seq}:${samples.at(-1)?.seq}`}</p>;
    }
    await mount(backend, <Count />);
    expect(text("n")).toBe("300:1:300");
    act(() => backend.emitMonitor([sampleEvent(301)]));
    expect(text("n")).toBe("300:2:301");
  });

  it("stopped deja el monitor no disponible", async () => {
    const backend = installBackend({ monitorSnapshot: snapshot([sample(1)]) });
    await mount(backend);
    act(() => backend.emitMonitor([{ kind: "stopped", reason: "sampler_failed" }]));
    expect(text()).toContain("unavailable|");
    expect(text()).toContain("err=monitor_unavailable");
  });

  it("monitor_unavailable al suscribirse deja el estado no disponible con el error", async () => {
    const backend = installBackend({ failures: { start_monitor_stream: { kind: "monitor_unavailable", message: "x" } } });
    await mount(backend);
    expect(text()).toBe("unavailable||stale=false|err=monitor_unavailable");
  });

  it("sin muestras por 3 intervalos queda 'sin datos recientes' y se recupera con una muestra", async () => {
    const backend = installBackend({ monitorSnapshot: snapshot([sample(1)]) });
    await mount(backend);
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(text()).toContain("stale=false");
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(text()).toContain("stale=true");
    act(() => backend.emitMonitor([sampleEvent(2)]));
    expect(text()).toContain("stale=false");
  });

  it("con la ventana oculta no re-renderiza y al volver muestra lo acumulado", async () => {
    const backend = installBackend({ monitorSnapshot: snapshot([sample(1)]) });
    await mount(backend);
    Object.defineProperty(document, "hidden", { configurable: true, value: true });
    act(() => backend.emitMonitor([sampleEvent(2), sampleEvent(3)]));
    expect(text()).toBe("live|1|stale=false|err=-");
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(text()).toBe("live|1,2,3|stale=false|err=-");
  });
});
