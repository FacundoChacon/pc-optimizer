import "@testing-library/jest-dom/vitest";

// jsdom no implementa de forma fiable <dialog>.showModal/close: shim minimo (solo si faltan).
const dialogProto = HTMLDialogElement.prototype;
if (typeof dialogProto.showModal !== "function") {
  dialogProto.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
}
if (typeof dialogProto.close !== "function") {
  dialogProto.close = function close(this: HTMLDialogElement) {
    this.removeAttribute("open");
    this.dispatchEvent(new Event("close"));
  };
}
