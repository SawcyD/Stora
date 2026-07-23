import { useEffect, useMemo, useRef } from "react";
import { InfoBar, NavigationView, type NavigationItem } from "@sawcy/memora-ui";

import {
  AboutIcon,
  AppsIcon,
  AutomationIcon,
  CleanupIcon,
  DeveloperIcon,
  DuplicatesIcon,
  FileIcon,
  HistoryIcon,
  HomeIcon,
  SettingsIcon,
  StorageIcon,
} from "./components/icons";
import { useApp, type PageId } from "./state/AppContext";

import AboutPage from "./pages/AboutPage";
import ApplicationsPage from "./pages/ApplicationsPage";
import AutomationPage from "./pages/AutomationPage";
import CleanupPage from "./pages/CleanupPage";
import ComingSoonPage from "./pages/ComingSoonPage";
import DeveloperPage from "./pages/DeveloperPage";
import DrivePressurePage from "./pages/DrivePressurePage";
import DuplicatesPage from "./pages/DuplicatesPage";
import HistoryPage from "./pages/HistoryPage";
import HomePage from "./pages/HomePage";
import LargeFilesPage from "./pages/LargeFilesPage";
import SettingsPage from "./pages/SettingsPage";
import StoragePage from "./pages/StoragePage";

const NAV_ITEMS: NavigationItem[] = [
  { id: "home", label: "Home", icon: <HomeIcon /> },
  { id: "storage", label: "Storage", icon: <StorageIcon /> },
  { id: "cleanup", label: "Cleanup", icon: <CleanupIcon /> },
  { id: "applications", label: "Applications", icon: <AppsIcon /> },
  { id: "largeFiles", label: "Large files", icon: <FileIcon /> },
  { id: "duplicates", label: "Duplicates", icon: <DuplicatesIcon /> },
  { id: "developer", label: "Developer", icon: <DeveloperIcon /> },
  { id: "drivePressure", label: "Drive pressure", icon: <StorageIcon /> },
  { id: "history", label: "History", icon: <HistoryIcon /> },
  { id: "automation", label: "Automation", icon: <AutomationIcon /> },
];

const FOOTER_ITEMS: NavigationItem[] = [
  { id: "settings", label: "Settings", icon: <SettingsIcon /> },
  { id: "about", label: "About", icon: <AboutIcon /> },
];

export default function App() {
  const {
    page,
    setPage,
    sidebarCollapsed,
    toggleSidebar,
    setSidebarCollapsed,
    notices,
    dismissNotice,
    ready,
  } = useApp();

  const contentRef = useRef<HTMLDivElement>(null);

  // Collapse the navigation automatically in narrow windows, matching the
  // behavior of Windows Settings.
  //
  // This only ever forces the collapsed state — it never re-expands, so a
  // user who collapsed a wide window keeps their choice.
  useEffect(() => {
    const query = window.matchMedia("(max-width: 820px)");
    const apply = (matches: boolean) => {
      if (matches) setSidebarCollapsed(true);
    };

    apply(query.matches);
    const listener = (event: MediaQueryListEvent) => apply(event.matches);
    query.addEventListener("change", listener);
    return () => query.removeEventListener("change", listener);
  }, [setSidebarCollapsed]);

  // Moving between pages should return the reading position to the top.
  useEffect(() => {
    contentRef.current?.scrollTo({ top: 0 });
  }, [page]);

  const body = useMemo(() => {
    switch (page) {
      case "home":
        return <HomePage />;
      case "storage":
        return <StoragePage />;
      case "cleanup":
        return <CleanupPage />;
      case "applications":
        return <ApplicationsPage />;
      case "largeFiles":
        return <LargeFilesPage />;
      case "duplicates":
        return <DuplicatesPage />;
      case "automation":
        return <AutomationPage />;
      case "developer":
        return <DeveloperPage />;
      case "drivePressure":
        return <DrivePressurePage />;
      case "history":
        return <HistoryPage />;
      case "settings":
        return <SettingsPage />;
      case "about":
        return <AboutPage />;
      default:
        return (
          <ComingSoonPage
            title="Not available"
            description="This section does not exist in this release."
          />
        );
    }
  }, [page]);

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main">
        Skip to main content
      </a>

      <NavigationView
        items={NAV_ITEMS}
        footerItems={FOOTER_ITEMS}
        selectedId={page}
        onSelect={(id) => setPage(id as PageId)}
        collapsed={sidebarCollapsed}
        onToggleCollapse={toggleSidebar}
        ariaLabel="Stora sections"
      />

      <div className="app-content">
        <main className="app-scroll" id="main" ref={contentRef} tabIndex={-1}>
          {notices.length > 0 ? (
            <div className="stack page-section" role="status" aria-live="polite">
              {notices.map((notice) => (
                <InfoBar
                  key={notice.id}
                  tone={notice.tone}
                  title={notice.title}
                  message={notice.detail}
                  onDismiss={() => dismissNotice(notice.id)}
                />
              ))}
            </div>
          ) : null}

          {ready ? body : null}
        </main>
      </div>
    </div>
  );
}
