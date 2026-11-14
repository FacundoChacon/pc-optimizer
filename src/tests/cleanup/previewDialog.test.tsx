import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CleanupPreviewDialog } from "../../features/cleanup/CleanupPreviewDialog";
import type { CleanupPreview, JunkCategory } from "../../lib/ipc";
import { es } from "../../shared/i18n/es";
import { format, formatBytes } from "../../shared/i18n/format";
import { preview, previewCategory } from "../support/cleanupFixtures";

afterEach(cleanup);

interface HarnessProps {
  preview?: CleanupPreview | null;
  loading?: boolean;
  busy?: boolean;
  selected?: readonly JunkCategory[];
  advanced?: boolean;
  notice?: string | null;
  onToggle?: (category: JunkCategory) => void;
  onConfirm?: () => void;
  onCancel?: () => void;
}

function Harness(props: HarnessProps) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        abrir
      </button>
      <CleanupPreviewDialog
        open={open}
        preview={props.preview === undefined ? preview() : props.preview}
        loading={props.loading === true}
        busy={props.busy === true}
        selected={props.selected ?? ["temp_user", "temp_windows"]}
        advanced={props.advanced === true}
        notice={props.notice ?? null}
        onToggle={props.onToggle ?? vi.fn()}
        onConfirm={props.onConfirm ?? vi.fn()}
        onCancel={() => {
          props.onCancel?.();
          setOpen(false);
        }}
      />
    </>
  );
}

function open(): HTMLElement {
  const trigger = screen.getByRole("button", { name: "abrir" });
  trigger.focus();
  fireEvent.click(trigger);
  return trigger;
}

const names = es.cleanup.categoryNames;
const confirmName = (count: number) => format(es.cleanup.confirmMove, { count: String(count) });

describe("CleanupPreviewDialog: apertura y contenido", () => {
  it("abre como alertdialog con el foco en Cancelar y todas las categorias marcadas", () => {
    render(<Harness />);
    open();
    const dialog = screen.getByRole("alertdialog", { name: es.cleanup.previewTitle });
    expect(screen.getByRole("button", { name: es.cleanup.cancel })).toHaveFocus();
    const boxes = within(dialog).getAllByRole("checkbox");
    expect(boxes.map((box) => (box as HTMLInputElement).checked)).toEqual([true, true]);
    expect(within(dialog).getByRole("checkbox", { name: new RegExp(names.temp_user) })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: new RegExp(names.temp_windows) })).toBeChecked();
  });

  it("muestra cantidad y tamano por categoria, el total y las notas de Papelera y de recientes", () => {
    render(<Harness />);
    open();
    const dialog = screen.getByRole("alertdialog");
    expect(
      within(dialog).getByText(format(es.cleanup.previewItemLine, { count: "3", size: formatBytes(5 * 1024 * 1024) })),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(format(es.cleanup.previewItemLine, { count: "2", size: formatBytes(2048) })),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        format(es.cleanup.previewTotal, { count: "5", size: formatBytes(5 * 1024 * 1024 + 2048) }),
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(es.cleanup.previewRecycleNote)).toBeInTheDocument();
    expect(
      within(dialog).getByText(format(es.cleanup.previewRecentNote, { count: "1", size: formatBytes(100) })),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: confirmName(5) })).toBeEnabled();
  });

  it("sin archivos recientes no muestra la nota de recientes y con un solo archivo usa el singular", () => {
    const one = preview({
      categories: [previewCategory("temp_user", { itemCount: 1, sizeBytes: 10 })],
    });
    render(<Harness preview={one} selected={["temp_user"]} />);
    open();
    expect(screen.queryByText(/24 horas/)).toBeNull();
    expect(screen.getByRole("button", { name: es.cleanup.confirmMoveOne })).toBeEnabled();
  });

  it("una categoria bloqueada queda deshabilitada con su motivo y no cuenta en el total", () => {
    const blocked = preview({
      categories: [
        {
          category: "temp_user",
          status: "blocked",
          blockReason: "different_user",
          itemCount: 0,
          sizeBytes: 0,
          recentCount: 0,
          recentBytes: 0,
        },
        previewCategory("temp_windows", { itemCount: 2, sizeBytes: 2048 }),
      ],
    });
    render(<Harness preview={blocked} selected={["temp_windows"]} />);
    open();
    const box = screen.getByRole("checkbox", { name: new RegExp(names.temp_user) });
    expect(box).toBeDisabled();
    expect(box).not.toBeChecked();
    expect(screen.getByText(es.cleanup.blockReasons.different_user)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: confirmName(2) })).toBeEnabled();
  });

  it("oculta las categorias sin archivos ni recientes", () => {
    const sparse = preview({
      categories: [
        previewCategory("temp_user", { itemCount: 1, sizeBytes: 10 }),
        previewCategory("wu_cache"),
      ],
    });
    render(<Harness preview={sparse} selected={["temp_user"]} />);
    open();
    expect(screen.queryByRole("checkbox", { name: new RegExp(names.wu_cache) })).toBeNull();
  });
});

describe("CleanupPreviewDialog: seleccion y confirmacion", () => {
  it("cada casilla avisa su categoria y confirmar llama onConfirm una sola vez", () => {
    const onToggle = vi.fn();
    const onConfirm = vi.fn();
    render(<Harness onToggle={onToggle} onConfirm={onConfirm} />);
    open();
    fireEvent.click(screen.getByRole("checkbox", { name: new RegExp(names.temp_windows) }));
    expect(onToggle).toHaveBeenCalledWith("temp_windows");
    fireEvent.click(screen.getByRole("button", { name: confirmName(5) }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("mientras llega un preview nuevo deshabilita Confirmar y lo anuncia en una region aria-live", () => {
    render(<Harness loading />);
    open();
    const confirm = screen.getByRole("button", { name: confirmName(5) });
    expect(confirm).toBeDisabled();
    const live = screen.getByText(es.cleanup.previewLoading);
    expect(live.closest("[aria-live]")).not.toBeNull();
  });

  it("anuncia el aviso de resumen actualizado dentro del dialogo", () => {
    render(<Harness notice={es.cleanup.previewUpdated} />);
    open();
    const notice = screen.getByText(es.cleanup.previewUpdated);
    expect(notice.closest("[aria-live]")).not.toBeNull();
    expect(screen.getByRole("alertdialog")).toContainElement(notice);
  });

  it("sin categorias marcadas deja Confirmar deshabilitado, explica por que y no llama onConfirm", () => {
    const onConfirm = vi.fn();
    render(<Harness selected={[]} onConfirm={onConfirm} />);
    open();
    const confirm = screen.getByRole("button", { name: confirmName(0) });
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAccessibleDescription(es.cleanup.previewNothingSelected);
    fireEvent.click(confirm);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("sin preview todavia o ocupado confirmando, Confirmar sigue deshabilitado", () => {
    const onConfirm = vi.fn();
    const { unmount } = render(<Harness preview={null} onConfirm={onConfirm} />);
    open();
    expect(screen.getByRole("button", { name: confirmName(0) })).toBeDisabled();
    unmount();
    render(<Harness busy onConfirm={onConfirm} />);
    open();
    const confirm = screen.getByRole("button", { name: confirmName(5) });
    expect(confirm).toBeDisabled();
    fireEvent.click(confirm);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});

describe("CleanupPreviewDialog: cerrar no ejecuta nada", () => {
  it("Cancelar y Escape llaman onCancel, nunca onConfirm, y devuelven el foco al disparador", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<Harness onConfirm={onConfirm} onCancel={onCancel} />);
    let trigger = open();
    fireEvent.click(screen.getByRole("button", { name: es.cleanup.cancel }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(trigger).toHaveFocus();
    trigger = open();
    const dialog = document.querySelector("dialog") as HTMLDialogElement;
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onConfirm).not.toHaveBeenCalled();
    expect(trigger).toHaveFocus();
  });
});

describe("CleanupPreviewDialog: mientras se confirma", () => {
  it("Cancelar y Escape no cierran el dialogo: la ejecucion ya fue pedida y cerrarlo dejaria de reflejarla", () => {
    const onCancel = vi.fn();
    render(<Harness busy onCancel={onCancel} />);
    open();
    const cancelButton = screen.getByRole("button", { name: es.cleanup.cancel });
    expect(cancelButton).toBeDisabled();
    fireEvent.click(cancelButton);
    const dialog = document.querySelector("dialog") as HTMLDialogElement;
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    expect(onCancel).not.toHaveBeenCalled();
    expect(dialog).toHaveAttribute("open");
  });
});

describe("CleanupPreviewDialog: aviso de Papelera deshabilitada (D-12 B)", () => {
  it("con recycleBinAvailable en false muestra una alerta y deshabilita Confirmar", () => {
    const onConfirm = vi.fn();
    render(<Harness preview={preview({ recycleBinAvailable: false })} onConfirm={onConfirm} />);
    open();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(es.cleanup.recycleBinDisabled);
    const confirm = screen.getByRole("button", { name: confirmName(5) });
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAccessibleDescription(es.cleanup.recycleBinDisabled);
    fireEvent.click(confirm);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("con recycleBinAvailable en true no hay alerta y Confirmar queda habilitado", () => {
    render(<Harness preview={preview({ recycleBinAvailable: true })} />);
    open();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText(es.cleanup.recycleBinDisabled)).toBeNull();
    expect(screen.getByRole("button", { name: confirmName(5) })).toBeEnabled();
  });
});

describe("CleanupPreviewDialog: modo avanzado", () => {
  it("muestra los recientes de cada categoria; en simple solo la nota agregada", () => {
    const { unmount } = render(<Harness advanced />);
    open();
    expect(
      screen.getByText(format(es.cleanup.previewRecentColumn, { count: "1", size: formatBytes(100) })),
    ).toBeInTheDocument();
    unmount();
    render(<Harness />);
    open();
    expect(screen.queryByText(format(es.cleanup.previewRecentColumn, { count: "1", size: formatBytes(100) }))).toBeNull();
  });
});
