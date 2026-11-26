// Whether the last thing the person did was a key press or a pointer press.
// A tooltip shows at once on focus only when focus came from the keyboard,
// including focus that code moves back after a mouse choice in a menu.

let keyboard = false;

if (typeof document !== "undefined") {
  document.addEventListener("keydown", () => (keyboard = true), true);
  document.addEventListener("pointerdown", () => (keyboard = false), true);
  document.addEventListener("mousedown", () => (keyboard = false), true);
}

/** True when the last input was a key press. */
export function lastInputWasKeyboard(): boolean {
  return keyboard;
}
