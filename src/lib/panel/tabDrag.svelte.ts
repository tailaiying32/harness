// Dragging a tab along its row to put it somewhere else in that row. Press and
// move a few pixels to start; the other tabs make room as it passes their
// middles; letting go leaves it there, and Escape puts it back. A drag never
// counts as a click on the tab.
//
// Tabs that move carry `data-tab` with their id; ones that stay put (notes,
// plan) don't, so nothing lands before them.

/** How far the pointer goes before a press becomes a drag. */
const START_PX = 4;

export class TabDrag {
  /** The row's ids in the order shown while dragging; null otherwise. */
  order = $state<string[] | null>(null);
  /** The tab being dragged. */
  id = $state<string | null>(null);

  /**
   * On a tab's pointerdown. `ids` gives the row's ids as they are now;
   * `done` is told where the tab ended up, if it moved.
   */
  start(e: PointerEvent, id: string, ids: () => string[], done: (id: string, to: number) => void): void {
    const row = (e.currentTarget as HTMLElement).closest<HTMLElement>(".tabs");
    if (e.button !== 0 || !row || ids().length < 2) return;
    const x0 = e.clientX;
    let dragging = false;

    const move = (ev: PointerEvent) => {
      if (!dragging) {
        if (Math.abs(ev.clientX - x0) < START_PX) return;
        dragging = true;
        this.id = id;
        this.order = ids();
      }
      const order = this.order;
      if (!order) return;
      // Its place: before the first other tab whose middle is right of the pointer.
      const others = [...row.querySelectorAll<HTMLElement>("[data-tab]")].filter((el) => el.dataset.tab !== id);
      let to = others.findIndex((el) => {
        const box = el.getBoundingClientRect();
        return ev.clientX < box.left + box.width / 2;
      });
      if (to < 0) to = others.length;
      const next = order.filter((t) => t !== id);
      next.splice(to, 0, id);
      if (next.join("\n") !== order.join("\n")) this.order = next;
    };

    const finish = (keep: boolean) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", escape, true);
      const order = this.order;
      this.order = null;
      this.id = null;
      if (!dragging) return;
      // The click that ends a drag isn't a pick.
      const swallow = (ev: Event) => {
        ev.stopPropagation();
        ev.preventDefault();
      };
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, { capture: true }));
      if (!keep || !order) return;
      // A tab opened or closed meanwhile: the drag no longer fits the row.
      const now = ids();
      if (now.length !== order.length || now.some((t) => !order.includes(t))) return;
      const to = order.indexOf(id);
      if (to !== now.indexOf(id)) done(id, to);
    };
    const up = () => finish(true);
    const cancel = () => finish(false);
    const escape = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape" || !dragging) return;
      ev.stopPropagation();
      finish(false);
    };

    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", escape, true);
  }

  /** `list` in the drag's order while one is under way. */
  arrange<T>(list: T[], key: (item: T) => string): T[] {
    const order = this.order;
    if (!order) return list;
    const by = new Map(list.map((item) => [key(item), item]));
    if (by.size !== order.length || order.some((id) => !by.has(id))) return list;
    return order.map((id) => by.get(id)!);
  }
}
