import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { ConfirmDialog } from "../../shared/ui/ConfirmDialog";
import { DensityProvider, useDensity } from "../../shared/ui/density";
import { focusRing } from "../../shared/ui/focus";
import { RadioGroup } from "../../shared/ui/RadioGroup";
import { StatusBadge } from "../../shared/ui/StatusBadge";
import { Switch } from "../../shared/ui/Switch";

afterEach(cleanup);

function DensityProbe() {
  return <p data-testid="d">{useDensity()}</p>;
}

describe("density", () => {
  it("devuelve simple sin provider", () => {
    render(<DensityProbe />);
    expect(screen.getByTestId("d")).toHaveTextContent("simple");
  });
  it("devuelve advanced con provider", () => {
    render(
      <DensityProvider value="advanced">
        <DensityProbe />
      </DensityProvider>,
    );
    expect(screen.getByTestId("d")).toHaveTextContent("advanced");
  });
});

describe("focusRing", () => {
  it("usa outline visible solo con focus-visible", () => {
    expect(focusRing).toContain("focus-visible:outline-2");
    expect(focusRing).toContain("focus-visible:outline-focus");
  });
});

describe("Button", () => {
  it.each(["primary", "secondary", "danger"] as const)("expone data-variant %s", (variant) => {
    render(<Button variant={variant}>x</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("data-variant", variant);
  });
  it("es type=button por defecto, incluye focusRing y respeta disabled", () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        x
      </Button>,
    );
    const button = screen.getByRole("button");
    expect(button).toHaveAttribute("type", "button");
    expect(button).toBeDisabled();
    expect(button.className).toContain(focusRing);
    expect(button).toHaveAttribute("data-variant", "secondary");
    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });
  it("llama onClick cuando esta habilitado", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>x</Button>);
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});

describe("Card", () => {
  it("renderiza el encabezado con el nivel pedido", () => {
    render(
      <Card heading="Titulo" level={3}>
        <p>cuerpo</p>
      </Card>,
    );
    expect(screen.getByRole("heading", { level: 3, name: "Titulo" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Titulo" })).toBeInTheDocument();
  });
  it("usa nivel 2 por defecto y data-density segun el contexto con el mismo contenido", () => {
    const { container, rerender } = render(
      <Card heading="T">
        <p>cuerpo</p>
      </Card>,
    );
    expect(screen.getByRole("heading", { level: 2 })).toBeInTheDocument();
    expect(container.firstElementChild).toHaveAttribute("data-density", "simple");
    rerender(
      <DensityProvider value="advanced">
        <Card heading="T">
          <p>cuerpo</p>
        </Card>
      </DensityProvider>,
    );
    expect(container.firstElementChild).toHaveAttribute("data-density", "advanced");
    expect(screen.getByText("cuerpo")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "T" })).toBeInTheDocument();
  });
});

describe("StatusBadge", () => {
  it("muestra icono aria-hidden y el texto", () => {
    const { container } = render(<StatusBadge tone="success">Lista</StatusBadge>);
    expect(screen.getByText("Lista")).toBeInTheDocument();
    const icon = container.querySelector("svg");
    expect(icon).not.toBeNull();
    expect(icon).toHaveAttribute("aria-hidden", "true");
  });
  it("tonos distintos usan iconos distintos", () => {
    const paths = (tone: "success" | "warning" | "danger" | "neutral"): string => {
      const { container, unmount } = render(<StatusBadge tone={tone}>t</StatusBadge>);
      const html = container.querySelector("svg")?.innerHTML ?? "";
      unmount();
      return html;
    };
    const all = (["success", "warning", "danger", "neutral"] as const).map(paths);
    expect(new Set(all).size).toBe(4);
    for (const html of all) expect(html.length).toBeGreaterThan(0);
  });
});

describe("RadioGroup", () => {
  const options = [
    { value: "simple", label: "Simple", description: "d1" },
    { value: "advanced", label: "Avanzado", description: "d2" },
  ];
  it("fieldset con legend y la opcion value marcada", () => {
    render(<RadioGroup legend="Modo" name="m" value="advanced" options={options} onChange={() => {}} />);
    expect(screen.getByRole("group", { name: "Modo" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Avanzado/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: /Simple/ })).not.toBeChecked();
    expect(screen.getByRole("radio", { name: /Simple/ }).className).toContain(focusRing);
  });
  it("onChange recibe el valor al cambiar", () => {
    const onChange = vi.fn();
    render(<RadioGroup legend="Modo" name="m" value="simple" options={options} onChange={onChange} />);
    fireEvent.click(screen.getByRole("radio", { name: /Avanzado/ }));
    expect(onChange).toHaveBeenCalledWith("advanced");
  });
  it("disabled deshabilita todas las opciones y las describe", () => {
    const onChange = vi.fn();
    render(
      <RadioGroup
        legend="Modo"
        name="m"
        value="simple"
        options={options}
        onChange={onChange}
        disabled
        disabledReason="Primero recupera"
      />,
    );
    for (const radio of screen.getAllByRole("radio")) {
      expect(radio).toBeDisabled();
      expect(radio).toHaveAccessibleDescription(/Primero recupera/);
    }
  });
});

describe("Switch", () => {
  function Harness() {
    const [on, setOn] = useState(false);
    return <Switch label="Auto" checked={on} onChange={setOn} />;
  }
  it("alterna con click (Enter/Space activan click en un button nativo)", () => {
    render(<Harness />);
    const sw = screen.getByRole("switch", { name: "Auto" });
    expect(sw).toHaveAttribute("aria-checked", "false");
    fireEvent.click(sw);
    expect(sw).toHaveAttribute("aria-checked", "true");
    fireEvent.click(sw);
    expect(sw).toHaveAttribute("aria-checked", "false");
    expect(sw.tagName).toBe("BUTTON");
  });
  it("no disponible: enfocable, aria-disabled, descrito y sin onChange", () => {
    const onChange = vi.fn();
    render(<Switch label="Auto" checked={false} onChange={onChange} unavailable description="Proximamente" />);
    const sw = screen.getByRole("switch", { name: "Auto" });
    expect(sw).not.toBeDisabled();
    expect(sw).toHaveAttribute("aria-disabled", "true");
    expect(sw).toHaveAccessibleDescription("Proximamente");
    sw.focus();
    expect(sw).toHaveFocus();
    fireEvent.click(sw);
    fireEvent.keyDown(sw, { key: "Enter" });
    fireEvent.keyDown(sw, { key: " " });
    expect(onChange).not.toHaveBeenCalled();
    expect(sw).toHaveAttribute("aria-checked", "false");
  });
});

describe("ConfirmDialog", () => {
  function Harness(props: { onConfirm: () => void; onCancel: () => void; busy?: boolean }) {
    const [open, setOpen] = useState(false);
    return (
      <>
        <button type="button" onClick={() => setOpen(true)}>
          abrir
        </button>
        <ConfirmDialog
          open={open}
          title="Titulo"
          body="Texto"
          confirmLabel="Confirmar"
          cancelLabel="Cancelar"
          busy={props.busy === true}
          onConfirm={props.onConfirm}
          onCancel={() => {
            props.onCancel();
            setOpen(false);
          }}
        />
      </>
    );
  }
  const open = () => {
    const trigger = screen.getByRole("button", { name: "abrir" });
    trigger.focus();
    fireEvent.click(trigger);
    return trigger;
  };

  it("al abrir es alertdialog descrito y el foco queda en Cancelar", () => {
    render(<Harness onConfirm={vi.fn()} onCancel={vi.fn()} />);
    open();
    const dialog = screen.getByRole("alertdialog", { name: "Titulo" });
    expect(dialog).toHaveAccessibleDescription("Texto");
    expect(screen.getByRole("button", { name: "Cancelar" })).toHaveFocus();
  });
  it("Cancelar y Escape llaman onCancel, no onConfirm, y devuelven el foco al disparador", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<Harness onConfirm={onConfirm} onCancel={onCancel} />);
    let trigger = open();
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(trigger).toHaveFocus();
    trigger = open();
    const dialog = document.querySelector("dialog") as HTMLDialogElement;
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onConfirm).not.toHaveBeenCalled();
    expect(trigger).toHaveFocus();
  });
  it("Confirmar llama onConfirm una vez", () => {
    const onConfirm = vi.fn();
    render(<Harness onConfirm={onConfirm} onCancel={vi.fn()} />);
    open();
    fireEvent.click(screen.getByRole("button", { name: "Confirmar" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
  it("busy deshabilita Confirmar", () => {
    const onConfirm = vi.fn();
    render(<Harness onConfirm={onConfirm} onCancel={vi.fn()} busy />);
    open();
    const confirm = screen.getByRole("button", { name: "Confirmar" });
    expect(confirm).toBeDisabled();
    fireEvent.click(confirm);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
