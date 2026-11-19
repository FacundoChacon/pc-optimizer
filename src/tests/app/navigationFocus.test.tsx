import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { type NavigateOptions, NavigationProvider, useNavigation } from "../../app/providers/NavigationProvider";

afterEach(cleanup);

function Harness({ options }: { options: NavigateOptions | undefined }) {
  const { current, navigate } = useNavigation();
  return (
    <>
      <button type="button" onClick={() => navigate("cleanup", options)}>
        ir
      </button>
      <main id="main" tabIndex={-1}>
        {current}
      </main>
    </>
  );
}

function renderHarness(options?: NavigateOptions) {
  render(
    <NavigationProvider>
      <Harness options={options} />
    </NavigationProvider>,
  );
  const trigger = screen.getByRole("button", { name: "ir" });
  trigger.focus();
  fireEvent.click(trigger);
  return screen.getByRole("main");
}

describe("NavigationProvider.navigate: opcion de foco", () => {
  it("con focus none cambia de pantalla sin mover el foco a main", () => {
    const main = renderHarness({ focus: "none" });
    expect(main).toHaveTextContent("cleanup");
    expect(main).not.toHaveFocus();
  });

  it("sin opciones el foco pasa a main", () => {
    const main = renderHarness();
    expect(main).toHaveTextContent("cleanup");
    expect(main).toHaveFocus();
  });

  it("con focus main explicito el foco pasa a main", () => {
    expect(renderHarness({ focus: "main" })).toHaveFocus();
  });
});
