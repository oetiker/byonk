import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, within } from "@testing-library/svelte";
import { tick } from "svelte";
import InlineNameForm from "../src/lib/components/InlineNameForm.svelte";
import InlineConfirm from "../src/lib/components/InlineConfirm.svelte";

beforeEach(() => cleanup());

function nameForm(props = {}) {
  const onsubmit = vi.fn();
  const oncancel = vi.fn();
  render(InlineNameForm, { label: "New screen name", onsubmit, oncancel, ...props });
  const form = dom.getByRole("form", { name: "New screen name" });
  return { form, input: within(form).getByRole("textbox"), onsubmit, oncancel };
}

describe("InlineNameForm", () => {
  it("puts the focus in the input", async () => {
    const { input } = nameForm();
    await tick();
    expect(document.activeElement).toBe(input);
  });
  it("starts from the given value", () => {
    expect(nameForm({ value: "clock" }).input.value).toBe("clock");
  });
  it("Enter submits the trimmed name", async () => {
    const { input, onsubmit } = nameForm();
    await fireEvent.input(input, { target: { value: "  fresh  " } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(onsubmit).toHaveBeenCalledWith("fresh");
  });
  it("OK submits", async () => {
    const { form, input, onsubmit } = nameForm();
    await fireEvent.input(input, { target: { value: "fresh" } });
    await fireEvent.click(within(form).getByRole("button", { name: "OK" }));
    expect(onsubmit).toHaveBeenCalledWith("fresh");
  });
  it("Escape cancels", async () => {
    const { input, onsubmit, oncancel } = nameForm();
    await fireEvent.keyDown(input, { key: "Escape" });
    expect(oncancel).toHaveBeenCalled();
    expect(onsubmit).not.toHaveBeenCalled();
  });
  it("Cancel cancels", async () => {
    const { form, oncancel } = nameForm();
    await fireEvent.click(within(form).getByRole("button", { name: "Cancel" }));
    expect(oncancel).toHaveBeenCalled();
  });
  it("refuses an empty or blank name", async () => {
    const { form, input, onsubmit } = nameForm();
    expect(within(form).getByRole("button", { name: "OK" })).toBeDisabled();
    await fireEvent.input(input, { target: { value: "   " } });
    expect(within(form).getByRole("button", { name: "OK" })).toBeDisabled();
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(onsubmit).not.toHaveBeenCalled();
  });
});

describe("InlineConfirm", () => {
  function confirmForm(props = {}) {
    const onconfirm = vi.fn();
    const oncancel = vi.fn();
    render(InlineConfirm, { label: "Confirm screen deletion",
      message: "Delete local/a? This cannot be undone.", onconfirm, oncancel, ...props });
    const form = dom.getByRole("form", { name: "Confirm screen deletion" });
    return { form, onconfirm, oncancel };
  }
  it("names what it deletes and puts the focus on Delete", async () => {
    const { form } = confirmForm();
    await tick();
    expect(form.textContent).toContain("Delete local/a? This cannot be undone.");
    expect(document.activeElement).toBe(within(form).getByRole("button", { name: "Delete" }));
  });
  it("Enter on Delete confirms; Enter on Cancel does not", async () => {
    const { form, onconfirm } = confirmForm();
    await fireEvent.keyDown(within(form).getByRole("button", { name: "Cancel" }), { key: "Enter" });
    expect(onconfirm).not.toHaveBeenCalled();
    await fireEvent.keyDown(within(form).getByRole("button", { name: "Delete" }), { key: "Enter" });
    expect(onconfirm).toHaveBeenCalledTimes(1);
  });
  it("Escape and Cancel cancel", async () => {
    const { form, onconfirm, oncancel } = confirmForm();
    await fireEvent.keyDown(form, { key: "Escape" });
    await fireEvent.click(within(form).getByRole("button", { name: "Cancel" }));
    expect(oncancel).toHaveBeenCalledTimes(2);
    expect(onconfirm).not.toHaveBeenCalled();
  });
  it("uses the given confirm label", () => {
    const { form } = confirmForm({ confirmLabel: "Rename anyway" });
    expect(within(form).getByRole("button", { name: "Rename anyway" })).toBeTruthy();
  });
});
