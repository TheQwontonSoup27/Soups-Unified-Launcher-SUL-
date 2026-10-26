import { BOTTOM_NAV, MODULE_NAV, TOP_NAV, type NavItem, type ViewId } from "../nav";
import { PresenceMessage, PresenceToggle } from "./PresenceToggle";

interface SidebarProps {
  active: ViewId;
  onSelect: (id: ViewId) => void;
}

export default function Sidebar({ active, onSelect }: SidebarProps) {
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
        <input className="global-search" type="search" placeholder="Search" aria-label="Global search" />
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
