import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button, Chip, Modal, SwitchRow } from "./index";

describe("ui components", () => {
  it("Button forwards clicks and variants", () => {
    const onClick = vi.fn();
    render(
      <Button variant="primary" onClick={onClick}>
        Go
      </Button>,
    );
    const b = screen.getByRole("button", { name: "Go" });
    expect(b).toHaveClass("hs-btn", "hs-btn--primary");
    fireEvent.click(b);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("Chip exposes aria-pressed", () => {
    render(<Chip pressed>KJV</Chip>);
    expect(screen.getByRole("button", { name: "KJV" })).toHaveAttribute("aria-pressed", "true");
  });

  it("SwitchRow reports the new value", () => {
    const onChange = vi.fn();
    render(<SwitchRow label="Copyright" checked={false} onChange={onChange} />);
    fireEvent.click(screen.getByLabelText("Copyright"));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("Modal closes on Escape and traps Tab", () => {
    const onClose = vi.fn();
    render(
      <Modal title="Dialogo" onClose={onClose} closeLabel="Cerrar">
        <button>uno</button>
        <button>dos</button>
      </Modal>,
    );
    const dlg = screen.getByRole("dialog", { name: "Dialogo" });
    const last = screen.getByRole("button", { name: "dos" });
    last.focus();
    fireEvent.keyDown(dlg, { key: "Tab" });
    expect(screen.getByRole("button", { name: "Cerrar" })).toHaveFocus();
    fireEvent.keyDown(dlg, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
