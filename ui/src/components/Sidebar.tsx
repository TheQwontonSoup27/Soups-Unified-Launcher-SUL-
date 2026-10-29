import { BOTTOM_NAV, MODULE_NAV, TOP_NAV, type NavItem, type ViewId } from "../nav";
import { PresenceMessage, PresenceToggle } from "./PresenceToggle";

interface SidebarProps {
  active: ViewId;
  onSelect: (id: ViewId) => void;
  /** What is typed in the search box, and what to do when it changes. */
  query: string;
  onSearch: (query: string) => void;
}

export default function Sidebar({ active, onSelect, query, onSearch }: SidebarProps) {
  const renderItem = (item: NavItem) => (
    <li key={item.id}>
      <button
        type="button"
        className="nav-item"
        aria-current={item.id === active ? "page" : undefined}
        onClick={() => onSelect(item.id)}
      >
        {item.label}
      </button>
    </li>
  );

  return (
    <aside className="sidebar">
      {/* App icon, global search, and the Discord presence switch sit on one row, in that order. */}
      <div className="sidebar-top">
        <img className="app-icon" src="logo.png" alt="Soups Unified Launcher" />
        <input
          className="global-search"
          type="search"
          placeholder="Search games"
          aria-label="Search games"
          value={query}
          onChange={(e) => onSearch(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onSearch("");
          }}
        />
        <PresenceToggle />
      </div>
      <PresenceMessage />

      <nav className="nav" aria-label="Main">
        <ul className="nav-group">{TOP_NAV.map(renderItem)}</ul>
        <p className="nav-label">Roadmap modules</p>
        <ul className="nav-group">{MODULE_NAV.map(renderItem)}</ul>
        <div className="nav-spacer" />
        <ul className="nav-group">{BOTTOM_NAV.map(renderItem)}</ul>
      </nav>
    </aside>
  );
}
