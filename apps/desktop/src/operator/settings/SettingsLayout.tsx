import { useTranslation } from "react-i18next";
import { NavLink, Outlet } from "react-router-dom";

const TABS = [
  { to: "outputs", key: "settings.outputs.title" },
  { to: "bible", key: "settings.bible.title" },
  { to: "remote", key: "settings.remote.title" },
  { to: "library", key: "settings.library.title" },
  { to: "language", key: "settings.language.title" },
  { to: "updates", key: "settings.updates.title" },
] as const;

/** Settings shell: section list on the left, the active section on the right. */
export function SettingsLayout() {
  const { t } = useTranslation();
  return (
    <div className="songs-page">
      <aside className="panel panel--left" aria-label={t("nav.settings")}>
        <h2 className="subhead">{t("nav.settings")}</h2>
        <nav className="song-list" aria-label={t("nav.settings")}>
          {TABS.map((x) => (
            <NavLink key={x.to} to={`/operator/settings/${x.to}`} className="song-row">
              <span className="song-row__title">{t(x.key)}</span>
            </NavLink>
          ))}
        </nav>
      </aside>
      <main className="center">
        <Outlet />
      </main>
    </div>
  );
}
