import type { CatalogueView } from "../lib/catalogue";

/**
 * One button per category in use, with how many games carry it, and one for every game. Pressing a category shows only the
 * games that carry it; it combines with what is typed in the sidebar's search box (a game must match both).
 */
export function CategoryChips({ view, selected, total, onSelect }: { view: CatalogueView | null; selected: string | null; total: number; onSelect: (category: string | null) => void }) {
  if (view === null || view.categories.length === 0) return null;
  return (
    <section className="card category-chips" aria-label="Filter by category">
      <h3>Categories</h3>
      <div className="row">
        <button type="button" className="btn btn-ghost" aria-pressed={selected === null} onClick={() => onSelect(null)}>
          All games ({total})
        </button>
        {view.categories.map((c) => (
          <button key={c.name} type="button" className="btn btn-ghost" aria-pressed={selected === c.name} onClick={() => onSelect(c.name)}>
            {c.name} ({c.count})
          </button>
        ))}
      </div>
    </section>
  );
}
