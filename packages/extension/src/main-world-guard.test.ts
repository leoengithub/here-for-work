import { afterEach, describe, expect, it, vi } from "vitest";

import { installMainWorldFinalizationGuard, releaseMainWorldFinalizationGuard } from "./main-world-guard";

describe("main-world finalization guard", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("blocks page-world submit APIs and terminal clicks until explicit release", () => {
    document.body.innerHTML = "<form><input><button type='submit'>Submit application</button></form>";
    const form = document.querySelector<HTMLFormElement>("form")!;
    const input = document.querySelector<HTMLInputElement>("input")!;
    const button = document.querySelector<HTMLButtonElement>("button")!;
    const prototype = HTMLFormElement.prototype;
    const submitDescriptor = Object.getOwnPropertyDescriptor(prototype, "submit");
    const requestDescriptor = Object.getOwnPropertyDescriptor(prototype, "requestSubmit");
    const submit = vi.fn();
    const requestSubmit = vi.fn();
    const click = vi.fn();
    Object.defineProperty(prototype, "submit", { configurable: true, writable: true, value: submit });
    Object.defineProperty(prototype, "requestSubmit", { configurable: true, writable: true, value: requestSubmit });
    button.addEventListener("click", (event) => {
      event.preventDefault();
      click();
    });
    const pageListener = () => {
      form.submit();
      form.requestSubmit();
      button.click();
    };
    input.addEventListener("input", pageListener);
    input.addEventListener("change", pageListener);

    try {
      installMainWorldFinalizationGuard("test-token");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
      expect(submit).not.toHaveBeenCalled();
      expect(requestSubmit).not.toHaveBeenCalled();
      expect(click).not.toHaveBeenCalled();

      expect(releaseMainWorldFinalizationGuard("wrong-token")).toBe(false);
      form.submit();
      expect(submit).not.toHaveBeenCalled();

      expect(releaseMainWorldFinalizationGuard("test-token")).toBe(true);
      form.submit();
      form.requestSubmit();
      button.click();
      expect(submit).toHaveBeenCalledTimes(1);
      expect(requestSubmit).toHaveBeenCalledTimes(1);
      expect(click).toHaveBeenCalledTimes(1);
    } finally {
      if (submitDescriptor) Object.defineProperty(prototype, "submit", submitDescriptor);
      if (requestDescriptor) Object.defineProperty(prototype, "requestSubmit", requestDescriptor);
    }
  });
});
