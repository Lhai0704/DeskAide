export class AvatarInteraction {
  private start: { x: number; y: number } | null = null;
  dragging = false;
  get active() {
    return this.start !== null;
  }
  down(x: number, y: number) {
    this.start = { x, y };
    this.dragging = false;
  }
  move(x: number, y: number) {
    if (!this.start || this.dragging) return false;
    if (Math.hypot(x - this.start.x, y - this.start.y) <= 5) return false;
    this.dragging = true;
    return true;
  }
  up() {
    const click = !!this.start && !this.dragging;
    this.cancel();
    return click;
  }
  cancel() {
    this.start = null;
    this.dragging = false;
  }
}
