import { useRemote } from "../state/RemoteContext";
import { BibleView } from "./Bible";
import { Header, MiniPreview, OfflineBanner } from "./common";
import { useT } from "../lib/i18n";

/** Pastor role: Bible search and reader only, plus a small look at what is live. */
export function Pastor() {
  const t = useT();
  const { session, status } = useRemote();
  const view = session.view;
  return (
    <div className="screen">
      <Header
        title={t("bible.title")}
        status={status}
        serverName={session.serverName}
        role={session.role}
      />
      <OfflineBanner status={status} />
      {view && (
        <div className="pad-x compact">
          <MiniPreview
            cue={view.live}
            kind="live"
            blackout={view.blackout}
            clear={view.clear}
            logo={view.logo}
          />
        </div>
      )}
      <div className="screen-body">
        <BibleView role="pastor" />
      </div>
    </div>
  );
}
