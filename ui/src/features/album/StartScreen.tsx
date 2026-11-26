import { Button, IconButton, MiddleEllipsis, Tooltip } from "../../controls";
import { Icon } from "../../icons";
import { albumText } from "../../text/album";
import { useAlbumStore } from "./session";

/** Shown while no album is open (docs/04-browsing.md, "Start screen"). */
export function StartScreen() {
  const recent = useAlbumStore((s) => s.recent);
  const pick = useAlbumStore((s) => s.pickFolder);
  const open = useAlbumStore((s) => s.openFolder);
  const remove = useAlbumStore((s) => s.removeRecent);

  return (
    <div className="ow-start">
      <h1 className="ow-start__heading">{albumText.startHeading}</h1>
      <p className="ow-start__intro">{albumText.startIntro}</p>
      <div>
        <Button kind="primary" icon="folder" onClick={() => void pick()}>
          {albumText.openFolder}
        </Button>
      </div>
      {recent.length > 0 && (
        <section className="ow-start__recent" aria-labelledby="ow-recent-heading">
          <h2 id="ow-recent-heading" className="ow-start__subheading">
            {albumText.recentHeading}
          </h2>
          <ul className="ow-recent">
            {recent.map((r) => (
              <li key={r.root} className={r.exists ? "ow-recent__item" : "ow-recent__item ow-recent__item--gone"}>
                {r.exists ? (
                  <button type="button" className="ow-recent__open" onClick={() => void open(r.root)}>
                    <Icon name="folder" />
                    <MiddleEllipsis text={r.root} />
                  </button>
                ) : (
                  <Tooltip text={albumText.notFound}>
                    <button type="button" className="ow-recent__open" aria-disabled="true">
                      <Icon name="folder" />
                      <span className="ow-recent__path">{r.root}</span>
                      <span className="ow-recent__missing">{albumText.notFound}</span>
                    </button>
                  </Tooltip>
                )}
                <IconButton icon="x" label={albumText.removeRecent} onClick={() => void remove(r.root)} />
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
