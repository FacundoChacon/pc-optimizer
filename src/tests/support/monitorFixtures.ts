import type { MetricSummary, MonitorSample, MonitorSnapshot, MonitorSummary } from "../../lib/ipc";

export function metric(current: number | null, level: MetricSummary["level"] = "normal"): MetricSummary {
  return { current, avg60: current, peak: current, level };
}

export function summary(overrides: Partial<MonitorSummary> = {}): MonitorSummary {
  return {
    cpu: metric(23),
    memory: metric(62),
    diskActive: metric(12),
    volume: { usedPct: 74.6, level: "normal" },
    availability: { cpu: "ok", memory: "ok", volume: "ok", diskActive: "ok", diskIo: "ok" },
    gapsInWindow: 0,
    avgReadMicros: 1800,
    ...overrides,
  };
}

/** Muestra n (1-based): una por segundo desde 1 800 000 000 000 ms. */
export function sample(seq: number, overrides: Partial<MonitorSample> = {}): MonitorSample {
  return {
    seq,
    at: 1_800_000_000_000 + (seq - 1) * 1000,
    gapBefore: false,
    cpuPct: 23,
    memUsedBytes: 10_000_000_000,
    memTotalBytes: 16_000_000_000,
    volumeTotalBytes: 500_000_000_000,
    volumeFreeBytes: 120_000_000_000,
    diskActivePct: 12,
    diskReadBps: 1_000_000,
    diskWriteBps: 2_000_000,
    ...overrides,
  };
}

export function snapshot(samples: MonitorSample[] = [], overrides: Partial<MonitorSnapshot> = {}): MonitorSnapshot {
  return {
    subscriptionId: 1,
    intervalMs: 1000,
    windowMs: 300_000,
    samples,
    summary: samples.length === 0 ? null : summary(),
    ...overrides,
  };
}

export function sampleEvent(seq: number, sum: MonitorSummary = summary(), overrides: Partial<MonitorSample> = {}) {
  return { kind: "sample", sample: sample(seq, overrides), summary: sum } as const;
}
